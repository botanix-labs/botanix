//! Guarded database transaction for shared, mutex-protected access.
//!
//! This module provides [`BotanixGuardedFactory`], a wrapper that manages a
//! database transaction lifecycle internally behind a mutex. It is intended for
//! use cases where a component takes ownership of database access but cannot
//! receive a transaction as a parameter.
use crate::{
    BotanixDatabaseProviderRW, BotanixProviderFactory,
    DatabaseProviderFactoryRW,
};
use reth_db::Database;
use reth_node_types::NodeTypes;
use reth_provider::{
    providers::NodeTypesForProvider, ProviderError, ProviderResult,
};
use std::sync::Arc;

/// Errors for [`BotanixGuardedFactory`] lifecycle operations.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Provider transaction already started.
    #[error("provider transaction already started")]
    ProviderAlreadyStarted,
    /// Provider transaction not started.
    #[error("provider transaction not started")]
    ProviderNotStarted,
}

impl From<Error> for ProviderError {
    fn from(err: Error) -> Self {
        ProviderError::other(err)
    }
}

/// A mutex guard providing access to the underlying
/// [`BotanixDatabaseProviderRW`].
///
/// This type alias is returned by [`BotanixGuardedFactory::guard`]. It
/// dereferences to [`BotanixDatabaseProviderRW`], which implements various
/// reader/writer traits for database operations.
pub type MutexGuard<'a, DB, N> =
    parking_lot::MappedMutexGuard<'a, BotanixDatabaseProviderRW<DB, N>>;

/// A guard around [`BotanixProviderFactory`] that manages database transaction
/// lifecycle internally behind a mutex.
///
/// This guard is designed for situations where a component takes ownership of
/// database access but cannot accept a transaction as a parameter, for example
/// when the transaction must span multiple method calls or async boundaries.
///
/// # Usage
///
/// ```ignore
/// // Start a transaction
/// db.start()?;
///
/// // Access the database (can be called multiple times)
/// db.guard()?.insert_attested_multisig(id, entry)?;
/// db.guard()?.get_attested_multisig(id)?;
///
/// // End the transaction
/// db.commit()?;  // or db.rollback()?;
/// ```
///
/// The type can be cloned; all clones share the same transaction state via an
/// internal `Arc<Mutex<_>>`.
#[derive(Debug)]
pub struct BotanixGuardedFactory<DB, N>
where
    DB: Database,
    N: NodeTypes,
{
    factory: BotanixProviderFactory<DB, N>,
    tx: Arc<parking_lot::Mutex<Option<BotanixDatabaseProviderRW<DB, N>>>>,
}

// Manual Clone implementation so that the type parameter wrapped in Arc does
// not require the Clone bound as well.
impl<DB, N> Clone for BotanixGuardedFactory<DB, N>
where
    DB: Database,
    N: NodeTypes,
{
    fn clone(&self) -> Self {
        Self { factory: self.factory.clone(), tx: Arc::clone(&self.tx) }
    }
}

impl<DB, N> BotanixGuardedFactory<DB, N>
where
    DB: Database,
    N: NodeTypes + NodeTypesForProvider,
{
    /// Creates a new guard with no active transaction.
    pub fn new(factory: BotanixProviderFactory<DB, N>) -> Self {
        BotanixGuardedFactory { factory, tx: Default::default() }
    }
    /// Returns a reference to the underlying database provider.
    ///
    /// The returned guard holds the internal mutex lock and provides access to
    /// the [`BotanixDatabaseProviderRW`], which implements various
    /// reader/writer traits for database operations.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ProviderNotStarted`] if no transaction is active.
    pub fn guard(&self) -> ProviderResult<MutexGuard<'_, DB, N>> {
        let tx = self.tx.lock();
        parking_lot::MutexGuard::try_map(tx, |opt| opt.as_mut())
            .map_err(|_| Error::ProviderNotStarted.into())
    }
    /// Starts a new database transaction.
    ///
    /// Creates a new read-write provider from the factory and stores it
    /// internally. The transaction remains active until [`commit`] or
    /// [`rollback`] is called.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ProviderAlreadyStarted`] if a transaction is already
    /// active.
    ///
    /// [`commit`]: Self::commit
    /// [`rollback`]: Self::rollback
    pub fn start(&self) -> ProviderResult<()> {
        let mut tx = self.tx.lock();
        if tx.is_some() {
            return Err(Error::ProviderAlreadyStarted)?;
        }

        let provider = self.factory.provider_rw()?;
        *tx = Some(provider);

        Ok(())
    }
    /// Commits the active transaction, persisting all changes to the database.
    ///
    /// After committing, the guard returns to the inactive state and a new
    /// transaction can be started with [`start`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::ProviderNotStarted`] if no transaction is active.
    ///
    /// [`start`]: Self::start
    pub fn commit(&self) -> ProviderResult<bool> {
        let mut tx = self.tx.lock();
        let provider = tx.take().ok_or(Error::ProviderNotStarted)?;
        let b = provider.commit()?;

        debug_assert!(tx.is_none());
        Ok(b)
    }
    /// Rolls back the active transaction, discarding all changes.
    ///
    /// After rolling back, the guard returns to the inactive state and a new
    /// transaction can be started with [`start`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::ProviderNotStarted`] if no transaction is active.
    ///
    /// [`start`]: Self::start
    pub fn rollback(&self) -> ProviderResult<()> {
        let mut tx = self.tx.lock();
        let provider = tx.take().ok_or(Error::ProviderNotStarted)?;
        std::mem::drop(provider);

        debug_assert!(tx.is_none());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::StagedMultisigEntry, test_utils::create_test_provider_factory,
        MultisigManagerReader, MultisigManagerWriter,
    };
    use botanix_types::MultisigId;
    use frost_secp256k1_tr as frost;
    use std::collections::BTreeMap;

    fn create_test_multisig_staging() -> StagedMultisigEntry {
        let multisig_id = MultisigId::new(42);
        let coordinator =
            frost::Identifier::derive(&[1u8]).expect("valid identifier");

        StagedMultisigEntry {
            multisig_id,
            coordinator,
            fed_members: BTreeMap::new(),
        }
    }

    #[test]
    fn guard_fails_outside_transaction() {
        let factory = create_test_provider_factory();
        let db = BotanixGuardedFactory::new(factory);

        // guard() should fail when no transaction is active
        assert!(db.guard().is_err());
    }

    #[test]
    fn can_run_multiple_transactions() {
        let factory = create_test_provider_factory();
        let db = BotanixGuardedFactory::new(factory);

        // First transaction (commit)
        db.start().unwrap();
        db.commit().unwrap();

        // Second transaction (rollback)
        db.start().unwrap();
        db.rollback().unwrap();

        // Third transaction (commit again)
        db.start().unwrap();
        db.commit().unwrap();
    }

    #[test]
    fn commit_persists_data() {
        let factory = create_test_provider_factory();
        let db = BotanixGuardedFactory::new(factory);
        let id = MultisigId::new(42);
        let entry = create_test_multisig_staging();

        // Insert and commit
        db.start().unwrap();
        db.guard().unwrap().insert_staging_multisig(id, entry).unwrap();
        db.commit().unwrap();

        // Verify data persisted in a new transaction
        db.start().unwrap();
        let result = db.guard().unwrap().get_staging_multisig(id).unwrap();
        db.rollback().unwrap();

        assert!(result.is_some());
    }

    #[test]
    fn rollback_discards_data() {
        let factory = create_test_provider_factory();
        let db = BotanixGuardedFactory::new(factory);
        let id = MultisigId::new(42);
        let entry = create_test_multisig_staging();

        // Insert and rollback
        db.start().unwrap();
        db.guard().unwrap().insert_staging_multisig(id, entry).unwrap();
        db.rollback().unwrap();

        // Verify data was discarded
        db.start().unwrap();
        let result = db.guard().unwrap().get_staging_multisig(id).unwrap();
        db.rollback().unwrap();

        assert!(result.is_none());
    }
}
