use botanix_cli_args::poa_node::PoaNodeArgs;
use botanix_comet_bft_rpc::HttpCometBFTRpcClientFactory;

/// Create an HttpCometBFTRpcClientFactory configured from the provided PoaNodeArgs.
///
/// # Arguments
/// * `poa_cfg` - Configuration containing the CometBFT RPC host and port.
pub fn create_cometbft_factory(
    poa_cfg: &PoaNodeArgs,
) -> HttpCometBFTRpcClientFactory {
    let cometbft_rpc_factory = HttpCometBFTRpcClientFactory::default()
        .with_url(&format!(
            "http://{}:{}",
            poa_cfg.cometbft_rpc_host, poa_cfg.cometbft_rpc_port
        ));
    cometbft_rpc_factory
}
