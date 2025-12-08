//! Database adapter for the Foundation layer.
//!
//! This module provides wrappers that bridge the Botanix storage layer with the
//! Foundation layer's trie-based commitment state. [`WBotanixProviderFactory`]
//! implements [`AtomicLayer`] to provide transactional semantics, while
//! [`WBotanixDatabaseProvider`] implements the underlying [`HashDB`] and
//! [`DataSource`] traits required by the trie and Foundation operations.

use botanix_storage::{
    BotanixDatabaseProviderRW, BotanixProviderFactory,
    DatabaseProviderFactoryRW, FoundationLayerReader, FoundationLayerWriter,
};
// TODO: Consider adding a convenience `prelude::*` export module.
use botanix_tem::{
    foundation::{
        bitcoin::{BlockHash, OutPoint, Txid},
        hash_db,
        trie_db::{self, DBValue, HashDB},
        AtomicError, AtomicErrorVariant, AtomicLayer, BotanixLayer, Checked,
        CommitHasher, CommitmentStateRoot, DataSource, DatabaseError,
        EOnchainHeader, EOnchainUtxo, EProposal, EUnassigned,
    },
    validation::pegout::PegoutId,
};
use reth_db_api::Database;
use reth_node_types::NodeTypes;
use reth_provider::providers::NodeTypesForProvider;
use reth_storage_errors::provider::ProviderError;

/// A wrapper over the [`BotanixProviderFactory`] that implements the
/// [`AtomicLayer`] from the Botanix TEM crate.
///
/// This is passed-on to the [`botanix_tem::foundation::Foundation`] structure.
#[derive(Debug)]
pub struct WBotanixProviderFactory<DB, N>
where
    DB: Database,
    N: NodeTypes,
{
    factory: BotanixProviderFactory<DB, N>,
    latest_commit: CommitmentStateRoot,
    //
    provider: Option<WBotanixDatabaseProvider<DB, N>>,
    wrk_root: Option<[u8; 32]>,
}

impl<DB, N> WBotanixProviderFactory<DB, N>
where
    DB: Database,
    N: NodeTypes + NodeTypesForProvider,
{
    /// Creates a new wrapper around the given [`BotanixProviderFactory`].
    ///
    /// This initializes the commitment state by reading the latest root from
    /// the database. If no root exists (new database), the trie is initialized
    /// with a null-node as required by `trie-db`.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] if the database transaction fails.
    pub fn new(
        factory: BotanixProviderFactory<DB, N>,
    ) -> Result<Self, ProviderError> {
        // Start a new database transaction.
        let mut provider =
            WBotanixDatabaseProvider { tx: factory.provider_rw()? };

        // Retrieve the root from the local database, or initialize with
        // null-node if this is a new database (required by `trie-db`).
        let mut wrk_root = match provider.tx.get_foundation_commitment_root()? {
            Some(r) => r,
            None => {
                let r = CommitHasher::HASHED_NULL_NODE;
                // Null-node MUST be inserted into the database.
                provider.tx.insert_foundation_commitment(r, vec![0u8])?;
                r
            }
        };

        // Retrieve the [`CommitmentStateRoot`] directly from the underlying
        // trie-db.
        let latest_commit =
            BotanixLayer::new(&mut provider, &mut wrk_root).root();
        debug_assert_eq!(latest_commit.as_ref(), &wrk_root);

        // COMMIT any changes to the database, assuming there are any.
        provider.tx.commit()?;

        Ok(WBotanixProviderFactory {
            factory,
            latest_commit,
            provider: None,
            wrk_root: None,
        })
    }
}

impl<DB, N> AtomicLayer<WBotanixDatabaseProvider<DB, N>>
    for WBotanixProviderFactory<DB, N>
where
    DB: Database,
    N: NodeTypes + NodeTypesForProvider,
{
    type BackendError = ProviderError;

    fn start_tx<'db>(
        &'db mut self,
    ) -> Result<
        BotanixLayer<'db, WBotanixDatabaseProvider<DB, N>>,
        AtomicError<ProviderError>,
    > {
        // Start a new database transaction.
        let tx =
            self.factory.provider_rw().map_err(AtomicErrorVariant::Backend)?;

        self.provider = Some(WBotanixDatabaseProvider { tx });
        self.wrk_root = Some(*self.latest_commit.as_ref());

        let botanix = BotanixLayer::new(
            self.provider.as_mut().expect("provider must be set"),
            self.wrk_root.as_mut().expect("working root must be set"),
        );

        Ok(botanix)
    }
    fn commit(
        &mut self,
    ) -> Result<CommitmentStateRoot, AtomicError<ProviderError>> {
        let mut provider = self
            .provider
            .take()
            .ok_or(AtomicErrorVariant::CommitmentLayerNotStarted)?;

        let mut wrk_root = self
            .wrk_root
            .take()
            .ok_or(AtomicErrorVariant::CommitmentLayerNotStarted)?;

        // Retrieve the [`CommitmentStateRoot`] directly from the underlying
        // trie-db.
        let commit = BotanixLayer::new(&mut provider, &mut wrk_root).root();
        debug_assert_eq!(commit.as_ref(), &wrk_root);

        // STORE the latest root directly into the transaction.
        provider
            .tx
            .insert_foundation_commitment_root(*commit.as_ref())
            .map_err(AtomicErrorVariant::Backend)?;

        // COMMIT the transaction changes to the database.
        provider.tx.commit().map_err(AtomicErrorVariant::Backend)?;

        // KEEP track of the latest commit, which will be passed-on to the next
        // initiation via `Self::start_tx`.
        self.latest_commit = commit;

        debug_assert!(self.provider.is_none());
        debug_assert!(self.wrk_root.is_none());

        Ok(commit)
    }
    fn rollback(
        &mut self,
    ) -> Result<CommitmentStateRoot, AtomicError<ProviderError>> {
        let provider = self
            .provider
            .take()
            .ok_or(AtomicErrorVariant::CommitmentLayerNotStarted)?;

        let _ = self
            .wrk_root
            .take()
            .ok_or(AtomicErrorVariant::CommitmentLayerNotStarted)?;

        // Just drop the database transaction; rollback is implied.
        let trash: WBotanixDatabaseProvider<_, _> = provider;
        std::mem::drop(trash);

        debug_assert!(self.provider.is_none());
        debug_assert!(self.wrk_root.is_none());

        Ok(self.latest_commit)
    }
}

/// A wrapper over the [`BotanixDatabaseProviderRW`] that implements both the
/// [`DataSource`] from the Botanix TEM, and the [`trie_db::HashDB`] from the
/// Parity crate required for Trie operations.
///
/// This is acquired internally in the [`botanix_tem::foundation::Foundation`]
/// structure via the [`WBotanixProviderFactory`].
#[derive(Debug)]
pub struct WBotanixDatabaseProvider<DB, N>
where
    DB: Database,
    N: NodeTypes,
{
    tx: BotanixDatabaseProviderRW<DB, N>,
}

impl<DB, N> hash_db::AsHashDB<CommitHasher, DBValue>
    for WBotanixDatabaseProvider<DB, N>
where
    DB: Database,
    N: NodeTypes,
{
    fn as_hash_db(&self) -> &dyn HashDB<CommitHasher, DBValue> {
        self
    }
    fn as_hash_db_mut<'a>(
        &'a mut self,
    ) -> &'a mut (dyn HashDB<CommitHasher, DBValue> + 'a) {
        self
    }
}

fn compute_key(prefix: (&[u8], Option<u8>), value: &[u8]) -> [u8; 32] {
    let (slice, byte) = prefix;

    let mut h = CommitHasher::new(b"botanix:trie-db");
    h.append_message(b"prefix", slice);

    if let Some(b) = byte {
        h.append_u64(b"prefix-extra", b as u64);
    }

    h.append_message(b"value", value);
    h.finalize()
}

impl<DB, N> trie_db::HashDB<CommitHasher, DBValue>
    for WBotanixDatabaseProvider<DB, N>
where
    DB: Database,
    N: NodeTypes,
{
    fn get(
        &self,
        key: &[u8; 32],
        _prefix: (&[u8], Option<u8>),
    ) -> Option<DBValue> {
        self.tx.get_foundation_commitment(*key).expect("failed to get key")
    }
    fn contains(&self, key: &[u8; 32], _prefix: (&[u8], Option<u8>)) -> bool {
        // TODO: Consider implementing explicit `contains_*` method.
        let val =
            self.tx.get_foundation_commitment(*key).expect("failed to get key");

        val.is_some()
    }
    fn insert(
        &mut self,
        prefix: (&[u8], Option<u8>),
        value: &[u8],
    ) -> [u8; 32] {
        // Compute the (reproducible) key for the prefix-value pair.
        let key = compute_key(prefix, value);

        self.tx
            .insert_foundation_commitment(key, value.to_vec())
            .expect("failed to insert key");

        key
    }
    fn emplace(
        &mut self,
        key: [u8; 32],
        // TODO: Do something with this?
        _prefix: (&[u8], Option<u8>),
        value: DBValue,
    ) {
        self.tx
            .insert_foundation_commitment(key, value)
            .expect("failed to insert key");
    }
    fn remove(
        &mut self,
        key: &[u8; 32],
        // TODO: Do something with this?
        _prefix: (&[u8], Option<u8>),
    ) {
        let did_remove = self
            .tx
            .remove_foundation_commitment(*key)
            .expect("failed to delete key");

        debug_assert!(did_remove);
    }
}

impl<DB, N> DataSource for WBotanixDatabaseProvider<DB, N>
where
    DB: Database,
    N: NodeTypes,
{
    type Error = ProviderError;

    fn insert_unassigned(
        &mut self,
        entry: Checked<EUnassigned>,
    ) -> Result<(), DatabaseError<Self::Error>> {
        self.tx
            .insert_unassigned_pegout(entry.k, entry.consume().v)
            .map_err(Into::into)
    }
    fn get_unassigned(
        &mut self,
        pegout: &PegoutId,
    ) -> Result<Option<EUnassigned>, DatabaseError<Self::Error>> {
        self.tx
            .get_unassigned_pegout(*pegout)
            .map(|opt| opt.map(|v| EUnassigned { k: *pegout, v }))
            .map_err(Into::into)
    }
    fn remove_unassigned(
        &mut self,
        entry: Checked<EUnassigned>,
    ) -> Result<(), DatabaseError<Self::Error>> {
        self.tx
            .remove_unassigned_pegout(entry.k)
            .map(|_| ())
            .map_err(Into::into)
    }
    fn insert_utxo(
        &mut self,
        entry: Checked<EOnchainUtxo>,
    ) -> Result<(), DatabaseError<Self::Error>> {
        self.tx
            .insert_onchain_utxo(entry.k, entry.consume().v)
            .map_err(Into::into)
    }
    fn get_utxo(
        &mut self,
        utxo: &OutPoint,
    ) -> Result<Option<EOnchainUtxo>, DatabaseError<Self::Error>> {
        self.tx
            .get_onchain_utxo(*utxo)
            .map(|opt| opt.map(|v| EOnchainUtxo { k: *utxo, v }))
            .map_err(Into::into)
    }
    fn finalize_utxo(
        &mut self,
        entry: Checked<EOnchainUtxo>,
    ) -> Result<(), DatabaseError<Self::Error>> {
        self.tx.remove_onchain_utxo(entry.k).map(|_| ()).map_err(Into::into)
    }
    fn orphan_utxo(
        &mut self,
        entry: Checked<EOnchainUtxo>,
    ) -> Result<(), DatabaseError<Self::Error>> {
        self.tx.remove_onchain_utxo(entry.k).map(|_| ()).map_err(Into::into)
    }
    fn insert_header(
        &mut self,
        entry: Checked<EOnchainHeader>,
    ) -> Result<(), DatabaseError<Self::Error>> {
        self.tx
            .insert_onchain_header(entry.k, entry.consume().v)
            .map_err(Into::into)
    }
    fn get_header(
        &mut self,
        block: &BlockHash,
    ) -> Result<Option<EOnchainHeader>, DatabaseError<Self::Error>> {
        self.tx
            .get_onchain_header(*block)
            .map(|opt| opt.map(|v| EOnchainHeader { k: *block, v }))
            .map_err(Into::into)
    }
    fn remove_header(
        &mut self,
        entry: Checked<EOnchainHeader>,
    ) -> Result<(), DatabaseError<Self::Error>> {
        self.tx.remove_onchain_header(entry.k).map(|_| ()).map_err(Into::into)
    }
    fn insert_pegout_proposal(
        &mut self,
        entry: Checked<EProposal>,
    ) -> Result<(), DatabaseError<Self::Error>> {
        self.tx
            .insert_pegout_proposal(entry.k, entry.consume().v)
            .map_err(Into::into)
    }
    fn get_proposal(
        &mut self,
        txid: &Txid,
    ) -> Result<Option<EProposal>, DatabaseError<Self::Error>> {
        self.tx
            .get_pegout_proposal(*txid)
            .map(|opt| opt.map(|v| EProposal { k: *txid, v }))
            .map_err(Into::into)
    }
    fn finalize_proposal(
        &mut self,
        entry: Checked<EProposal>,
    ) -> Result<(), DatabaseError<Self::Error>> {
        self.tx.remove_pegout_proposal(entry.k).map(|_| ()).map_err(Into::into)
    }
    fn orphan_proposal(
        &mut self,
        entry: Checked<EProposal>,
    ) -> Result<(), DatabaseError<Self::Error>> {
        self.finalize_proposal(entry)
    }
}
