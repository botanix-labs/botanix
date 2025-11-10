use std::sync::Arc;

use botanix_btc_wallet::{
    bitcoind::{
        BitcoindClient, BitcoindClientFactory, BitcoindConfig, BitcoindFactory,
    },
    fallback::{
        BitcoindClientWrapper, ClientSelection, FallbackBitcoindClient,
    },
};
use botanix_cli_args::bitcoind::BitcoindArgs;
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
    bitcoind_cfg: &BitcoindArgs,
    is_primary: bool,
) -> eyre::Result<BitcoindConfig> {
    let bitcoind_config = if is_primary {
        BitcoindConfig {
            url: bitcoind_cfg.primary_url.clone(),
            username: bitcoind_cfg.primary_username.clone(),
            password: bitcoind_cfg.primary_password.clone(),
        }
    } else {
        BitcoindConfig {
            url: bitcoind_cfg.secondary_url.clone(),
            username: bitcoind_cfg.secondary_username.clone(),
            password: bitcoind_cfg.secondary_password.clone(),
        }
    };
    Ok(bitcoind_config)
}

/// Sets up and returns a Bitcoind client using the provided configuration arguments.
pub async fn setup_bitcoind_client(
    bitcoind_cfg: &BitcoindArgs,
    client_selection: ClientSelection,
) -> eyre::Result<FallbackBitcoindClient> {
    let primary_bitcoind_config = get_bitcoind_config(bitcoind_cfg, true)?;
    let (primary_bitcoind_client, _) =
        create_bitcoind_client(&primary_bitcoind_config)?;

    let secondary_bitcoind_config = get_bitcoind_config(bitcoind_cfg, false)?;
    let (secondary_bitcoind_client, _) =
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

    Ok(fallback_client)
}
