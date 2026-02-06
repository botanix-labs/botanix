//! Defines structure for botanix RPC configurable and business logic

use alloy_primitives::U256;
use alloy_rpc_types_eth::{
    Block, BlockId, BlockNumberOrTag, Header, Transaction,
};
use botanix_authority_edh::extra_data_header::ExtraDataHeader;
use botanix_authority_edh::header_ext::HeaderExt;
use botanix_btc_wallet::{
    error::BitcoindAdapterError, fallback::FallbackBitcoindClient,
};
use botanix_rpc_types::types::RichBlock;
use btcserverlib::wallet::address::{
    generate_taproot_address, generate_tweaked_public_key,
};
use frost_secp256k1_tr::{self as frost};
use reth_rpc_eth_api::helpers::EthBlocks;
use reth_storage_api::{BlockReaderIdExt, HeaderProvider};
use std::{fmt, str::FromStr, sync::Arc};
use thiserror::Error;
use tracing::error;

use crate::impl_to_rpc_result;

/// Settings for the [`BotanixConfig`]
#[derive(Clone)]
pub struct BotanixConfig {
    /// Bitcoin network
    pub bitcoin_network: bitcoin::Network,

    /// bitcoind configuration
    pub bitcoind_factory: Arc<FallbackBitcoindClient>,
}

impl Default for BotanixConfig {
    // Creates a mocked config. Do not use in production
    fn default() -> Self {
        Self {
            bitcoin_network: bitcoin::Network::Regtest,
            // Use a public signet endpoint by default
            bitcoind_factory: Arc::new(FallbackBitcoindClient::default()),
        }
    }
}

impl BotanixConfig {
    /// Creates a new [`BotanixConfig`]
    pub const fn new(
        bitcoin_network: bitcoin::Network,
        bitcoind_factory: Arc<FallbackBitcoindClient>,
    ) -> Self {
        Self { bitcoin_network, bitcoind_factory }
    }
}

/// Errors from get gateway address RPC endpoint
#[derive(Debug, Error)]
pub enum GatewayAddressRPCError {
    /// Failed to get latest header
    #[error("Failed to get latest header")]
    FailedToGetLatestHeader,
    /// Cannot calculate gateway address for genesis block
    #[error("Cannot calculate gateway address for genesis block")]
    GenesisBlock,
    /// Failed to deserialize aggregated public key
    #[error("Frost deserialization failed {0}")]
    FailedToDeserializeAggregatedPublicKey(#[from] frost::Error),
    /// Failed to tweak the public key
    #[error("Failed to tweak the public key")]
    FailedToTweakPublicKey,
}

/// Errors from get merkle proof RPC endpoint
#[derive(Debug)]
pub enum MerkleProofRPCError {
    /// Incorrect txid format
    InvalidTxId,
    /// Failed to get txids from blockhash
    FailedToGetTxIds,
    /// txid not in block
    TxIdNotInBlock,
    /// Failed to encode Partial Merkle Tree
    FailedToEncodePartialMerkleTree(bitcoin::consensus::encode::Error),
    /// Malformed block hash
    MalformedBlockHash,
    /// Bitcoin client initialization
    BitcoindClientInitialization,
}

impl fmt::Display for MerkleProofRPCError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTxId => write!(f, "Invalid txid format"),
            Self::FailedToGetTxIds => {
                write!(f, "Failed to get txids from blockhash")
            }
            Self::TxIdNotInBlock => write!(f, "Txid not in block"),
            Self::FailedToEncodePartialMerkleTree(e) => {
                write!(f, "Failed to encode Partial Merkle Tree: {}", e)
            }
            Self::MalformedBlockHash => write!(f, "Malformed block hash"),
            Self::BitcoindClientInitialization => {
                write!(f, "Bad bitcoind client initialization")
            }
        }
    }
}

impl std::error::Error for MerkleProofRPCError {}

impl From<MerkleProofRPCError> for String {
    fn from(error: MerkleProofRPCError) -> Self {
        error.to_string()
    }
}

/// Error from get btc fee rate RPC endpoint
#[derive(Debug)]
pub enum BtcFeeRateRPCError {
    /// Failed to get estimate smart fee rate
    FailedToGetEstimateSmartFee(BitcoindAdapterError),
    /// Failed to initialize bitcoind client
    BitcoindClientInitialization,
    /// Failed to get estimate smart fee rate
    FailedToEstimateSmartFee(String),
}

impl fmt::Display for BtcFeeRateRPCError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FailedToGetEstimateSmartFee(e) => {
                write!(f, "Failed to get estimate smart fee rate: {}", e)
            }
            Self::BitcoindClientInitialization => {
                write!(f, "Failed to initialize bitcoind client")
            }
            Self::FailedToEstimateSmartFee(e) => {
                write!(f, "Failed to estimate smart fee rate: {}", e)
            }
        }
    }
}

/// Error from rich block by number RPC endpoint
#[derive(Debug, Error)]
pub enum RichBlockRPCError {
    /// Failed to get block from EthApi
    #[error("Failed to get block")]
    FailedToGetBlock,
    /// Failed to serialize block
    #[error("Failed to serialize block: {0}")]
    FailedToSerializeBlock(serde_json::Error),
    /// Failed to deserialize block
    #[error("Failed to deserialize block: {0}")]
    FailedToDeserializeBlock(serde_json::Error),
    /// Failed to deserialize extra data header
    #[error("Failed to deserialize extra data header: {0}")]
    FailedToDeserializeExtraDataHeader(String),
}

impl_to_rpc_result!(BtcFeeRateRPCError);
impl_to_rpc_result!(MerkleProofRPCError);
impl_to_rpc_result!(GatewayAddressRPCError);
impl_to_rpc_result!(RichBlockRPCError);

/// Botanix config
#[derive(Clone, Default)]
pub struct Botanix {
    /// Botanix config
    pub botanix_rpc_config: BotanixConfig,
}

impl Botanix {
    /// Creates and returns instance of [Botanix]
    pub const fn new(config: BotanixConfig) -> Self {
        Self { botanix_rpc_config: config }
    }

    /// Returns the configuration of botanix provider
    pub const fn config(&self) -> &BotanixConfig {
        &self.botanix_rpc_config
    }

    /// Function calls "`get_aggregate_public_key`"
    pub async fn get_aggregate_public_key<P>(
        &self,
        provider: &P,
    ) -> std::result::Result<secp256k1::PublicKey, GatewayAddressRPCError>
    where
        P: BlockReaderIdExt,
        <P as HeaderProvider>::Header: HeaderExt,
    {
        let latest_header = provider
            .latest_header()
            .map_err(|_| GatewayAddressRPCError::FailedToGetLatestHeader)?
            .ok_or(GatewayAddressRPCError::FailedToGetLatestHeader)?;

        if latest_header.num_hash().number == 0 {
            return Err(GatewayAddressRPCError::GenesisBlock);
        }

        let agg_pk = latest_header
            .header()
            .deserialize_extra_data_header()
            .map_err(|_| GatewayAddressRPCError::FailedToGetLatestHeader)?
            .aggregated_public_key;

        Ok(agg_pk)
    }

    /// Function calls `btc_server` to get "aggregated public key" and generated taproot gateway
    /// address
    pub async fn get_gateway_address<P>(
        &self,
        eth_address: alloy_primitives::Address,
        provider: &P,
    ) -> std::result::Result<
        (bitcoin::Address, secp256k1::PublicKey),
        GatewayAddressRPCError,
    >
    where
        P: BlockReaderIdExt,
        <P as HeaderProvider>::Header: HeaderExt,
    {
        let eth_address_bytes = eth_address.0 .0;
        let latest_header = provider
            .latest_header()
            .map_err(|_| GatewayAddressRPCError::FailedToGetLatestHeader)?
            .ok_or(GatewayAddressRPCError::FailedToGetLatestHeader)?;

        if latest_header.num_hash().number == 0 {
            return Err(GatewayAddressRPCError::GenesisBlock);
        }

        // We need to tweak the aggregated public key with the eth address
        let agg_pk = latest_header
            .header()
            .deserialize_extra_data_header()
            .map_err(|_| GatewayAddressRPCError::FailedToGetLatestHeader)?
            .aggregated_public_key;

        let vpk = frost::VerifyingKey::deserialize(&agg_pk.serialize())?;
        let tweaked_pk = generate_tweaked_public_key(&vpk, &eth_address_bytes)
            .map_err(|_| GatewayAddressRPCError::FailedToTweakPublicKey)?;
        let address = generate_taproot_address(
            &tweaked_pk,
            self.botanix_rpc_config.bitcoin_network,
        );

        Ok((address, agg_pk))
    }

    /// Function generates merkle proof for txid in a given block
    pub async fn get_merkle_proof(
        &self,
        txid: String,
        block_hash: String,
    ) -> std::result::Result<Vec<u8>, MerkleProofRPCError> {
        let tx_id: bitcoin::Txid = bitcoin::Txid::from_str(txid.as_str())
            .map_err(|_e| MerkleProofRPCError::InvalidTxId)?;

        let bitcoind_client = self.botanix_rpc_config.bitcoind_factory.clone();

        let block_hash = bitcoin::BlockHash::from_str(&block_hash)
            .map_err(|_e| MerkleProofRPCError::MalformedBlockHash)?;

        let txids = bitcoind_client
            .get_block_info_rpc(&block_hash)
            .map_err(|_e| MerkleProofRPCError::BitcoindClientInitialization)?
            .tx;

        if !txids.contains(&tx_id) {
            return Err(MerkleProofRPCError::TxIdNotInBlock);
        }

        let matches =
            txids.iter().map(|txid| txid == &tx_id).collect::<Vec<_>>();

        let pmt = bitcoin::merkle_tree::PartialMerkleTree::from_txids(
            &txids, &matches,
        );
        Ok(bitcoin::consensus::serialize(&pmt))
    }

    /// Function calls `btc_server` to get btc fee rate in BTC/kB for a pegout transaction.
    ///
    /// Converts fee rate to sat/vB and returns it.
    pub async fn get_btc_fee_rate(
        &self,
    ) -> std::result::Result<U256, BtcFeeRateRPCError> {
        let bitcoind_client = self.botanix_rpc_config.bitcoind_factory.clone();
        let fee_result = bitcoind_client
            .get_estimate_smart_fee_rpc()
            .map_err(BtcFeeRateRPCError::FailedToGetEstimateSmartFee)?;

        if let Some(fee) = fee_result.fee_rate {
            // Conversion formula
            let sat_per_vb =
                fee.to_float_in(bitcoin::Denomination::Bitcoin) * 100_000.0;
            // this really doesn't need to be a U256 can be U64
            Ok(U256::from(sat_per_vb.ceil() as u64))
        } else {
            // Use errors if available
            if let Some(errors) = fee_result.errors {
                let concatenated_errors = errors.join(", ");
                error!(
                    "Failed to get estimate smart fee rate: {}",
                    concatenated_errors
                );
                Err(BtcFeeRateRPCError::FailedToEstimateSmartFee(
                    concatenated_errors,
                ))
            } else {
                // else use default generic error
                Err(BtcFeeRateRPCError::FailedToEstimateSmartFee(
                    "Failed to get estimate smart fee rate".to_string(),
                ))
            }
        }
    }

    /// Function to get a rich block by number with optional extra data header
    pub async fn rich_block_by_number<EthApi>(
        &self,
        eth_api: &EthApi,
        number: BlockNumberOrTag,
        full: bool,
        include_extra_data_header: Option<bool>,
    ) -> std::result::Result<Option<RichBlock>, RichBlockRPCError>
    where
        EthApi: EthBlocks,
    {
        // Call the standard eth_getBlockByNumber via EthBlocks helper
        let Some(api_block) =
            EthBlocks::rpc_block(eth_api, BlockId::Number(number), full)
                .await
                .map_err(|_| RichBlockRPCError::FailedToGetBlock)?
        else {
            return Ok(None);
        };

        // Convert the block to concrete alloy types via JSON serialization
        // This is necessary because rpc_block returns EthApi-specific types
        let block: Block<Transaction, Header> = serde_json::from_value(
            serde_json::to_value(&api_block)
                .map_err(RichBlockRPCError::FailedToSerializeBlock)?,
        )
        .map_err(RichBlockRPCError::FailedToDeserializeBlock)?;

        // Create RichBlock with optional extra_data_header
        let extra_data_header = match include_extra_data_header {
            Some(true) => {
                // Deserialize the extra data header from the block's header
                let header = ExtraDataHeader::deserialize(
                    &mut block.header.extra_data.to_vec().as_slice(),
                )
                .map_err(|e| {
                    RichBlockRPCError::FailedToDeserializeExtraDataHeader(
                        e.to_string(),
                    )
                })?;
                Some(header)
            }
            _ => None,
        };
        // construct richblock
        let rich_block = RichBlock { block, extra_data_header };

        Ok(Some(rich_block))
    }
}
