use std::sync::Arc;

use botanix_btc_wallet::{
    bitcoind::{
        BitcoindClient, BitcoindClientFactory, BitcoindConfig, BitcoindFactory,
    },
    fallback::{
        BitcoindClientWrapper, ClientSelection, FallbackBitcoindClient,
    },
};
use botanix_cli_args::{bitcoind::BitcoindArgs, poa_node::PoaNodeArgs};
use eyre::{Context, Ok};

fn create_bitcoind_client(
    bitcoind_config: &BitcoindConfig,
) -> eyre::Result<(BitcoindClient, BitcoindClientFactory)> {
    let bitcoind_factory = BitcoindClientFactory::new(bitcoind_config.clone());

    // create bitcoind client and make sure its synced
    let bitcoind_client =
        bitcoind_factory.build_and_connect().wrap_err_with(|| {
            format!(
                "Could build and connect to bitcoind at {}",
                bitcoind_config.url
            )
        })?;
    Ok((bitcoind_client, bitcoind_factory))
}

fn get_bitcoind_config(
    poa_cfg: &PoaNodeArgs,
    bitcoind_cfg: &BitcoindArgs,
    is_primary: bool,
) -> eyre::Result<BitcoindConfig> {
    let mut bitcoind_config: BitcoindConfig = bitcoind_cfg.clone().into();

    let bitcoind_config_path = if is_primary {
        &poa_cfg.bitcoind_config_path
    } else {
        &poa_cfg.bitcoind_fallback_config_path
    };

    if let Some(bitcoind_config_path) = &bitcoind_config_path {
        let config = confy::load_path::<BitcoindArgs>(&bitcoind_config_path)
            .wrap_err_with(|| {
                format!("Could not load config file {:?}", bitcoind_config_path)
            })?;

        tracing::info!(target: "reth::cli", path = ?bitcoind_config_path, "Bitcoind config loaded from file");
        bitcoind_config = config.into();
    }
    Ok(bitcoind_config)
}

/// Sets up and returns a Bitcoind client using the provided configuration arguments.
pub async fn setup_bitcoind_client(
    bitcoind_cfg: &BitcoindArgs,
    poa_cfg: &PoaNodeArgs,
    client_selection: ClientSelection,
) -> eyre::Result<(FallbackBitcoindClient, BitcoindClientFactory)> {
    let primary_bitcoind_config =
        get_bitcoind_config(poa_cfg, bitcoind_cfg, true)?;
    let (primary_bitcoind_client, primary_bitcoind_factory) =
        create_bitcoind_client(&primary_bitcoind_config)?;

    let secondary_bitcoind_config =
        get_bitcoind_config(poa_cfg, bitcoind_cfg, false)?;
    let (secondary_bitcoind_client, _secondary_bitcoind_factory) =
        create_bitcoind_client(&secondary_bitcoind_config)?;

    let fallback_client = FallbackBitcoindClient::new(
        vec![
            BitcoindClientWrapper::Provider1(Arc::new(primary_bitcoind_client)),
            BitcoindClientWrapper::Provider2(Arc::new(
                secondary_bitcoind_client,
            )),
        ],
        client_selection,
    );

    Ok((fallback_client, primary_bitcoind_factory))
}
