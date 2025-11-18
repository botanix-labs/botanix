use alloy_primitives::{Address, Bytes, U256};
use botanix_rpc_client::botanix::EthBotanixApi;
use botanix_rpc_config::{botanix_config::Botanix, result::ToRpcResult};
use botanix_rpc_types::types::GatewayAddress;
use jsonrpsee_core::RpcResult;
// Reth block related imports
use reth_ethereum::provider::BlockReaderIdExt;

// Rpc related imports
use jsonrpsee::proc_macros::rpc;
use secp256k1::PublicKey;

use crate::BotanixBlock;

/// trait interface for a custom rpc namespace: `botanixrpcExt`
///
/// This defines an additional namespace where all methods are configured as trait functions.
#[rpc(server, namespace = "botanixrpcExt")]
#[async_trait::async_trait]
pub trait BotanixRpcExtApi {
    /// Returns the frost aggregated public key.
    #[method(name = "aggregatePublicKey")]
    async fn aggregate_public_key(&self) -> RpcResult<PublicKey>;

    /// Method to get the gateway address
    #[method(name = "getGatewayAddress")]
    async fn get_gateway_address(
        &self,
        eth_address: Address,
    ) -> RpcResult<Option<GatewayAddress>>;

    /// Method to get the merkle proof from the db
    #[method(name = "getMerkleProof")]
    async fn get_merkle_proof(
        &self,
        txid: String,
        block_hash: String,
    ) -> RpcResult<Bytes>;

    /// Method to get the btc fee rate
    #[method(name = "getBtcFeeRate")]
    async fn get_btc_fee_rate(&self) -> RpcResult<Option<U256>>;
}

/// The type that implements `botanixrpcExt` rpc namespace trait
pub struct BotanixRpcExt<Provider> {
    /// The Ethereum provider used to read blocks.
    pub provider: Provider,
    /// Botanix client and configuration.
    pub botanix: Botanix,
}

#[async_trait::async_trait]
impl<Provider> BotanixRpcExtApiServer for BotanixRpcExt<Provider>
where
    Provider: BlockReaderIdExt<Block = BotanixBlock> + Clone + 'static,
{
    async fn aggregate_public_key(&self) -> RpcResult<PublicKey> {
        self.botanix
            .get_aggregate_public_key(&self.provider)
            .await
            .to_rpc_result()
    }

    async fn get_gateway_address(
        &self,
        eth_address: Address,
    ) -> RpcResult<Option<GatewayAddress>> {
        let (address, public_key) = self
            .botanix
            .get_gateway_address(eth_address, &self.provider)
            .await
            .to_rpc_result()?;
        Ok(Some(GatewayAddress {
            gateway_address: address.to_string(),
            aggregate_public_key: public_key.to_string(),
            eth_address,
        }))
    }

    async fn get_merkle_proof(
        &self,
        txid: String,
        block_hash: String,
    ) -> RpcResult<Bytes> {
        self.botanix
            .get_merkle_proof(txid, block_hash)
            .await
            .map(Bytes::from)
            .to_rpc_result()
    }

    async fn get_btc_fee_rate(&self) -> RpcResult<Option<U256>> {
        self.botanix.get_btc_fee_rate().await.map(Some).to_rpc_result()
    }
}

impl<Provider> EthBotanixApi for BotanixRpcExt<Provider>
where
    Provider: BlockReaderIdExt<Block = BotanixBlock> + Clone + 'static,
{
    fn provider(&self) -> impl BlockReaderIdExt {
        self.provider.clone()
    }

    fn botanix_provider(&self) -> &Botanix {
        &self.botanix
    }
}
