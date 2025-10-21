use alloy_primitives::Address;
use botanix_activation_manager::{ActivationManager, ActivationManagerBuilder, VoteWatcher};
use crate::botanix_authority_consensus::comet_bft::abci::{RUNTIME_VERSION_V2, RUNTIME_VERSION_V3};
use botanix_cli_args::{chain::BotanixNetwork};
use botanix_storage::models::Vote;


/// Sets up and returns an `ActivationManager` configured for the given Botanix network.
pub fn setup_activation_manager(botanix_network: &BotanixNetwork) -> ActivationManager<VoteWatcher, Address> {
    // ActivationManager: setup parameters and conditions.
    let quorum;
    let min_validator_count;
    let target_height;
    let our_vote;

    // Setting all the values on a per network basis in case they are different in the future.
    #[allow(clippy::branches_sharing_code)]
    if botanix_network.is_testnet() {
        quorum = 100; // 100% ~= 3/3 members must approve
        min_validator_count = 3;
        target_height = 1; // Activate as soon as possible
        our_vote = Vote::Aye;
    } else if botanix_network.is_devnet() {
        quorum = 100; // 100% ~= 6/6 members must approve
        min_validator_count = 6;
        target_height = 1; // Activate as soon as possible
        our_vote = Vote::Aye;
    } else {
        quorum = 100; // 100% ~= 16/16 members must approve
        min_validator_count = 16;
        target_height = 1; // Activate as soon as possible
        our_vote = Vote::Aye;
    }

    // ActivationManager: setup compliance and vote inclusion.
    let activation_manager =
        ActivationManagerBuilder::new(VoteWatcher::default(), RUNTIME_VERSION_V2)
            .build_COMPLIANT_network_upgrade(
                RUNTIME_VERSION_V3,
                quorum,
                min_validator_count,
                target_height,
                Some(our_vote),
            );
    activation_manager
}