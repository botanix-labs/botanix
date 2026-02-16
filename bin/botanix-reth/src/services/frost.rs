use botanix_chainspec::BotanixChainSpec;
use botanix_configs::federation::FederationTomlConfig;
use botanix_types::MultisigId;
use reth::args::{DatadirArgs, NetworkArgs};
use reth_cli_util::get_secret_key;
use reth_discv4::NodeRecord;
use reth_network_peers::pk2id;
use secp256k1::{PublicKey, SecretKey, SECP256K1};
use std::net::SocketAddr;

/// Configuration for a single federation multisig, representing one epoch in
/// the dynafed lifecycle.
#[derive(Debug, Clone)]
pub struct MultisigConfig {
    /// Identifier for this multisig.
    pub multisig_id: MultisigId,
    /// Minimum number of signers required to produce a valid signature.
    pub min_signers: u16,
    /// Total number of signers for this multisig (defaults to member count).
    pub max_signers: u16,
    /// Public keys of all members participating in this multisig.
    pub authorities: Vec<secp256k1::PublicKey>,
    /// Index of the current node within the `authorities` list.
    pub authority_index: usize,
}

/// Result of setting up the Frost configuration for a node.
#[derive(Debug, Clone)]
pub struct FrostConfigSetupResult {
    /// The node's secret key, used for peer identity and signing.
    pub secret_key: SecretKey,
    /// The multisig configurations for each federation epoch.
    pub multisigs: Vec<MultisigConfig>,
}

impl FrostConfigSetupResult {
    /// Returns the combined list of authority public keys across all multisig
    /// configurations.
    pub fn authorities(&self) -> Vec<secp256k1::PublicKey> {
        self.multisigs
            .iter()
            .map(|m| m.authorities.to_vec())
            .flatten()
            .collect()
    }
}

/// Sets up the Frost configuration for a node by loading the federation config
/// and resolving the multisig parameters and configurations.
pub fn setup_frost(
    chain_spec: &BotanixChainSpec,
    datadir_args: &DatadirArgs,
    federation_config: FederationTomlConfig,
    network_args: &NetworkArgs,
    reth_config: &mut reth_config::Config,
) -> eyre::Result<FrostConfigSetupResult> {
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

    // Add trusted nodes with va the `--trusted-peers` CLI flag.
    tracing::info!(target: "reth::cli", "Adding trusted nodes");
    if !network_args.trusted_peers.is_empty() {
        network_args.trusted_peers.iter().for_each(|peer| {
            reth_config.peers.trusted_nodes.push(peer.clone());
        });
    }

    // Compose *all* federation authorities.
    let federation_authorities = federation_config.get_federation_addrs()?;

    // Add trusted peers from auths
    add_trusted_peers_from_authorities(
        &secret_key,
        federation_authorities.clone(),
        reth_config,
    );

    let multisigs = federation_config
        .multisigs
        .into_iter()
        .map(|m| {
            let authorities = m.get_federation_pub_keys()?;

            let authority_index = authorities.iter().position(|a| *a == authority_pk).ok_or_else(|| {
                eyre::eyre!(
                    "Your public key could not be found in the list of federation public keys"
                )
            })?;

            // TODO: Do basic validation?
            Ok(MultisigConfig {
                multisig_id: m.multisig_id,
                min_signers: m.min_signers,
                max_signers: authorities.len() as u16,
                authorities,
                authority_index,
            })
        })
        .collect::<eyre::Result<Vec<_>>>()?;

    Ok(FrostConfigSetupResult { secret_key, multisigs })
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
