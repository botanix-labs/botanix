use crate::{
    models::{
        AttestedMultisigEntry, MultisigRecord, MultisigStatus,
        StagedMultisigEntry, UnoptimizedValue,
    },
    provider::database::provider::BotanixDatabaseProvider,
    tables, BotanixDatabaseProviderRW, MultisigManagerReader,
    MultisigManagerWriter,
};
use botanix_types::MultisigId;
use reth_db_api::{
    cursor::DbCursorRO,
    transaction::{DbTx, DbTxMut},
    Database,
};
use reth_node_types::NodeTypes;
use reth_provider::DBProvider;
use reth_storage_errors::provider::ProviderResult;

impl<TX: DbTx + 'static, N: NodeTypes> MultisigManagerReader
    for BotanixDatabaseProvider<TX, N>
{
    fn get_staging_multisig(
        &self,
        id: MultisigId,
    ) -> ProviderResult<Option<StagedMultisigEntry>> {
        self.inner
            .tx_ref()
            .get::<tables::Multisigs>(id.into())
            .map(|v| {
                if let Some(MultisigRecord::Staged { entry }) =
                    v.map(UnoptimizedValue::into_inner)
                {
                    Some(entry)
                } else {
                    None
                }
            })
            .map_err(Into::into)
    }

    fn get_attested_multisig(
        &self,
        id: MultisigId,
    ) -> ProviderResult<Option<AttestedMultisigEntry>> {
        self.inner
            .tx_ref()
            .get::<tables::Multisigs>(id.into())
            .map(|v| {
                if let Some(MultisigRecord::Attested { entry }) =
                    v.map(UnoptimizedValue::into_inner)
                {
                    Some(entry)
                } else {
                    None
                }
            })
            .map_err(Into::into)
    }

    fn get_active_multisigs(
        &self,
    ) -> ProviderResult<Vec<AttestedMultisigEntry>> {
        self.inner
            .tx_ref()
            .cursor_read::<tables::Multisigs>()?
            .walk(None)?
            .filter_map(|res| {
                let (k, v) = match res {
                    Ok(k_v) => k_v,
                    Err(err) => return Some(Err(err)),
                };

                let MultisigRecord::Attested { entry } = v.clone().into_inner()
                else {
                    return None;
                };

                if !matches!(
                    entry.status,
                    MultisigStatus::Funding | MultisigStatus::Degrading
                ) {
                    return None;
                }

                debug_assert_eq!(k.into_inner(), entry.multisig_id);

                Some(Ok(entry))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    fn get_multisig_records(&self) -> ProviderResult<Vec<MultisigRecord>> {
        self.inner
            .tx_ref()
            .cursor_read::<tables::Multisigs>()?
            .walk(None)?
            .filter_map(|res| {
                let (_k, v) = match res {
                    Ok(k_v) => k_v,
                    Err(err) => return Some(Err(err)),
                };

                Some(Ok(v.into_inner()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}

impl<DB: Database, N: NodeTypes> MultisigManagerReader
    for BotanixDatabaseProviderRW<DB, N>
{
    #[inline(always)]
    fn get_staging_multisig(
        &self,
        id: MultisigId,
    ) -> ProviderResult<Option<StagedMultisigEntry>> {
        self.0.get_staging_multisig(id)
    }

    #[inline(always)]
    fn get_attested_multisig(
        &self,
        id: MultisigId,
    ) -> ProviderResult<Option<AttestedMultisigEntry>> {
        self.0.get_attested_multisig(id)
    }

    #[inline(always)]
    fn get_active_multisigs(
        &self,
    ) -> ProviderResult<Vec<AttestedMultisigEntry>> {
        self.0.get_active_multisigs()
    }

    #[inline(always)]
    fn get_multisig_records(&self) -> ProviderResult<Vec<MultisigRecord>> {
        self.0.get_multisig_records()
    }
}

impl<TX: DbTxMut + DbTx + 'static, N: NodeTypes> MultisigManagerWriter
    for BotanixDatabaseProvider<TX, N>
{
    fn insert_staging_multisig(
        &self,
        id: MultisigId,
        entry: StagedMultisigEntry,
    ) -> ProviderResult<()> {
        self.inner
            .tx_ref()
            .put::<tables::Multisigs>(
                id.into(),
                MultisigRecord::Staged { entry }.into(),
            )
            .map_err(Into::into)
    }

    fn insert_attested_multisig(
        &self,
        id: MultisigId,
        entry: AttestedMultisigEntry,
    ) -> ProviderResult<()> {
        self.inner
            .tx_ref()
            .put::<tables::Multisigs>(
                id.into(),
                MultisigRecord::Attested { entry }.into(),
            )
            .map_err(Into::into)
    }

    fn remove_multisig(&self, id: MultisigId) -> ProviderResult<bool> {
        self.inner
            .tx_ref()
            .delete::<tables::Multisigs>(id.into(), None)
            .map_err(Into::into)
    }
}

impl<DB: Database, N: NodeTypes> MultisigManagerWriter
    for BotanixDatabaseProviderRW<DB, N>
{
    #[inline(always)]
    fn insert_staging_multisig(
        &self,
        id: MultisigId,
        entry: StagedMultisigEntry,
    ) -> ProviderResult<()> {
        self.0.insert_staging_multisig(id, entry)
    }

    #[inline(always)]
    fn insert_attested_multisig(
        &self,
        id: MultisigId,
        entry: AttestedMultisigEntry,
    ) -> ProviderResult<()> {
        self.0.insert_attested_multisig(id, entry)
    }

    #[inline(always)]
    fn remove_multisig(&self, id: MultisigId) -> ProviderResult<bool> {
        self.0.remove_multisig(id)
    }
}
