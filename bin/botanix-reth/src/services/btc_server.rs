use std::time::Duration;
use botanix_btc_server_client::{BtcServerExtendedApi, BtcServerExtendedClient, Empty, GrpcClientFactory};
use botanix_cli_args::{bitcoind::BitcoindArgs, poa_node::PoaNodeArgs};
use btcserverlib::util::retry_exec;
use eyre::Ok;

/// Creates and connects a BTC server client if federation mode is enabled, otherwise returns None.
pub async fn create_btc_server_client(poa_cfg: &PoaNodeArgs, bitcoind_cfg: &BitcoindArgs) -> eyre::Result<Option<(GrpcClientFactory, BtcServerExtendedClient)>> {
    match poa_cfg.federation_mode {
        true => {
            let btc_server_factory = GrpcClientFactory::new(
                bitcoind_cfg.btc_server.clone().expect("btc_server exists"),
                bitcoind_cfg.btc_signing_server_jwt_secret().ok().flatten(),
            );

            let fut = || async { btc_server_factory.build_and_connect().await };

            let mut btc_server_client =
            match retry_exec("btc_server_start", fut, 3, Duration::from_secs(2)).await {
                std::result::Result::Ok(client) => client,
                Err(err) => {
                    tracing::error!(target: "reth::cli", "Failed to connect to btc server: {}", err);
                    return Err(eyre::eyre!("Failed to connect to btc server: {}", err));
                }
            };
            tracing::info!(target: "reth::cli", "Btc server connected");

            // Check our connection to the btc server is authenticated properly
            btc_server_client.health_check(Empty {}).await.map_err(|err| {
                tracing::error!(target: "reth::cli", "Failed to authenticate to btc server: {}", err);
                eyre::eyre!("Failed to authenticate to btc server: {}", err)
            })?;
            tracing::info!(target: "reth::cli", "Btc server authenticated");

            return Ok(Some((btc_server_factory, btc_server_client)));
        },
        false => {
            return Ok(None);
        }
    }
}