use crate::{
    evm::{
        api::{BotanixContext, BotanixEvm},
        transaction::BotanixTxEnv,
    },
};
use botanix_chainspec::BotanixHardfork;
use reth_evm::{precompiles::PrecompilesMap, Database, EvmEnv, EvmFactory};
use revm::{
    context::result::{EVMError, HaltReason},
    inspector::NoOpInspector,
    Inspector,
};

/// Factory producing [`BotanixEvm`].
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct BotanixEvmFactory;

impl EvmFactory for BotanixEvmFactory {
    type Evm<DB: Database, I: Inspector<BotanixContext<DB>>> = BotanixEvm<DB, I>;
    type Context<DB: Database> = BotanixContext<DB>;
    type Tx = BotanixTxEnv;
    type Error<DBError: core::error::Error + Send + Sync + 'static> = EVMError<DBError>;
    type HaltReason = HaltReason;
    type Spec = BotanixHardfork;
    type Precompiles = PrecompilesMap;

    fn create_evm<DB: Database>(
        &self,
        db: DB,
        input: EvmEnv<BotanixHardfork>,
    ) -> Self::Evm<DB, NoOpInspector> {
        BotanixEvm::new(input, db, NoOpInspector {}, false)
    }

    fn create_evm_with_inspector<DB: Database, I: Inspector<Self::Context<DB>>>(
        &self,
        db: DB,
        input: EvmEnv<BotanixHardfork>,
        inspector: I,
    ) -> Self::Evm<DB, I> {
        BotanixEvm::new(input, db, inspector, true)
    }
}
