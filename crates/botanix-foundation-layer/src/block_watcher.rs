//! Asynchronous block watcher for monitoring Bitcoin block confirmations.
//!
//! This module provides the [`BlockWatcher`] component, which runs as a
//! background task and monitors Bitcoin blocks by querying `bitcoind`. It
//! serves as the bridge between the Foundation layer and the Bitcoin network,
//! ensuring that referenced blocks are actually confirmed on-chain.
//!
//! # Architecture
//!
//! The [`BlockWatcher`] communicates with the [`FoundationTask`] via two
//! message-passing channels:
//!
//! - **Inbound** ([`ToBlockWatchMsg`]): Receives requests to watch or stop
//!   watching specific block hashes.
//!
//! - **Outbound** ([`FromBlockWatchMsg`]): Sends notifications when watched
//!   blocks are observed, or periodically sends the current best block as a
//!   heartbeat, even if unchanged.
//!
//! # Backpressure
//!
//! Outbound notifications use blocking async sends. If the [`FoundationTask`]
//! is slow to consume messages, the [`BlockWatcher`] will pause until the
//! channel has capacity. This ensures no notifications are dropped and applies
//! appropriate backpressure when the consumer is overwhelmed.
//!
//! # Watchlist Behavior
//!
//! Block hashes are tracked in an LRU cache and unobserved hashes are
//! periodically re-checked against `bitcoind`. Once a block is observed, it
//! stops being re-checked but remains in the watchlist, allowing for idempotent
//! notifications if queried again. A block hash is removed from the watchlist
//! when:
//!
//! - The [`FoundationTask`] explicitly requests its removal
//! - It exceeds [`WATCHLIST_MAX_CHECKS`] failed observation attempts
//! - It gets evicted by the LRU cache due to capacity limits

#[allow(unused)]
use crate::foundation::FoundationTask; // Imported for doc comments
use crate::foundation::{BestBlock, CONF_DEPTH};
use botanix_btc_wallet::{
    error::BitcoindAdapterError, fallback::FallbackBitcoindClient,
};
use botanix_tem::foundation::bitcoin::BlockHash;
use lru::LruCache;
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{self, mpsc::error::SendError},
    time,
};

/// Interval between periodic re-checks of unobserved block hashes.
const WATCHLIST_RECHECK: Duration = Duration::from_secs(20);
/// Maximum number of attempts to observe a block hash before giving up.
const WATCHLIST_MAX_CHECKS: usize = 10;

/// Messages sent from the [`FoundationTask`] to the [`BlockWatcher`].
pub(crate) enum ToBlockWatchMsg {
    /// Request to start watching a block hash for confirmation.
    InsertBlock { block_hash: BlockHash },
    /// Request to stop watching a block hash (e.g., after finalization).
    RemoveBlock { block_hash: BlockHash },
}

/// Messages sent from the [`BlockWatcher`] to the [`FoundationTask`].
pub(crate) enum FromBlockWatchMsg {
    /// A watched block hash has been confirmed in `bitcoind`.
    Observed { block_hash: BlockHash },
    /// The current best block has been discovered or updated.
    //
    // TODO: Technically not needed; All blocks should be treated as "Observed"
    // and then let the FoundationTask decide which is the best (according to
    // height).
    Best { block: BestBlock },
    /// An error occurred while communicating with `bitcoind`.
    ///
    /// The [`BlockWatcher`] continues running after sending this; the
    /// [`FoundationTask`] can log, alert, or take corrective action.
    BitcoindError { error: BitcoindAdapterError },
}

/// Tracks the observation state of a block hash in the watchlist.
#[derive(Debug, Clone, Copy)]
struct WatchEntry {
    /// Whether the block has been confirmed in `bitcoind`.
    observed: bool,
    /// Number of observation attempts made so far.
    attempts: usize,
}

impl WatchEntry {
    /// Creates a new unobserved entry with zero attempts.
    fn unobserved() -> Self {
        WatchEntry { observed: false, attempts: 0 }
    }
}

/// Errors that can occur during block watcher operation.
enum Error {
    /// An error occurred while communicating with `bitcoind`.
    Bitcoind(BitcoindAdapterError),
    /// The [`FoundationTask`] dropped its channel, indicating shutdown.
    Shutdown,
}

impl From<BitcoindAdapterError> for Error {
    fn from(err: BitcoindAdapterError) -> Self {
        Error::Bitcoind(err)
    }
}

impl<T> From<SendError<T>> for Error {
    fn from(_: SendError<T>) -> Self {
        Error::Shutdown
    }
}

/// Monitors Bitcoin blocks by querying `bitcoind` and notifies the Foundation
/// layer.
///
/// The `BlockWatcher` maintains a watchlist of block hashes that need to be
/// confirmed on the Bitcoin chain. It periodically queries `bitcoind` to check
/// if watched blocks exist and notifies the [`FoundationTask`] when they are
/// observed.
///
/// This component should be run as a separate async task and communicates with
/// the `FoundationTask` via message-passing channels.
pub(crate) struct BlockWatcher {
    /// LRU cache of block hashes being watched, with their observation state.
    watchlist: LruCache<BlockHash, WatchEntry>,
    /// The most recent best block known from `bitcoind`.
    best_block: BestBlock,
    /// Channel for sending notifications to the [`FoundationTask`].
    to_foundation: sync::mpsc::Sender<FromBlockWatchMsg>,
    /// Channel for receiving requests from the [`FoundationTask`].
    from_foundation: sync::mpsc::Receiver<ToBlockWatchMsg>,
    /// Client for querying `bitcoind` RPC endpoints.
    bitcoind_factory: Arc<FallbackBitcoindClient>,
}

impl BlockWatcher {
    /// Creates a new block watcher with its communication channels.
    ///
    /// Initializes the watchlist with the provided block hashes, typically
    /// unconfirmed blocks from the Foundation layer's persisted state, and sets
    /// up bidirectional communication channels with the [`FoundationTask`].
    ///
    /// # Arguments
    ///
    /// * `init` - Initial block hashes to watch. These are added to the
    ///   watchlist as unobserved entries and immediately checked on the first
    ///   [`Self::run`] execution.
    /// * `best_block` - The current best block known to the Foundation layer.
    /// * `bitcoind_factory` - Client for querying `bitcoind` RPC endpoints.
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// - The [`BlockWatcher`] instance, with a call to [`Self::run`] to start
    ///   the event loop
    /// - A sender for the [`FoundationTask`] to send messages to the watcher
    /// - A receiver for the [`FoundationTask`] to receive notifications from
    ///   the watcher
    pub fn new(
        init: &[BlockHash],
        best_block: BestBlock,
        bitcoind_factory: Arc<FallbackBitcoindClient>,
    ) -> (
        Self,
        sync::mpsc::Sender<ToBlockWatchMsg>,
        sync::mpsc::Receiver<FromBlockWatchMsg>,
    ) {
        let mut watchlist = LruCache::new(
            (CONF_DEPTH as usize * 2)
                .try_into()
                .expect("LruCache size must not be zero"),
        );

        // Setup the initial watchlist
        for block_hash in init {
            watchlist.put(*block_hash, WatchEntry::unobserved());
        }

        let (tx, from_foundation) = sync::mpsc::channel(100);
        let (to_foundation, recv) = sync::mpsc::channel(100);

        let this = BlockWatcher {
            watchlist,
            best_block,
            to_foundation,
            from_foundation,
            bitcoind_factory,
        };

        (this, tx, recv)
    }
    /// Runs the block watcher event loop.
    ///
    /// This is the main entry point that drives the block watcher. It handles
    /// two types of events concurrently:
    ///
    /// - **Messages from [`FoundationTask`]**: Block hashes to watch or remove.
    ///   When a new block hash arrives via [`ToBlockWatchMsg::InsertBlock`], it
    ///   is observed immediately. When [`ToBlockWatchMsg::RemoveBlock`]
    ///   arrives, the block hash is removed from the watchlist.
    ///
    /// - **Periodic tick** (every [`WATCHLIST_RECHECK`]): Re-checks all pending
    ///   block hashes in the watchlist via [`Self::_check_watchlist`], and
    ///   discovers the current best block via [`Self::_discover_blocks`].
    ///
    /// Outbound notifications block until the [`FoundationTask`] consumes them,
    /// applying backpressure if the receiver is slow. This guarantees no
    /// notifications are dropped.
    ///
    /// # Error Handling
    ///
    /// - **`bitcoind` errors**: Forwarded to the [`FoundationTask`] via
    ///   [`FromBlockWatchMsg::BitcoindError`], then the loop continues. This
    ///   allows the `FoundationTask` to log, alert, or take corrective action
    ///   while the watcher remains operational.
    ///
    /// - **Consumer dropped**: The loop exits when the [`FoundationTask`] drops
    ///   its channel, indicating shutdown.
    pub async fn run(mut self) {
        let mut interval = time::interval(WATCHLIST_RECHECK);
        let mut buf_recheck = Vec::with_capacity(self.watchlist.len());

        loop {
            match self
                ._do_run(&mut interval, &mut buf_recheck)
                .await
                .unwrap_err()
            {
                // If the FoundationTask dropped its channel, then exit.
                Error::Shutdown => return,
                // Notify the FoundationTask about any `bitcoind` related
                // issues, then continue as usual.
                Error::Bitcoind(error) => {
                    if self
                        .to_foundation
                        .send(FromBlockWatchMsg::BitcoindError { error })
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
        }

        unreachable!()
    }
    /// Inner event loop; see [`Self::run`] for details.
    async fn _do_run(
        &mut self,
        interval: &mut time::Interval,
        buf_recheck: &mut Vec<BlockHash>,
    ) -> Result<(), Error> {
        loop {
            tokio::select! {
                msg = self.from_foundation.recv() => {
                    let Some(msg) = msg else {
                        // FoundationTask has dropped, exit the loop.
                        return Err(Error::Shutdown);
                    };

                    match msg {
                        ToBlockWatchMsg::InsertBlock { block_hash } => {
                            self._observe_block_hash(block_hash).await?;
                        }
                        ToBlockWatchMsg::RemoveBlock { block_hash } => {
                            self.watchlist.pop(&block_hash);
                        }
                    }
                }
                // Periodic re-check.
                _ = interval.tick() => {
                    // Check pending block hashes.
                    self._check_watchlist(buf_recheck).await?;

                    // Check best block hashes.
                    self._discover_blocks().await?;
                }
            }
        }

        unreachable!();
    }
    /// Re-checks all unobserved block hashes in the watchlist.
    ///
    /// Iterates through the watchlist and calls [`Self::_observe_block_hash`]
    /// for each block that has not yet been confirmed in `bitcoind`. This
    /// allows pending blocks to eventually be observed after they appear on the
    /// Bitcoin chain.
    async fn _check_watchlist(
        &mut self,
        buf_recheck: &mut Vec<BlockHash>,
    ) -> Result<(), Error> {
        // Collect unobserved hashes into a buffer first, since
        // `_observe_block_hash` requires `&mut self` (lifetime conflict). We
        // reuse the buffer across ticks to avoid repeated allocations.
        buf_recheck.extend(
            self.watchlist
                .iter() //.
                .filter(|(_, v)| !v.observed)
                .map(|(k, _)| *k),
        );

        // Check each unobserved block.
        for block_hash in buf_recheck.drain(..) {
            self._observe_block_hash(block_hash).await?;
        }

        debug_assert!(buf_recheck.is_empty());
        Ok(())
    }
    /// Discovers and tracks the current best block from `bitcoind`.
    ///
    /// Queries `bitcoind` for the best block hash and compares it against the
    /// cached value. The behavior depends on whether the best block changed:
    ///
    /// - **Unchanged**: Sends a notification to the [`FoundationTask`] with the
    ///   current best block. This is idempotent and serves as a heartbeat.
    ///
    /// - **Changed**: Fetches the new block's header and height, updates the
    ///   internal cache, registers the block hash for observation, and notifies
    ///   the [`FoundationTask`] of the new best block.
    async fn _discover_blocks(&mut self) -> Result<(), Error> {
        // TODO: Would be nice if we had the `getchaintips` endpoint.
        let block_hash = self.bitcoind_factory.get_best_block_hash_rpc()?;

        // Check whether the fetched best block remains unchanged.
        if block_hash == self.best_block.block_hash {
            // Notify the FoundationTask about the best block (again).
            self //.
                .to_foundation
                .send(FromBlockWatchMsg::Best { block: self.best_block })
                .await?;

            return Ok(());
        } else {
            // Also treat this block as a new unobserved block.
            self._observe_block_hash(block_hash).await?;
        }

        let block = {
            // TODO: The header can technically be reconstructed based on the
            // `block_info` type.
            let header =
                self.bitcoind_factory.get_block_header_rpc(&block_hash)?;

            let height =
                self.bitcoind_factory.get_block_info_rpc(&block_hash)?.height
                    as u64;

            BestBlock { block_hash, header, height }
        };

        // Update cached best block.
        self.best_block = block;

        // Notify the FoundationTask about the best block.
        self //.
            .to_foundation
            .send(FromBlockWatchMsg::Best { block })
            .await?;

        Ok(())
    }
    // TODO: We don't have the `gettransaction` endpoint.
    async fn _discover_txid(&mut self) -> Result<(), Error> {
        todo!()
    }
    /// Observes a block hash by checking whether it exists in `bitcoind`.
    ///
    /// This method tracks block hashes in a watchlist and queries `bitcoind`
    /// to confirm their existence. The behavior depends on the block's state:
    ///
    /// - **Already observed**: Sends a notification to the `FoundationTask`
    ///   again. This is idempotent and safe to call multiple times.
    ///
    /// - **Not yet observed**: Queries `bitcoind` for the block. If found,
    ///   marks it as observed and notifies the `FoundationTask`. If not found,
    ///   increments the attempt counter for retry on the next call.
    ///
    /// - **Max attempts exceeded**: After [`WATCHLIST_MAX_CHECKS`] failed
    ///   attempts, the block hash is removed from the watchlist. The
    ///   `FoundationTask` is not notified, effectively treating the block as
    ///   invalid or unavailable.
    async fn _observe_block_hash(
        &mut self,
        block_hash: BlockHash,
    ) -> Result<(), Error> {
        let v = self.watchlist.get_or_insert_mut(
            block_hash,
            // Initial value.
            || WatchEntry::unobserved(),
        );

        if v.observed {
            // Notify the FoundationTask about the observed block (again).
            let _ = self
                .to_foundation
                .try_send(FromBlockWatchMsg::Observed { block_hash });

            return Ok(());
        }

        if v.attempts >= WATCHLIST_MAX_CHECKS {
            // Clear timed-out block hash.
            self.watchlist.pop(&block_hash);
            return Ok(());
        }

        // Increment block information request attempts.
        v.attempts = v.attempts + 1;

        // Request information on the block.
        let _res = self.bitcoind_factory.get_block_info_rpc(&block_hash)?;

        // Block hash observed, notify the FoundationTask. NOTE that it
        // remains in the watchlist, but will no longer be checked by
        // querying `bitcoind`.
        v.observed = true;

        self.to_foundation
            .send(FromBlockWatchMsg::Observed { block_hash })
            .await?;

        Ok(())
    }
}
