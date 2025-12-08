use std::collections::HashMap;

use crate::{
    tables, BotanixDatabaseProvider, BotanixDatabaseProviderRW,
    FoundationLayerReader, FoundationLayerWriter,
};
use botanix_tem::{
    foundation::{
        bitcoin::{BlockHash, OutPoint, Txid},
        OnchainHeaderEntry, OnchainUtxoEntry, ProposalEntry, UnassignedEntry,
    },
    validation::pegout::PegoutId,
};
use itertools::Itertools;
use reth_db::cursor::DbCursorRO;
use reth_db_api::{
    transaction::{DbTx, DbTxMut},
    Database,
};
use reth_node_types::NodeTypes;
use reth_provider::DBProvider;
use reth_storage_errors::provider::ProviderResult;

impl<TX: DbTx + 'static, N: NodeTypes> FoundationLayerReader
    for BotanixDatabaseProvider<TX, N>
{
    fn get_unassigned_pegout(
        &self,
        id: PegoutId,
    ) -> ProviderResult<Option<UnassignedEntry>> {
        Ok(self
            .inner
            .tx_ref()
            .get::<tables::UnassignedPegouts>(id.into())?
            .map(|v| v.0))
    }
    fn get_onchain_utxo(
        &self,
        utxo: OutPoint,
    ) -> ProviderResult<Option<OnchainUtxoEntry>> {
        Ok(self
            .inner
            .tx_ref()
            .get::<tables::OnchainUtxos>(utxo.into())?
            .map(|v| v.0))
    }
    fn get_onchain_header(
        &self,
        header: BlockHash,
    ) -> ProviderResult<Option<OnchainHeaderEntry>> {
        Ok(self
            .inner
            .tx_ref()
            .get::<tables::OnchainHeaders>(header.into())?
            .map(|v| v.0))
    }
    fn get_onchain_headers(&self) -> ProviderResult<Vec<OnchainHeaderEntry>> {
        Ok(self
            .inner
            .tx_ref()
            .cursor_read::<tables::OnchainHeaders>()?
            .walk(None)?
            .collect::<Result<HashMap<_, _>, _>>()?
            .values()
            .map(|v| &v.0)
            .cloned()
            .sorted_by_key(|v| v.height)
            .collect::<Vec<_>>())
    }
    fn get_pegout_proposal(
        &self,
        txid: Txid,
    ) -> ProviderResult<Option<ProposalEntry>> {
        Ok(self
            .inner
            .tx_ref()
            .get::<tables::PegoutProposals>(txid.into())?
            .map(|v| v.0))
    }
    fn get_foundation_commitment(
        &self,
        key: [u8; 32],
    ) -> ProviderResult<Option<Vec<u8>>> {
        Ok(self
            .inner
            .tx_ref()
            .get::<tables::FoundationCommitments>(key.into())?
            .map(|v| v.0))
    }
    fn get_foundation_commitment_root(
        &self,
    ) -> ProviderResult<Option<[u8; 32]>> {
        Ok(self
            .inner
            .tx_ref()
            .get::<tables::FoundationCommitmentRoots>(().into())?
            .map(|v| v.0))
    }
}

impl<DB: Database, N: NodeTypes> FoundationLayerReader
    for BotanixDatabaseProviderRW<DB, N>
{
    #[inline(always)]
    fn get_unassigned_pegout(
        &self,
        id: PegoutId,
    ) -> ProviderResult<Option<UnassignedEntry>> {
        self.0.get_unassigned_pegout(id)
    }
    #[inline(always)]
    fn get_onchain_utxo(
        &self,
        utxo: OutPoint,
    ) -> ProviderResult<Option<OnchainUtxoEntry>> {
        self.0.get_onchain_utxo(utxo)
    }
    #[inline(always)]
    fn get_onchain_header(
        &self,
        header: BlockHash,
    ) -> ProviderResult<Option<OnchainHeaderEntry>> {
        self.0.get_onchain_header(header)
    }
    #[inline(always)]
    fn get_onchain_headers(&self) -> ProviderResult<Vec<OnchainHeaderEntry>> {
        self.0.get_onchain_headers()
    }
    #[inline(always)]
    fn get_pegout_proposal(
        &self,
        txid: Txid,
    ) -> ProviderResult<Option<ProposalEntry>> {
        self.0.get_pegout_proposal(txid)
    }
    #[inline(always)]
    fn get_foundation_commitment(
        &self,
        key: [u8; 32],
    ) -> ProviderResult<Option<Vec<u8>>> {
        self.0.get_foundation_commitment(key)
    }
    #[inline(always)]
    fn get_foundation_commitment_root(
        &self,
    ) -> ProviderResult<Option<[u8; 32]>> {
        self.0.get_foundation_commitment_root()
    }
}

impl<DB: Database, N: NodeTypes> FoundationLayerWriter
    for BotanixDatabaseProviderRW<DB, N>
{
    #[inline(always)]
    fn insert_unassigned_pegout(
        &self,
        id: PegoutId,
        entry: UnassignedEntry,
    ) -> ProviderResult<()> {
        self.inner
            .tx_ref()
            .put::<tables::UnassignedPegouts>(id.into(), entry.into())
            .map_err(Into::into)
    }
    #[inline(always)]
    fn remove_unassigned_pegout(&self, id: PegoutId) -> ProviderResult<bool> {
        self.inner
            .tx_ref()
            .delete::<tables::UnassignedPegouts>(id.into(), None)
            .map_err(Into::into)
    }
    #[inline(always)]
    fn insert_onchain_utxo(
        &self,
        utxo: OutPoint,
        entry: OnchainUtxoEntry,
    ) -> ProviderResult<()> {
        self.inner
            .tx_ref()
            .put::<tables::OnchainUtxos>(utxo.into(), entry.into())
            .map_err(Into::into)
    }
    #[inline(always)]
    fn remove_onchain_utxo(&self, utxo: OutPoint) -> ProviderResult<bool> {
        self.tx_ref()
            .delete::<tables::OnchainUtxos>(utxo.into(), None)
            .map_err(Into::into)
    }
    #[inline(always)]
    fn insert_onchain_header(
        &self,
        header: BlockHash,
        entry: OnchainHeaderEntry,
    ) -> ProviderResult<()> {
        self.inner
            .tx_ref()
            .put::<tables::OnchainHeaders>(header.into(), entry.into())
            .map_err(Into::into)
    }
    #[inline(always)]
    fn remove_onchain_header(&self, header: BlockHash) -> ProviderResult<bool> {
        self.inner
            .tx_ref()
            .delete::<tables::OnchainHeaders>(header.into(), None)
            .map_err(Into::into)
    }
    #[inline(always)]
    fn insert_pegout_proposal(
        &self,
        txid: Txid,
        entry: ProposalEntry,
    ) -> ProviderResult<()> {
        self.inner
            .tx_ref()
            .put::<tables::PegoutProposals>(txid.into(), entry.into())
            .map_err(Into::into)
    }
    #[inline(always)]
    fn remove_pegout_proposal(&self, txid: Txid) -> ProviderResult<bool> {
        self.inner
            .tx_ref()
            .delete::<tables::PegoutProposals>(txid.into(), None)
            .map_err(Into::into)
    }
    #[inline(always)]
    fn insert_foundation_commitment(
        &self,
        key: [u8; 32],
        value: Vec<u8>,
    ) -> ProviderResult<()> {
        self.inner
            .tx_ref()
            .put::<tables::FoundationCommitments>(key.into(), value.into())
            .map_err(Into::into)
    }
    #[inline(always)]
    fn remove_foundation_commitment(
        &self,
        key: [u8; 32],
    ) -> ProviderResult<bool> {
        self.inner
            .tx_ref()
            .delete::<tables::FoundationCommitments>(key.into(), None)
            .map_err(Into::into)
    }
    #[inline(always)]
    fn insert_foundation_commitment_root(
        &self,
        root: [u8; 32],
    ) -> ProviderResult<()> {
        self.inner
            .tx_ref()
            .put::<tables::FoundationCommitmentRoots>(().into(), root.into())
            .map_err(Into::into)
    }
}
