use alloy_primitives::Address;
use alloy_rpc_types_eth::{Block, Header, Transaction};
use botanix_authority_edh::extra_data_header::ExtraDataHeader;
use serde::{Deserialize, Serialize};

/// Information about the Botanix Pegin Gateway Address
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct GatewayAddress {
    /// Bitcoin Pegin Address
    pub gateway_address: String,
    /// Aggregated public key used as internal taproot key
    pub aggregate_public_key: String,
    /// User Eth account
    pub eth_address: Address,
}

/// Rich block response with optional extra data header
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RichBlock<T = Transaction, H = Header> {
    /// The standard block data
    #[serde(flatten)]
    pub block: Block<T, H>,
    /// Optional extra data header from Botanix consensus
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_data_header: Option<ExtraDataHeader>,
}
