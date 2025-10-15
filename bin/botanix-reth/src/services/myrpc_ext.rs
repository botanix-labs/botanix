use botanix_rpc_client::botanix::EthBotanixApi;
use botanix_rpc_config::botanix_config::Botanix;
use botanix_rpc_types::types::GatewayAddress;
use alloy_primitives::{Address, Bytes, U256};
use jsonrpsee_core::RpcResult;
// Reth block related imports
use reth_ethereum::{provider::BlockReaderIdExt, rpc::eth::EthResult};

// Rpc related imports
use jsonrpsee::proc_macros::rpc;
use secp256k1::PublicKey;

use crate::BotanixBlock;

/// trait interface for a custom rpc namespace: `myrpcExt`
///
/// This defines an additional namespace where all methods are configured as trait functions.
#[rpc(server, namespace = "myrpcExt")]
#[async_trait::async_trait]
pub trait MyRpcExtApi {
    /// Returns the frost aggregated public key.
    #[method(name = "aggregatePublicKey")]
    async fn aggregate_public_key(&self) -> RpcResult<PublicKey>;

    /// Method to get the gateway address
    #[method(name = "getGatewayAddress")]
    async fn get_gateway_address(&self, eth_address: Address) -> RpcResult<Option<GatewayAddress>>;

    /// Method to get the merkle proof from the db
    #[method(name = "getMerkleProof")]
    async fn get_merkle_proof(&self, txid: String, block_hash: String) -> RpcResult<Bytes>;

    /// Method to get the btc fee rate
    #[method(name = "getBtcFeeRate")]
    async fn get_btc_fee_rate(&self) -> RpcResult<Option<U256>>;
}

/// The type that implements `myrpcExt` rpc namespace trait
pub struct MyRpcExt<Provider> {
    pub provider: Provider,
    pub botanix: Botanix,
}

#[async_trait::async_trait]
impl<Provider> MyRpcExtApiServer for MyRpcExt<Provider>
where
    Provider: BlockReaderIdExt<Block = BotanixBlock> + Clone + 'static,
{   
    async fn aggregate_public_key(&self) -> RpcResult<PublicKey> {
        self.botanix.aggregate_public_key().await.to_rpc_result()
    }
    
    async fn get_gateway_address(&self, eth_address: Address) ->  RpcResult<Option<GatewayAddress>> {
        self.botanix.get_gateway_address(eth_address, &self.provider).await.to_rpc_result()
    }

    async fn get_merkle_proof(&self, txid: String, block_hash:String) -> RpcResult<Bytes> {
        self.botanix.get_merkle_proof(txid, block_hash).await.to_rpc_result()
    }

    async fn get_btc_fee_rate(&self) ->  RpcResult<Option<U256>> {
        self.botanix.get_btc_fee_rate().await.to_rpc_result()
    }
}

impl<Provider> EthBotanixApi for MyRpcExt<Provider> where
    Provider: BlockReaderIdExt<Block = BotanixBlock> + Clone + 'static, {
    fn provider(&self) -> impl BlockReaderIdExt {
        self.provider.clone()
    }

    fn botanix_provider(&self) -> &Botanix {
        &self.botanix
    }
}
