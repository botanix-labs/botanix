//! Chain specification for Botanix (Mainnet)
use crate::hardforks::botanix::BotanixHardfork;
use alloy_primitives::U256;
use botanix_chainspec::constants::{BOTANIX_MAINNET_CHAIN_ID, BOTANIX_MAINNET_GENESIS};
use reth_chainspec::{
    make_genesis_header, BaseFeeParams, BaseFeeParamsKind, Chain, ChainSpec, Head,
};
use reth_primitives::SealedHeader;

/// Returns the Botanix Mainnet chain specification.
pub fn botanix_mainnet() -> ChainSpec {
    let genesis = serde_json::from_str(include_str!("botanix_genesis.json"))
        .expect("Can't deserialize Botanix Mainnet genesis json");
    let hardforks = BotanixHardfork::botanix_mainnet();
    ChainSpec {
        chain: Chain::from_id(BOTANIX_MAINNET_CHAIN_ID),
        genesis: serde_json::from_str(include_str!("botanix_genesis.json"))
            .expect("Can't deserialize Botanix Mainnet genesis json"),
        paris_block_and_final_difficulty: Some((0, U256::from(0))),
        hardforks: BotanixHardfork::botanix_mainnet(),
        deposit_contract: None,
        base_fee_params: BaseFeeParamsKind::Constant(BaseFeeParams::new(1, 1)),
        prune_delete_limit: 3500,
        genesis_header: SealedHeader::new(
            make_genesis_header(&genesis, &hardforks),
            BOTANIX_MAINNET_GENESIS,
        ),
        ..Default::default()
    }
}

/// Returns the canonical head for Botanix Mainnet.
pub fn head() -> Head {
    Head { number: 40_000_000, timestamp: 1751250600, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use crate::chainspec::botanix::{botanix_mainnet, head};
    use alloy_primitives::hex;
    use reth_chainspec::{ForkHash, ForkId};

    #[test]
    fn can_create_forkid() {
        let b = hex::decode("098d24ac").unwrap();
        let expected = [b[0], b[1], b[2], b[3]];
        let expected_f_id = ForkId { hash: ForkHash(expected), next: 0 };

        let fork_id = botanix_mainnet().fork_id(&head());
        assert_eq!(fork_id, expected_f_id);
    }
}
