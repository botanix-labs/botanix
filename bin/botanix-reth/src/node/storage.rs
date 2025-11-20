use crate::{BotanixBlock, BotanixBlockBody, BotanixPrimitives};
use reth_chainspec::EthereumHardforks;
use reth_db::transaction::{DbTx, DbTxMut};
use reth_provider::{
    providers::{ChainStorage, NodeTypesForProvider},
    BlockBodyReader, BlockBodyWriter, ChainSpecProvider, ChainStorageReader,
    ChainStorageWriter, DBProvider, DatabaseProvider, EthStorage,
    ProviderResult, ReadBodyInput, StorageLocation,
};

/// Storage wrapper for Botanix-specific block bodies and sidecars.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct BotanixStorage(EthStorage);

impl<Provider> BlockBodyWriter<Provider, BotanixBlockBody> for BotanixStorage
where
    Provider: DBProvider<Tx: DbTxMut>,
{
    fn write_block_bodies(
        &self,
        provider: &Provider,
        bodies: Vec<(u64, Option<BotanixBlockBody>)>,
        write_to: StorageLocation,
    ) -> ProviderResult<()> {
        // Convert BotanixBlockBody to underlying BlockBody
        let bodies = bodies
            .into_iter()
            .map(|(num, body)| (num, body.map(|b| b.inner)))
            .collect();
        self.0.write_block_bodies(provider, bodies, write_to)?;

        Ok(())
    }

    fn remove_block_bodies_above(
        &self,
        provider: &Provider,
        block: u64,
        remove_from: StorageLocation,
    ) -> ProviderResult<()> {
        self.0.remove_block_bodies_above(provider, block, remove_from)?;

        // TODO: Remove sidecars

        Ok(())
    }
}

impl<Provider> BlockBodyReader<Provider> for BotanixStorage
where
    Provider: DBProvider + ChainSpecProvider<ChainSpec: EthereumHardforks>,
{
    type Block = BotanixBlock;

    fn read_block_bodies(
        &self,
        provider: &Provider,
        inputs: Vec<ReadBodyInput<'_, Self::Block>>,
    ) -> ProviderResult<Vec<BotanixBlockBody>> {
        let eth_bodies = self.0.read_block_bodies(provider, inputs)?;

        // TODO: Read pegins, pegouts

        Ok(eth_bodies
            .into_iter()
            .map(|inner| BotanixBlockBody { inner })
            .collect())
    }
}

impl ChainStorage<BotanixPrimitives> for BotanixStorage {
    fn reader<TX, Types>(
        &self,
    ) -> impl ChainStorageReader<DatabaseProvider<TX, Types>, BotanixPrimitives>
    where
        TX: DbTx + 'static,
        Types: NodeTypesForProvider<Primitives = BotanixPrimitives>,
    {
        self
    }

    fn writer<TX, Types>(
        &self,
    ) -> impl ChainStorageWriter<DatabaseProvider<TX, Types>, BotanixPrimitives>
    where
        TX: DbTxMut + DbTx + 'static,
        Types: NodeTypesForProvider<Primitives = BotanixPrimitives>,
    {
        self
    }
}
