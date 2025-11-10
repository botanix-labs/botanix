use std::sync::Arc;

use botanix_btc_wallet::{
    bitcoind::BitcoindClientFactory, fallback::FallbackBitcoindClient,
};
use botanix_cli_args::bitcoind::BitcoindArgs;
use botanix_rpc_config::botanix_config::{Botanix, BotanixConfig};
use eyre::Ok;

/// Creates a Botanix provider from the provided bitcoind configuration and client factory.
///
/// # Arguments
///
/// * `bitcoind_cfg` - BitcoindArgs containing the BTC network settings.
/// * `bitcoind_factory` - BitcoindClientFactory used to create bitcoind clients.
///
/// # Errors
///
/// Returns an eyre::Result containing a Botanix instance on success.
pub fn create_botanix_provider(
    bitcoind_cfg: &BitcoindArgs,
    bitcoind_factory: Arc<FallbackBitcoindClient>,
) -> eyre::Result<Botanix> {
    let botanix_config =
        BotanixConfig::new(bitcoind_cfg.btc_network, bitcoind_factory);
    let botanix_provider = Botanix::new(botanix_config);
    Ok(botanix_provider)
}
