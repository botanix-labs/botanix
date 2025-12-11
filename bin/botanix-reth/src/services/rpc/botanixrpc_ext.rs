use alloy_primitives::{Address, Bytes, U256};
use alloy_rpc_types_eth::{BlockNumberOrTag};
use botanix_authority_edh::header_ext::HeaderExt;
use botanix_rpc_client::botanix::EthBotanixApi;
use botanix_rpc_config::{botanix_config::Botanix, result::ToRpcResult};
use botanix_rpc_types::types::{GatewayAddress, RichBlock};
use jsonrpsee_core::RpcResult;
// Reth block related imports
use reth_ethereum::provider::BlockReaderIdExt;
use reth_provider::HeaderProvider;
use reth_rpc_eth_api::helpers::EthBlocks;

// Rpc related imports
use jsonrpsee::proc_macros::rpc;
use secp256k1::PublicKey;

use crate::BotanixBlock;

/// trait interface for a custom rpc namespace: `eth`
///
/// This defines an additional namespace where all methods are configured as trait functions.
#[rpc(server, namespace = "eth")]
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

    /// Method to get a rich block by number with optional extra data header
    #[method(name = "richBlockByNumber")]
    async fn rich_block_by_number(
        &self,
        number: BlockNumberOrTag,
        full: bool,
        include_extra_data_header: Option<bool>,
    ) -> RpcResult<Option<RichBlock>>;

}

/// The type that implements `botanixrpcExt` rpc namespace trait
pub struct BotanixRpcExt<Provider, EthApi> {
    /// The Ethereum provider used to read blocks.
    pub provider: Provider,
    /// Botanix client and configuration.
    pub botanix: Botanix,
    /// The Ethereum API for calling standard eth methods
    pub eth_api: EthApi,
}

#[async_trait::async_trait]
impl<Provider, EthApi> BotanixRpcExtApiServer for BotanixRpcExt<Provider, EthApi>
where
    Provider: BlockReaderIdExt<Block = BotanixBlock> + Clone + 'static,
    <Provider as HeaderProvider>::Header: HeaderExt,
    EthApi: EthBlocks + Send + Sync + 'static,
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

    /// Handler for: `eth_richBlockByNumber`
    async fn rich_block_by_number(
        &self,
        number: BlockNumberOrTag,
        full: bool,
        include_extra_data_header: Option<bool>,
    ) -> RpcResult<Option<RichBlock>> {
        self.botanix
            .rich_block_by_number(&self.eth_api, number, full, include_extra_data_header)
            .await
            .to_rpc_result()
    }

}

impl<Provider, Eth> EthBotanixApi for BotanixRpcExt<Provider, Eth>
where
    Provider: BlockReaderIdExt<Block = BotanixBlock> + Clone + 'static,
    <Provider as HeaderProvider>::Header: HeaderExt,
    Eth: Send + Sync + 'static,
{
    fn provider(&self) -> impl BlockReaderIdExt {
        self.provider.clone()
    }

    fn botanix_provider(&self) -> &Botanix {
        &self.botanix
    }
}
