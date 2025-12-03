use bitcoin::hashes::{sha256, Hash};
use botanix_cli_args::poa_node::PoaNodeArgs;
use eyre::{Context, Ok};
use reth::args::NetworkArgs;
use std::{fs, path::PathBuf};

/// Loads the Reth configuration using the provided PoA and network arguments.
pub fn load_reth_config(
    poa_args: &PoaNodeArgs,
    network_args: &NetworkArgs,
) -> eyre::Result<reth_config::Config> {
    match <std::option::Option<PathBuf> as Clone>::clone(
        &poa_args.network_config_path,
    ) {
        Some(config_path) => {
            let mut config =
                confy::load_path::<reth_config::Config>(&config_path)
                    .wrap_err_with(|| {
                        format!("Could not load config file {:?}", config_path)
                    })?;

            tracing::info!(target: "reth::cli", path = ?config_path, "Network onfiguration loaded");

            // Update the config with the command line arguments
            config.peers.trusted_nodes_only = network_args.trusted_only;

            if !network_args.trusted_peers.is_empty() {
                tracing::info!(target: "reth::cli", "Adding trusted nodes");
                network_args.trusted_peers.iter().for_each(|peer| {
                    config.peers.trusted_nodes.push(peer.clone());
                });
            }
            Ok(config)
        }
        None => Ok(reth_config::Config::default()),
    }
}

/// Validates that the federation config content matches the provided config
/// hash flag.
pub fn verify_federation_config_hash(
    poa_args: &PoaNodeArgs,
) -> eyre::Result<()> {
    if let Some(expected_hash) = &poa_args.federation_config_hash {
        let raw = fs::read_to_string(&poa_args.federation_config_path)
            .wrap_err_with(|| {
                format!(
                    "Could not read federation config file {:?}",
                    poa_args.federation_config_path
                )
            })?;
        verify_raw_config_hash(&raw, expected_hash);
    }

    Ok(())
}

fn verify_raw_config_hash(raw: &str, expected_hash: &str) {
    let normalized_expected = normalize_hash(expected_hash);
    if normalized_expected.is_empty() {
        panic!("provided federation config hash must not be empty");
    }

    let computed = compute_config_hash(raw);
    if normalized_expected != computed {
        panic!(
            "federation config hash mismatch: expected {}, found {}",
            normalized_expected, computed
        );
    }
}

fn compute_config_hash(raw: &str) -> String {
    sha256::Hash::hash(raw.as_bytes()).to_string()
}

fn normalize_hash(value: &str) -> String {
    value.trim().trim_start_matches("0x").to_ascii_lowercase()
}
