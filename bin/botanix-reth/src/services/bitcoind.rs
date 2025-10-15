use botanix_btc_wallet::bitcoind::{BitcoindClient, BitcoindClientFactory, BitcoindConfig, BitcoindFactory};
use botanix_cli_args::{bitcoind::BitcoindArgs, poa_node::PoaNodeArgs};
use eyre::{Context, Ok};

/// Sets up and returns a Bitcoind client using the provided configuration arguments.
pub async fn setup_bitcoind_client(
    bitcoind_cfg: &BitcoindArgs,
    poa_cfg: &PoaNodeArgs
) -> eyre::Result<(BitcoindClient, BitcoindClientFactory)> {
    let mut bitcoind_config: BitcoindConfig = bitcoind_cfg.clone().into();
    // prioritize the bitcoind config path from cli args
    if let Some(bitcoind_config_path) = &poa_cfg.bitcoind_config_path {
        let config =
            confy::load_path::<BitcoindArgs>(&bitcoind_config_path).wrap_err_with(|| {
                format!("Could not load config file {:?}", bitcoind_config_path)
            })?;

        tracing::info!(target: "reth::cli", path = ?bitcoind_config_path, "Bitcoind config loaded from file");
        bitcoind_config = config.into();
    }
    let bitcoind_factory = BitcoindClientFactory::new(bitcoind_config.clone());

    // create bitcoind client and make sure its synced
    let bitcoind_client = bitcoind_factory.build_and_connect().wrap_err_with(|| { format!("Could build and connect to bitcoind at {}", bitcoind_config.url)})?;
    Ok((bitcoind_client, bitcoind_factory))
}