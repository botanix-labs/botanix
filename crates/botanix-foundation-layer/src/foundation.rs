//! Foundation task for managing the Botanix Foundation layer state machine.
//!
//! This module provides the [`FoundationTask`], which orchestrates state
//! transitions for the Foundation layer. It processes proposals, validates
//! consensus payloads, and commits finalized state to persistent storage.
//!
//! # Architecture
//!
//! The Foundation task communicates with other components via message-passing:
//!
//! - **[`ProposalHandle`]**: Cloneable handle for creating and validating
//!   proposals. Used by validators during block production and verification.
//!
//! - **[`FinalizationHandle`]**: Exclusive (non-cloneable) handle for
//!   committing accepted proposals. Owned by the consensus finalization
//!   mechanism to ensure sequential commits.
//!
//! - **[`BlockWatcher`]**: Background task that monitors Bitcoin blocks via
//!   `bitcoind` and notifies the Foundation task when blocks are confirmed.
//!
//! # Lifecycle
//!
//! 1. Create the task via [`FoundationTask::new`], which returns the task
//!    instance along with [`ProposalHandle`] and [`FinalizationHandle`].
//!
//! 2. Start the event loop via [`FoundationTask::run`], which spawns the
//!    [`BlockWatcher`] and begins processing requests.
//!
//! 3. The task runs until the [`FinalizationHandle`] is dropped (graceful
//!    shutdown) or the [`BlockWatcher`] fails (unexpected shutdown).

use crate::{
    block_watcher::{BlockWatcher, FromBlockWatchMsg, ToBlockWatchMsg},
    db_impl::{WBotanixDatabaseProvider, WBotanixProviderFactory},
    payload::ConsensusPayload,
    Error,
};
use botanix_btc_wallet::fallback::FallbackBitcoindClient;
use botanix_storage::{
    BotanixDatabaseProviderRO, BotanixProviderFactory,
    DatabaseProviderFactoryRO, FoundationLayerReader,
};
use botanix_tem::foundation::{
    self,
    bitcoin::{self, BlockHash, Txid},
    proof::{AuxEvent, FoundationStateProof, FoundationStateRoot},
    CheckedFoundationProof, CommitmentsDraft, DatabaseError, Foundation,
    ProposalEntry, ValidationError,
};
use reth_db::Database;
use reth_node_types::NodeTypes;
use reth_provider::{providers::NodeTypesForProvider, ProviderError};
use std::{collections::VecDeque, sync::Arc};
use tokio::sync;

/// The confirmation depth at which a Bitcoin block is considered finalized.
pub const CONF_DEPTH: u64 = 18;

/// The result of executing consensus payloads on the Foundation layer.
///
/// This struct is returned by both validation and finalization operations:
///
/// - **Validation**: When validating a proposal, the payloads are applied and
///   then rolled back. The returned state represents what the Foundation layer
///   would look like if the proposal were finalized, but no persistent changes
///   are made.
/// - **Finalization**: When finalizing an accepted proposal, the payloads are
///   applied and committed to persistent storage. The returned state reflects
///   the new committed state of the Foundation layer.
pub struct FoundationOutcome {
    /// Post-execution root of the Foundation layer.
    pub root: FoundationStateRoot,
    /// Post-execution state of the Foundation layer.
    pub state: FoundationStateProof,
}

/// The result of creating a new proposal on the Foundation layer.
///
/// Contains the projected state after applying the proposal's payloads, along
/// with the payloads themselves. The state is not yet finalized and represents
/// what the Foundation layer would look like if this proposal were accepted.
pub struct ProposalOutcome {
    /// Projected post-execution root if this proposal is finalized.
    pub root: FoundationStateRoot,
    /// Projected post-execution state proof if this proposal is finalized.
    pub state: FoundationStateProof,
    /// The consensus payloads included in this proposal.
    pub payloads: Vec<ConsensusPayload>,
}

/// Internal message type for communicating with the proposer task.
///
/// These messages are sent through a channel to the Foundation task's proposer
/// component, which processes them sequentially to ensure state consistency.
enum ProposerMessage {
    /// Request to create a new proposal with optional pegout and replacement.
    CreateProposal {
        /// Optional pegout proposal to include in the block.
        pegout_proposal: Option<ProposalEntry>,
        /// Optional transaction ID of a previous proposal to replace.
        replacing_proposal: Option<Txid>,
        /// Channel to send the proposal result back to the caller.
        callback: sync::oneshot::Sender<Result<ProposalOutcome, Error>>,
    },
    /// Request to validate a set of consensus payloads without finalizing.
    ValidateProposal {
        /// The consensus payloads to validate.
        payloads: Vec<ConsensusPayload>,
        /// Channel to send the validation result back to the caller.
        callback: sync::oneshot::Sender<Result<FoundationOutcome, Error>>,
    },
}

/// Internal message type for communicating with the finalizer task.
///
/// When a proposal has been accepted by consensus, a finalization message is
/// sent to commit the changes to the Foundation layer's persistent state.
struct FinalizerMessage {
    /// The consensus payloads to finalize and commit.
    payloads: Vec<ConsensusPayload>,
    /// Channel to send the finalization result back to the caller.
    callback: sync::oneshot::Sender<Result<FoundationOutcome, Error>>,
}

/// Handle for submitting proposal requests to the Foundation task.
///
/// This type implements `Clone`, allowing multiple components to submit
/// proposals concurrently. Proposals are validated but not finalized until a
/// corresponding finalization request is processed.
#[derive(Debug, Clone)]
pub struct ProposalHandle {
    queue: sync::mpsc::Sender<ProposerMessage>,
}

impl ProposalHandle {
    /// Validates consensus payloads against the current Foundation state.
    ///
    /// This method applies the payloads to compute the resulting state root and
    /// proof. Any changes are rolled back and not persisted
    ///
    /// This method is used to verify that a proposal from another validator
    /// would produce a valid state transition before voting to accept it in
    /// consensus.
    ///
    /// # Arguments
    ///
    /// * `payloads` - The consensus payloads to validate. Must include a
    ///   [`ConsensusPayload::FoundationRoot`] as the final element.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Shutdown`] if the Foundation task has shut down.
    pub async fn validate_proposal(
        &self,
        payloads: Vec<ConsensusPayload>,
    ) -> Result<FoundationOutcome, Error> {
        let (callback, response) = sync::oneshot::channel();

        let msg = ProposerMessage::ValidateProposal { payloads, callback };

        self.queue.send(msg).await.map_err(|_| Error::Shutdown)?;
        response.await.map_err(|_| Error::Shutdown)?
    }

    /// Creates a new block proposal with optional pegout and replacement.
    ///
    /// This method constructs a proposal by gathering pending operations from
    /// the Foundation layer, optionally including a pegout transaction and/or
    /// replacing a previous proposal (for the UTXO-reuse mandate). Any changes
    /// are rolled back and not persisted.
    ///
    /// This method is used by the block producer and the returned
    /// [`ProposalOutcome`] contains the projected state and the consensus
    /// payloads that would need to be finalized to apply the changes.
    ///
    /// # Arguments
    ///
    /// * `pegout_proposal` - Optional pegout entry to include in the proposal.
    /// * `replacing_proposal` - Optional Txid of a previous proposal to
    ///   replace.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Shutdown`] if the Foundation task has shut down.
    pub async fn create_proposal(
        &self,
        pegout_proposal: Option<ProposalEntry>,
        replacing_proposal: Option<Txid>,
    ) -> Result<ProposalOutcome, Error> {
        let (callback, response) = sync::oneshot::channel();

        let msg = ProposerMessage::CreateProposal {
            pegout_proposal,
            replacing_proposal,
            callback,
        };

        self.queue.send(msg).await.map_err(|_| Error::Shutdown)?;
        response.await.map_err(|_| Error::Shutdown)?
    }
}

/// Handle for submitting finalization requests to the Foundation task.
///
/// Unlike [`ProposalHandle`], this type intentionally does not implement
/// `Clone`. This ensures exclusive ownership, allowing the consensus
/// finalization mechanism to monopolize finalization requests.
#[derive(Debug)]
pub struct FinalizationHandle {
    /* NOTE: This type should NOT implement Clone! */
    queue: sync::mpsc::Sender<FinalizerMessage>,
}

impl FinalizationHandle {
    /// Finalizes consensus payloads and commits them to persistent storage.
    ///
    /// This method should be called after a proposal has been accepted by
    /// consensus. The payloads are applied and committed to the database,
    /// making the state changes permanent.
    ///
    /// # Arguments
    ///
    /// * `payloads` - The consensus payloads to finalize. Must include a
    ///   [`ConsensusPayload::FoundationRoot`] as the final element.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Shutdown`] if the Foundation task has shut down.
    pub async fn finalize(
        &self,
        payloads: Vec<ConsensusPayload>,
    ) -> Result<FoundationOutcome, Error> {
        let (callback, response) = sync::oneshot::channel();

        let msg = FinalizerMessage { payloads, callback };

        self.queue.send(msg).await.map_err(|_| Error::Shutdown)?;
        response.await.map_err(|_| Error::Shutdown)?
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BestBlock {
    pub block_hash: BlockHash,
    pub header: bitcoin::block::Header,
    pub height: u64,
}

/// Background task that processes Foundation layer state transitions.
///
/// This task receives proposal and finalization requests via separate queues,
/// validates consensus payloads, and manages the Foundation commitment state.
/// It serves as the single point of mutation for the Foundation layer, ensuring
/// sequential processing of state changes.
///
/// Created via [`FoundationTask::new`], which returns the task along with
/// [`ProposalHandle`] and [`FinalizationHandle`] for submitting requests. The
/// task does not begin processing until [`FoundationTask::run`] is called.
pub struct FoundationTask<DB, N>
where
    DB: Database,
    N: NodeTypes,
{
    // Proposal and finalization use separate queues to prevent a flood of
    // proposal requests from starving finalization processing.
    from_proposer: sync::mpsc::Receiver<ProposerMessage>,
    from_finalizer: sync::mpsc::Receiver<FinalizerMessage>,
    //
    to_blockwatcher: sync::mpsc::Sender<ToBlockWatchMsg>,
    from_blockwatcher: sync::mpsc::Receiver<FromBlockWatchMsg>,
    //
    blockwatcher: Option<BlockWatcher>,
    best_block: BestBlock,
    db_factory: BotanixProviderFactory<DB, N>,
    foundation: Foundation<
        WBotanixProviderFactory<DB, N>,
        WBotanixDatabaseProvider<DB, N>,
    >,
}

impl<DB, N> FoundationTask<DB, N>
where
    DB: Database,
    N: NodeTypes + NodeTypesForProvider,
{
    /// Creates a new Foundation task with its communication handles.
    ///
    /// Initializes the Foundation layer state machine, sets up communication
    /// channels, and creates the [`BlockWatcher`] for monitoring Bitcoin
    /// confirmations. The [`BlockWatcher`] is initialized but not started until
    /// [`Self::run`] is called.
    ///
    /// # Genesis vs Recovery
    ///
    /// - **Genesis**: If no Bitcoin headers exist in the database, this is the
    ///   initial coordinator setup. The current best block from `bitcoind` is
    ///   used as the starting point.
    ///
    /// - **Recovery**: If Bitcoin headers exist, they are loaded from the
    ///   database and validated against the commitment state. This path is used
    ///   by all subsequent nodes syncing existing state.
    ///
    /// # Arguments
    ///
    /// * `bitcoin_height` - The current Bitcoin block height.
    /// * `botanix_height` - The current Botanix block height.
    /// * `bitcoind_factory` - Client for querying `bitcoind` RPC endpoints.
    /// * `db_factory` - Factory for creating database providers.
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// 
    /// - The `FoundationTask` instance (call [`Self::run`] to start the event
    ///   loop)
    /// - A [`ProposalHandle`] for submitting proposals (cloneable)
    /// - A [`FinalizationHandle`] for finalizing accepted proposals (exclusive)
    ///
    /// # Errors
    ///
    /// Returns an error if database access fails, `bitcoind` communication
    /// fails, or Foundation state initialization fails.
    pub fn new(
        bitcoin_height: u64,
        botanix_height: u64,
        bitcoind_factory: Arc<FallbackBitcoindClient>,
        db_factory: BotanixProviderFactory<DB, N>,
    ) -> Result<(Self, ProposalHandle, FinalizationHandle), Error> {
        let db = db_factory.provider()?;

        // Check the database for any tracked Bitcoin headers.
        let bitcoin_headers: Vec<BlockHash> = db
            .get_onchain_headers()?
            .into_iter()
            .map(|h| h.block_hash)
            .collect();

        std::mem::drop(db);

        let wfactory = WBotanixProviderFactory::new(db_factory.clone())?;

        // Request the best block via bitcoind.
        let best_block = {
            let block_hash = bitcoind_factory.get_best_block_hash_rpc()?;
            let header = bitcoind_factory.get_block_header_rpc(&block_hash)?;
            let info = bitcoind_factory.get_block_info_rpc(&block_hash)?;

            BestBlock { block_hash, header, height: info.height as u64 }
        };

        // If no Bitcoin headers are available yet, then this is the genesis
        // state for the Foundation layer. This occurs exactly once in the
        // lifetime of the Botanix chain, during initial coordinator setup. All
        // subsequent nodes must sync the existing state to avoid a CometBFT
        // _appHash_ mismatch (fork).
        let foundation = if bitcoin_headers.is_empty() {
            // We just use the best block as the initial starting point.
            Foundation::new_genesis(
                wfactory,
                best_block.header,
                bitcoin_height,
                botanix_height,
                CONF_DEPTH,
            )?
        } else {
            Foundation::new(
                wfactory,
                // Preload Bitcoin headers, which are validated against the
                // commitment state.
                &bitcoin_headers,
                botanix_height,
                CONF_DEPTH,
            )?
        };

        // Setup communication channels.
        let (to_proposer, from_proposer) = sync::mpsc::channel(100);
        let (to_finalizer, from_finalizer) = sync::mpsc::channel(100);

        let prop = ProposalHandle { queue: to_proposer };
        let fin = FinalizationHandle { queue: to_finalizer };

        // Setup the [`BlockWatcher`] task and preload it with the tracked
        // blocks to watch, according to the Foundation layer.
        let tracked_blocks = foundation.tracked_blocks();
        let (blockwatcher, to_blockwatcher, from_blockwatcher) =
            BlockWatcher::new(&tracked_blocks, best_block, bitcoind_factory);

        // TODO: The Foundation layer should have a "best block" method, then
        // compare it against the locally retrieved best block!

        let this = FoundationTask {
            from_proposer,
            from_finalizer,
            to_blockwatcher,
            best_block,
            from_blockwatcher,
            blockwatcher: Some(blockwatcher),
            db_factory,
            foundation,
        };

        Ok((this, prop, fin))
    }
    /// Runs the Foundation task event loop.
    ///
    /// Spawns the [`BlockWatcher`] as a background task and processes events
    /// from multiple sources concurrently:
    ///
    /// - **Proposal requests** ([`ProposalHandle`]): Creates new proposals or
    ///   validates proposals from other nodes. Closing this channel is ignored
    ///   since proposals are optional during normal (non-validator) operation.
    ///
    /// - **Finalization requests** ([`FinalizationHandle`]): Commits accepted
    ///   proposals to persistent storage.
    ///
    /// - **Block notifications** ([`BlockWatcher`]): Updates internal state
    ///   when Bitcoin blocks are observed or the best block changes.
    ///
    /// # Shutdown
    ///
    /// The loop exits when any of the following occur:
    ///
    /// - The [`FinalizationHandle`] is dropped (graceful shutdown)
    /// - The [`BlockWatcher`] channel closes (unexpected)
    /// - The [`BlockWatcher`] task completes (unexpected)
    //
    // TODO: `bitcoind` error handling from the `BlockWatcher` is still to be
    // decided.
    pub async fn run(mut self) {
        // Spawn-off the [`BlockWatcher`] task.
        let block_watcher_handle = tokio::spawn(
            self.blockwatcher
                .take()
                .expect("BlockWatcher must be initialized")
                .run(),
        );

        tokio::pin!(block_watcher_handle);

        loop {
            tokio::select! {
                // Validate any state transitions from the proposer.
                Some(msg) = self.from_proposer.recv() => {
                    match msg {
                        // Create a proposal to be included in the Consensus
                        // layer. This usually occurs only for validators.
                        ProposerMessage::CreateProposal { pegout_proposal, replacing_proposal, callback } => {
                            let res: Result<_, _> = self._propose_consensus_payloads(pegout_proposal, replacing_proposal).await;
                            let _ = callback.send(res);
                        }
                        // Validate a proposal from the Consensus layer. This
                        // usually occurs only for validators during the
                        // CometBFT `fn process_block` stage.
                        ProposerMessage::ValidateProposal { payloads, callback } => {
                            let res: Result<_, _> = self._validate_consensus_payloads(payloads).await;
                            let _ = callback.send(res);
                        }
                    }
                }
                // Finalize any state transitions from the finalizer.
                msg = self.from_finalizer.recv() => {
                    let Some(msg) = msg else {
                        // Finalizer dropped, exit the loop.
                        return;
                    };

                    let res: Result<_, _> = self._finalize_consensus_payloads(msg.payloads).await;
                    let _ = msg.callback.send(res);
                }
                // Mark any blocks that the [`BlockWatcher`] confirms it has
                // seen.
                msg = self.from_blockwatcher.recv() => {
                    let Some(msg) = msg else {
                        // BlockWatcher dropped, exit the loop.
                        return;
                    };

                    match msg {
                        FromBlockWatchMsg::Observed { block_hash } => {
                            if self.foundation.mark_bitcoin_header(block_hash).is_err() {
                                // TODO: Should technically never occur.
                            }
                        },
                        FromBlockWatchMsg::Best { block } => {
                            // TODO: Consider checking whether this is actually
                            // a better block than the current.
                            self.best_block = block;
                        }
                        FromBlockWatchMsg::BitcoindError { error } => {
                            todo!();
                        }
                    }
                }
                // If the BlockWatcher exists, then exit this loop as well.
                _ = &mut block_watcher_handle => {
                    return;
                }
            }
        }
    }
    /// Creates consensus payloads for a new block proposal.
    ///
    /// Gathers pending operations and constructs the payloads to include in a
    /// proposal:
    ///
    /// 1. **Bitcoin header**: Included if the current best block is not yet
    ///    tracked by the Foundation layer.
    ///
    /// 2. **Pegout proposal**: Included if `pegout_proposal` is provided,
    ///    optionally replacing a previous proposal via `replacing_proposal`.
    ///
    /// 3. **Foundation root**: Always included as the final payload, containing
    ///    the computed state root after applying all other payloads.
    ///
    /// Any changes are rolled back and not persisted. See
    /// [`ProposalHandle::create_proposal`] for the public API.
    async fn _propose_consensus_payloads(
        &mut self,
        mut pegout_proposal: Option<ProposalEntry>,
        replacing_proposal: Option<Txid>,
    ) -> Result<ProposalOutcome, Error> {
        if pegout_proposal.is_none() && replacing_proposal.is_some() {
            // TODO: Use different variant.
            return Err(ValidationError::InvalidState)?;
        }

        let mut payloads = vec![];

        // Prepare a Bitcoin header proposal, if appropriate.
        // TODO: Assess risks of multiple inclusions of the same block.
        if !self
            .foundation
            .tracked_blocks()
            .contains(&self.best_block.block_hash)
        {
            payloads.push(ConsensusPayload::BitcoinHeader {
                header: self.best_block.header,
                height: self.best_block.height,
            });
        }

        // Prepare a pegout proposal, if appropriate.
        if let Some(proposal) = pegout_proposal.take() {
            payloads.push(ConsensusPayload::PegoutProposal {
                proposal,
                replacing: replacing_proposal,
            });
        }

        let mut db = self.db_factory.provider()?;

        // Validate the proposed commitments/payloads and compute the Foundation
        // commitment state root.
        let res: CheckedFoundationProof<_> =
            self.foundation.propose_commitments(|c| {
                // Cloning is required here, since the Foundation layer mandates
                // ownership of data.
                for p in payloads.clone() {
                    commit_payload(p, &mut db, c)?
                }

                Ok(())
            })?;

        // TODO: Consider adding a `fn into_state` function.
        let root = res.compute_root();
        let state = res.state().clone();

        payloads.push(ConsensusPayload::FoundationRoot { root });

        Ok(ProposalOutcome { root, state, payloads })
    }
    /// Validates consensus payloads from another node's proposal.
    ///
    /// Applies the payloads to compute the resulting state root and verifies it
    /// matches the provided root. Any changes are rolled back and not
    /// persisted.
    ///
    /// # Validation Steps
    ///
    /// 1. Extracts the [`ConsensusPayload::FoundationRoot`] from the end of the
    ///    payload list.
    ///
    /// 2. Applies all remaining payloads to compute the state root.
    ///
    /// 3. Verifies the computed root matches the provided root.
    ///
    /// 4. Notifies the [`BlockWatcher`] to watch any new Bitcoin headers
    ///    (removals are deferred until finalization).
    ///
    /// See [`ProposalHandle::validate_proposal`] for the public API.
    async fn _validate_consensus_payloads(
        &mut self,
        payloads: Vec<ConsensusPayload>,
    ) -> Result<FoundationOutcome, Error> {
        // Convert into VecDeque since it's easier to work with.
        let mut payloads: VecDeque<_> = payloads.into();

        // The last payload MUST be the foundation root.
        let Some(ConsensusPayload::FoundationRoot { root: provided_root }) =
            payloads.pop_back()
        else {
            return Err(ValidationError::InvalidState)?;
        };

        if payloads.is_empty() {
            return Err(ValidationError::InvalidState)?;
        }

        let mut db = self.db_factory.provider()?;

        // Validate the proposed commitments/payloads and compute the Foundation
        // commitment state root.
        let res: CheckedFoundationProof<_> =
            self.foundation.propose_commitments(|c| {
                for p in payloads {
                    commit_payload(p, &mut db, c)?
                }

                Ok(())
            })?;

        // TODO: There should be a method for this logic in the Foundation layer
        // directly, just like [`Foundation::finalize_commitments`] - but
        // without the persistent changes.
        let computed_root = res.compute_root();
        if computed_root != provided_root {
            return Err(ValidationError::InvalidState)?;
        }

        // Notify the [`BlockWatcher`] to start watching newly inserted blocks.
        for event in &res.state().aux_events {
            match event {
                AuxEvent::NewBitcoinHeader { block_hash } => {
                    // TODO: Reconsider blocking.
                    self.to_blockwatcher
                        .send(ToBlockWatchMsg::InsertBlock {
                            block_hash: *block_hash,
                        })
                        .await
                        .unwrap();
                }
                // Block removals are skipped, since this is the uncommited
                // propose state. Only during finalization is the BlockWatcher
                // notified about removals.
                _ => {}
            }
        }

        // TODO: Consider adding a `fn into_state` function.
        let state = res.state().clone();

        Ok(FoundationOutcome { root: computed_root, state })
    }
    /// Finalizes consensus payloads and commits them to persistent storage.
    ///
    /// Applies the payloads, verifies the computed root matches the provided
    /// root, and commits all changes to the database. Unlike validation, this
    /// method persists state changes.
    ///
    /// # Finalization Steps
    ///
    /// 1. Extracts the [`ConsensusPayload::FoundationRoot`] from the end of the
    ///    payload list.
    ///
    /// 2. Applies all remaining payloads and commits to persistent storage.
    ///
    /// 3. Verifies the computed root matches the provided root.
    ///
    /// 4. Notifies the [`BlockWatcher`] about new Bitcoin headers to watch and
    ///    finalized/orphaned headers to stop watching.
    ///
    /// See [`FinalizationHandle::finalize`] for the public API.
    async fn _finalize_consensus_payloads(
        &mut self,
        payloads: Vec<ConsensusPayload>,
    ) -> Result<FoundationOutcome, Error> {
        // Convert into VecDeque since it's easier to work with.
        let mut payloads: VecDeque<_> = payloads.into();

        // The last payload MUST be the foundation root.
        let Some(ConsensusPayload::FoundationRoot { root: provided_root }) =
            payloads.pop_back()
        else {
            return Err(ValidationError::InvalidState)?;
        };

        if payloads.is_empty() {
            return Err(ValidationError::InvalidState)?;
        }

        let mut db = self.db_factory.provider()?;

        // Validate and finalize the proposed commitments/payloads, then compute
        // the Foundation commitment state root and compare it against the
        // provided root.
        let res: CheckedFoundationProof<_> =
            self.foundation.finalize_commitments(provided_root, |c| {
                for p in payloads {
                    commit_payload(p, &mut db, c)?
                }

                Ok(())
            })?;

        // Notify the [`BlockWatcher`] to start watching newly inserted blocks
        // and to stop watching finalized/orphaned blocks. Regarding the latter,
        // the watcher handles that implicitly via the capped LruCache, but
        // explicit removal is cleaner.
        //
        // TODO: This gets tricky while syncing; bitcoind would get hammered
        // here. Ideally we would distinguish between "syncing" and live
        // participation.
        for event in &res.state().aux_events {
            match event {
                AuxEvent::NewBitcoinHeader { block_hash } => {
                    // TODO: Reconsider blocking.
                    self.to_blockwatcher
                        .send(ToBlockWatchMsg::InsertBlock {
                            block_hash: *block_hash,
                        })
                        .await
                        .unwrap();
                }
                AuxEvent::FinalizedBitcoinHeader { block_hash, .. } |
                AuxEvent::OrphanedBitcoinHeader { block_hash, .. } => {
                    // We don't bother waiting if the queue is full, which is
                    // unlikely anyway. Worst-cache the cache will drop it at
                    // some point later.
                    let _ = self.to_blockwatcher.try_send(
                        ToBlockWatchMsg::RemoveBlock {
                            block_hash: *block_hash,
                        },
                    );
                }
                _ => {}
            }
        }

        // TODO: Consider adding a `fn into_state` function.
        let root = res.compute_root();
        let state = res.state().clone();

        Ok(FoundationOutcome { root, state })
    }
}

/// Convenience function for validating and applying a consensus payload to the
/// commitment state. This is called during both proposal and finalization to
/// ensure payloads are valid before committing them to the Foundation layer.
fn commit_payload<DB, N>(
    payload: ConsensusPayload,
    db: &mut BotanixDatabaseProviderRO<DB, N>,
    c: &mut CommitmentsDraft<
        '_,
        WBotanixProviderFactory<DB, N>,
        WBotanixDatabaseProvider<DB, N>,
    >,
) -> Result<(), foundation::Error<ProviderError, ProviderError>>
where
    DB: Database,
    N: NodeTypes + NodeTypesForProvider,
{
    match payload {
        ConsensusPayload::PegoutProposal { proposal, replacing } => {
            // INSERT state commitment.
            c.insert_pegout_proposal(proposal, replacing)
        }
        // TODO: Add additional PoW check for mainnet only!
        ConsensusPayload::BitcoinHeader { header, height } => {
            // INSERT state commitment.
            c.insert_bitcoin_header(header, height)
        }
        ConsensusPayload::BitcoinTransaction { block_hash, tx, proof } => {
            let txid = tx.compute_txid();

            // TODO: Consider handling this on the Foundation layer
            let proposal = db
                .get_pegout_proposal(txid)
                .map_err(DatabaseError::from)?
                .ok_or(ValidationError::InvalidState)?;

            // INSERT state commitment.
            c.insert_bitcoin_tx(block_hash, tx, proof, proposal)
        }
        ConsensusPayload::FoundationRoot { root: _ } => {
            return Err(ValidationError::InvalidState)?;
        }
    }
}
