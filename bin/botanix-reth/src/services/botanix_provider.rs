
use botanix_btc_wallet::bitcoind::BitcoindClientFactory;
use botanix_cli_args::bitcoind::BitcoindArgs;
use botanix_rpc_config::botanix_config::{Botanix, BotanixConfig};
use eyre::Ok;

pub fn create_botanix_provider(bitcoind_cfg: &BitcoindArgs, bitcoind_factory: &BitcoindClientFactory) -> eyre::Result<Botanix> {
    let botanix_config = BotanixConfig::new(bitcoind_cfg.btc_network, bitcoind_factory.clone());
    let botanix_provider = Botanix::new(botanix_config);
    Ok(botanix_provider)
}