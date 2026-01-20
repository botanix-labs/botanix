use crate::{
    models::{MigrationId, MigrationRecord, MigrationStatus},
    provider::{
        database::provider::BotanixDatabaseProvider, MigrationReader,
        MigrationWriter,
    },
    tables::Migrations,
    BotanixDatabaseProviderRW,
};
use reth_db_api::{
    cursor::DbCursorRO,
    transaction::{DbTx, DbTxMut},
    Database,
};
use reth_node_types::NodeTypes;
use reth_provider::{providers::NodeTypesForProvider, DBProvider};
use reth_storage_errors::provider::ProviderResult;
use std::collections::HashMap;

impl<TX: DbTx + 'static, N: NodeTypes> MigrationReader
    for BotanixDatabaseProvider<TX, N>
{
    fn get_all_migrations(&self) -> ProviderResult<Vec<MigrationRecord>> {
        Ok(self
            .inner
            .tx_ref()
            .cursor_read::<Migrations>()?
            .walk(None)?
            .collect::<Result<HashMap<_, _>, _>>()?
            .values()
            .cloned()
            .collect::<Vec<_>>())
    }

    fn get_migration(
        &self,
        migration_id: MigrationId,
    ) -> ProviderResult<Option<MigrationRecord>> {
        Ok(self
            .inner
            .tx_ref()
            .cursor_read::<Migrations>()?
            .seek_exact(migration_id)
            .ok()
            .flatten()
            .map(|x| x.1))
    }

    fn migration_exists_for_multisig(
        &self,
        multisig_id: u32,
    ) -> ProviderResult<Option<MigrationId>> {
        let migrations = self.get_all_migrations()?;
        for migration in migrations {
            if migration.multisig_id_from() == multisig_id
                || migration.multisig_id_to() == multisig_id
            {
                return Ok(Some(migration.migration_id()));
            }
        }
        Ok(None)
    }

    fn get_migrations_count(&self) -> ProviderResult<usize> {
        Ok(self.inner.tx_ref().cursor_read::<Migrations>()?.walk(None)?.count())
    }
}

impl<DB: Database, N: NodeTypes> MigrationReader
    for BotanixDatabaseProviderRW<DB, N>
{
    #[inline(always)]
    fn get_all_migrations(&self) -> ProviderResult<Vec<MigrationRecord>> {
        self.0.get_all_migrations()
    }

    #[inline(always)]
    fn get_migration(
        &self,
        migration_id: MigrationId,
    ) -> ProviderResult<Option<MigrationRecord>> {
        self.0.get_migration(migration_id)
    }

    #[inline(always)]
    fn migration_exists_for_multisig(
        &self,
        multisig_id: u32,
    ) -> ProviderResult<Option<MigrationId>> {
        self.0.migration_exists_for_multisig(multisig_id)
    }

    #[inline(always)]
    fn get_migrations_count(&self) -> ProviderResult<usize> {
        self.0.get_migrations_count()
    }
}

impl<DB: Database, N: NodeTypes + NodeTypesForProvider> MigrationWriter
    for BotanixDatabaseProviderRW<DB, N>
{
    fn store_migration(&self, record: &MigrationRecord) -> ProviderResult<()> {
        self.0
            .inner
            .tx_ref()
            .put::<Migrations>(record.migration_id(), record.clone())?;
        Ok(())
    }

    fn update_migration_status(
        &self,
        migration_id: MigrationId,
        status: MigrationStatus,
    ) -> ProviderResult<bool> {
        let migration = self.0.get_migration(migration_id)?;
        if let Some(mut record) = migration {
            record.set_status(status);
            self.0.inner.tx_ref().put::<Migrations>(migration_id, record)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn remove_migration(
        &self,
        migration_id: MigrationId,
    ) -> ProviderResult<bool> {
        let exists = self.0.get_migration(migration_id)?.is_some();
        if exists {
            self.0.inner.remove::<Migrations>(migration_id..=migration_id)?;
        }
        Ok(exists)
    }

    fn remove_all_migrations(&self) -> ProviderResult<()> {
        let migration_ids: Vec<_> = self
            .0
            .get_all_migrations()?
            .into_iter()
            .map(|m| m.migration_id())
            .collect();

        for id in migration_ids {
            self.0.inner.remove::<Migrations>(id..=id)?;
        }
        Ok(())
    }
}
