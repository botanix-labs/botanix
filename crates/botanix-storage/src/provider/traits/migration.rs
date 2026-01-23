//! Traits for migration database operations.

use crate::models::{MigrationId, MigrationRecord, MigrationStatus};
use reth_storage_errors::provider::ProviderResult;

/// Trait for reading migration data from the database.
///
/// This trait provides read-only access to migration records,
/// which track active multisig migrations during the dynafed
/// transition process.
#[auto_impl::auto_impl(&, Arc, Box)]
pub trait MigrationReader: Send + Sync {
    /// Get all migration records.
    ///
    /// Retrieves all active migration records from the database.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<MigrationRecord>)` - A vector of all migration records
    /// * `Err(ProviderError)` - If there was a database error
    fn get_all_migrations(&self) -> ProviderResult<Vec<MigrationRecord>>;

    /// Get a migration by its ID.
    ///
    /// # Parameters
    ///
    /// * `migration_id` - The unique migration ID
    ///
    /// # Returns
    ///
    /// * `Ok(Some(MigrationRecord))` - The migration record if found
    /// * `Ok(None)` - If no migration exists with the given ID
    /// * `Err(ProviderError)` - If there was a database error
    fn get_migration(
        &self,
        migration_id: MigrationId,
    ) -> ProviderResult<Option<MigrationRecord>>;

    /// Check if a migration exists for a given multisig ID.
    ///
    /// Searches for any active migration where the given multisig ID
    /// is either the source or target.
    ///
    /// # Parameters
    ///
    /// * `multisig_id` - The multisig ID to search for
    ///
    /// # Returns
    ///
    /// * `Ok(Some(MigrationId))` - The migration ID if found
    /// * `Ok(None)` - If no migration involves the given multisig
    /// * `Err(ProviderError)` - If there was a database error
    fn migration_exists_for_multisig(
        &self,
        multisig_id: u32,
    ) -> ProviderResult<Option<MigrationId>>;

    /// Get the count of active migrations.
    ///
    /// # Returns
    ///
    /// * `Ok(usize)` - The number of active migrations
    /// * `Err(ProviderError)` - If there was a database error
    fn get_migrations_count(&self) -> ProviderResult<usize>;
}

/// Trait for writing migration data to the database.
///
/// This trait provides write access to migration records,
/// enabling the creation, updating, and removal of migrations.
#[auto_impl::auto_impl(&, Arc, Box)]
pub trait MigrationWriter: Send + Sync {
    /// Store a new migration record.
    ///
    /// # Parameters
    ///
    /// * `record` - The migration record to store
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the record was successfully stored
    /// * `Err(ProviderError)` - If there was a database error
    fn store_migration(&self, record: &MigrationRecord) -> ProviderResult<()>;

    /// Update the status of a migration.
    ///
    /// # Parameters
    ///
    /// * `migration_id` - The ID of the migration to update
    /// * `status` - The new status
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - If the migration was found and updated
    /// * `Ok(false)` - If the migration was not found
    /// * `Err(ProviderError)` - If there was a database error
    fn update_migration_status(
        &self,
        migration_id: MigrationId,
        status: MigrationStatus,
    ) -> ProviderResult<bool>;

    /// Remove a migration by its ID.
    ///
    /// # Parameters
    ///
    /// * `migration_id` - The ID of the migration to remove
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - If the migration was found and removed
    /// * `Ok(false)` - If the migration was not found
    /// * `Err(ProviderError)` - If there was a database error
    fn remove_migration(
        &self,
        migration_id: MigrationId,
    ) -> ProviderResult<bool>;

    /// Remove all migrations.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If all migrations were successfully removed
    /// * `Err(ProviderError)` - If there was a database error
    fn remove_all_migrations(&self) -> ProviderResult<()>;
}
