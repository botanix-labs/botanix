use super::BotanixEngineApi;
use reth::{
    api::{AddOnsContext, FullNodeComponents},
    builder::rpc::EngineApiBuilder,
};

/// Builder for mocked [`BotanixEngineApi`] implementation.
#[derive(Debug, Default)]
pub struct BotanixEngineApiBuilder;

impl<N> EngineApiBuilder<N> for BotanixEngineApiBuilder
where
    N: FullNodeComponents,
{
    type EngineApi = BotanixEngineApi;

    async fn build_engine_api(
        self,
        _ctx: &AddOnsContext<'_, N>,
    ) -> eyre::Result<Self::EngineApi> {
        Ok(BotanixEngineApi::default())
    }
}
