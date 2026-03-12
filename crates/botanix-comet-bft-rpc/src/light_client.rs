use std::time::Duration;

use super::{Client, CometBftRpcFactory, HttpCometBFTRpcClientFactory};
use eyre::Context;
use tendermint_light_client::{
    builder::LightClientBuilder,
    instance::Instance,
    light_client::Options,
    store::{memory::MemoryStore, LightStore},
    types::{Height, TrustThreshold},
};

/// Builds a light client from a comet bft rpc client factory and a light store
#[derive(Debug)]
pub struct LightCBFTClientBuilder {
    /// The rpc client factory
    rpc_client_factory: HttpCometBFTRpcClientFactory,
    /// The light store, best to use [MemoryStore] from the tendermint_light_client crate
    light_store: Box<dyn LightStore>,
}

impl LightCBFTClientBuilder {
    /// Create a new light client builder
    pub fn new(rpc_client_factory: HttpCometBFTRpcClientFactory) -> Self {
        let light_store = Box::new(MemoryStore::new());

        Self { rpc_client_factory, light_store }
    }

    /// Set the light store
    pub fn with_light_store(
        mut self,
        light_store: Box<dyn LightStore>,
    ) -> Self {
        self.light_store = light_store;
        self
    }

    /// Build and verify the light client starting at block height 1
    pub async fn build_and_verify(&self) -> eyre::Result<Instance> {
        let light_store = Box::new(MemoryStore::new());

        let rpc_client = self
            .rpc_client_factory
            .build_and_connect()
            .context("Failed to connect to RPC client")?;
        let node_id = rpc_client
            .status()
            .await
            .context("Failed to get node info")?
            .node_info
            .id;

        let trusted_block_height = 1u32;
        let block_hash = rpc_client
            .block(trusted_block_height)
            .await
            .context("to have first block")?
            .block
            .header
            .hash();

        let light_client = match LightClientBuilder::prod(
            node_id,
            rpc_client.clone(),
            light_store,
            Options {
                trust_threshold: TrustThreshold::TWO_THIRDS,
                // 2 week trusting period
                trusting_period: Duration::from_secs(1209600),
                // 3 seconds clock drift
                clock_drift: Duration::from_secs(5),
            },
            None,
        )
        .trust_primary_at(Height::from(trusted_block_height), block_hash)
        {
            Ok(light_client) => light_client,
            Err(e) => {
                tracing::error!("Failed to create light client: {:?}", e);
                return Err(eyre::eyre!(
                    "Failed to create light client: {}",
                    e
                ));
            }
        };

        Ok(light_client.build())
    }
}
