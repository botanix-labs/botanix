use eyre::WrapErr;

use botanix_chainspec::BotanixChainSpec;
use botanix_cli_args::{
    frost_args::FrostArgs, poa_node::PoaNodeArgs, state_sync::StateSyncArgs,
};
use botanix_configs::federation::{
    load_federation_config_toml, FederationTomlConfig,
};
use reth::args::{DatadirArgs, NetworkArgs};
use reth_cli_util::get_secret_key;
use reth_discv4::NodeRecord;
use reth_network::frost::manager::FrostConfig;
use reth_network_peers::pk2id;
use secp256k1::{PublicKey, SecretKey, SECP256K1};
use std::net::SocketAddr;

const MIN_ALLOWED_FROST_SIGNERS: u16 = 2;

use crate::{
    consensus::{
        wallet_state_sync::WalletStateSync, AuthorityConsensusBuilder,
    },
    node::{evm::config::BotanixEvmConfig, BotanixNode},
    services::network_builder::BotanixNetworkHandle,
};

/// Result of setting up the Frost configuration for a node, containing the
/// optional FrostConfig, the socket addresses of federation authorities, the
/// node's secret key, and the genesis authorities.
#[derive(Debug)]
pub struct FrostConfigSetupResult {
    pub frost_config: Option<FrostConfig>,
    pub authorities_socket_addresses: Vec<SocketAddr>,
    pub secret_key: SecretKey,
    pub genesis_authorities: Vec<PublicKey>,
    pub federation_config: FederationTomlConfig,
}

/// Sets up the Frost configuration for a node, returning
/// `FrostConfigSetupResult`. If the node is not in federation mode,
/// `FrostConfigSetupResult.frost_config` will be `None`. Returns an error if
/// the minimum number of signers is greater than the maximum.
///
/// # Arguments
/// * `frost_args` - Arguments related to Frost configuration.
/// * `state_sync` - State synchronization arguments.
/// * `authority_pk` - The public key of the authority node.
/// * `genesis_authorities` - List of all federation authority public keys.
pub fn setup_frost(
    chain_spec: &BotanixChainSpec,
    datadir_args: &DatadirArgs,
    poa_cfg: &PoaNodeArgs,
    network_args: &NetworkArgs,
    frost_args: &FrostArgs,
    state_sync: &StateSyncArgs,
    reth_config: &mut reth_config::Config,
) -> eyre::Result<FrostConfigSetupResult> {
    // Only check if in federation mode.
    if poa_cfg.federation_mode
        && frost_args.min_signers > frost_args.max_signers
    {
        return Err(eyre::eyre!(
            "min_signers should be less than or equal to max_signers"
        ));
    }

    if poa_cfg.federation_mode && frost_args.min_signers < MIN_ALLOWED_FROST_SIGNERS
    {
        return Err(eyre::eyre!(
            "min_signers should be at least 2"
        ));
    }

    // Setup frost if in federation mode
    let data_dir = datadir_args
        .datadir
        .unwrap_or_chain_default(chain_spec.chain, datadir_args.clone());
    let network_secret_path = network_args
        .p2p_secret_key
        .clone()
        .unwrap_or_else(|| data_dir.p2p_secret());

    tracing::debug!(target: "reth::cli", ?network_secret_path, "Loading p2p key file");
    let secret_key = get_secret_key(&network_secret_path)?;
    let authority_pk = secret_key.public_key(SECP256K1);
    tracing::info!("Federation Member PubKey {:?}", authority_pk.to_string());
    tracing::info!("Federation Member Enode {:?}", pk2id(&authority_pk));

    let federation_config = load_federation_config_toml(
        &poa_cfg.federation_config_path,
    )
    .wrap_err("Failed to read federation config file")?;
    let federation_authorities =
        federation_config.get_federation_pks_from_path()?;
    let genesis_authorities = federation_authorities
        .iter()
        .map(|authority| authority.0)
        .collect::<Vec<PublicKey>>();
    let authorities_socket_addresses = federation_authorities
        .iter()
        .map(|authority| authority.1)
        .collect::<Vec<SocketAddr>>();
    if poa_cfg.federation_mode
        && federation_authorities.len() != frost_args.max_signers as usize
    {
        return Err(eyre::eyre!(
            "max_signers does not match the length of federation_authorities"
        ));
    }

    // add trusted peers from auths
    add_trusted_peers_from_authorities(
        &secret_key,
        federation_authorities.clone(),
        reth_config,
    );

    // create frost config if in federation mode
    let frost_config = if poa_cfg.federation_mode {
        let authority_index =
            genesis_authorities.iter().position(|a| *a == authority_pk).ok_or_else(|| {
                eyre::eyre!(
                    "Your public key could not be found in the list of federation public keys"
                )
            })?;

        let config = FrostConfig::new(
            authority_pk,
            authority_index,
            genesis_authorities.to_vec(),
            frost_args.min_signers,
            frost_args.max_signers,
            state_sync.wallet_state_sync_chunk_size,
        );

        tracing::info!(target: "reth::cli", "Frost config initialized");
        Some(config)
    } else {
        None
    };

    Ok(FrostConfigSetupResult {
        frost_config,
        authorities_socket_addresses,
        secret_key,
        genesis_authorities,
        federation_config,
    })
}

fn add_trusted_peers_from_authorities(
    secret_key: &SecretKey,
    authorities: Vec<(PublicKey, SocketAddr)>,
    reth_config: &mut reth_config::Config,
) {
    let self_peer_id = pk2id(&secret_key.public_key(SECP256K1));
    for authority in &authorities {
        // don't add self
        let peer_id = pk2id(&authority.0);
        if self_peer_id != peer_id {
            reth_config
                .peers
                .trusted_nodes
                .push(NodeRecord::new(authority.1, peer_id).into());
        }
    }
}
