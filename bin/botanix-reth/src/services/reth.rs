use botanix_cli_args::poa_node::PoaNodeArgs;
use botanix_configs::hash::verify_config_hash;
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

            tracing::info!(target: "reth::cli", path = ?config_path, "Network configuration loaded");

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
        verify_config_hash(&raw, expected_hash)
            .map_err(|e| eyre::eyre!(e.to_string()))?;
    }

    Ok(())
}
