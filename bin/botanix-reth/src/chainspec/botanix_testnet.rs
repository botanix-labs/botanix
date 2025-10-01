//! Chain specification for Botanix Testnet
use crate::hardforks::botanix::BotanixHardfork;
use alloy_primitives::U256;
use botanix_chainspec::constants::{BOTANIX_TESTNET_CHAIN_ID, BOTANIX_TESTNET_GENESIS};
use reth_chainspec::{
    make_genesis_header, BaseFeeParams, BaseFeeParamsKind, Chain, ChainSpec, Head,
};
use reth_primitives::SealedHeader;

/// Returns the chain specification for the Botanix Testnet.
pub fn botanix_testnet() -> ChainSpec {
    let genesis = serde_json::from_str(include_str!("botanix_testnet.json"))
        .expect("Can't deserialize Botanix Testnet genesis json");
    let hardforks = BotanixHardfork::botanix_testnet();
    ChainSpec {
        chain: Chain::from_id(BOTANIX_TESTNET_CHAIN_ID),
        genesis: serde_json::from_str(include_str!("botanix_testnet.json"))
            .expect("Can't deserialize Botanix Testnet genesis json"),
        paris_block_and_final_difficulty: Some((0, U256::from(0))),
        hardforks: BotanixHardfork::botanix_testnet(),
        deposit_contract: None,
        base_fee_params: BaseFeeParamsKind::Constant(BaseFeeParams::new(1, 1)),
        prune_delete_limit: 3500,
        genesis_header: SealedHeader::new(
            make_genesis_header(&genesis, &hardforks),
            BOTANIX_TESTNET_GENESIS,
        ),
        ..Default::default()
    }
}

/// Dummy Head for Botanix Testnet
pub fn head() -> Head {
    Head {
        number: 57_638_970,
        hash: BOTANIX_TESTNET_GENESIS,
        difficulty: U256::from(2),
        total_difficulty: U256::from(115_030_996),
        timestamp: 1752059605,
    }
}
