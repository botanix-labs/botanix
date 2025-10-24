
use std::{net::SocketAddr, sync::Arc};
use botanix_chainspec::BotanixChainSpec;
use btcserverlib::version::{CARGO_PKG_VERSION, VERGEN_BUILD_TIMESTAMP, VERGEN_GIT_SHA};

use reth_node_metrics::{chain::ChainSpecInfo, hooks::Hooks, server::{MetricServer, MetricServerConfig}, version::VersionInfo};

/// Starts an optional metrics server if `metrics_args` is Some(SocketAddr).
///
/// - `metrics_args`: Optional socket address to bind the metrics server to.
/// - `task_executor`: Task executor used by the metrics server.
/// - `chain_spec`: Shared chain specification with chain metadata.
///
/// Returns Ok(()) when the server is started or when metrics are disabled.
pub async fn run_metrics_service(metrics_args: Option<SocketAddr>, task_executor: &reth::tasks::TaskExecutor, chain_spec: Arc<BotanixChainSpec>) -> eyre::Result<()> {
    // add metrics if necessary
    if let Some(metrics_listener_address) = metrics_args {
        // start the metrics server
        let hooks = Hooks::builder().build();
        tracing::info!(target: "reth::cli", "Starting metrics endpoint at {}", metrics_listener_address.to_string());
        let config = MetricServerConfig::new(
            metrics_listener_address,
            VersionInfo {
                version: CARGO_PKG_VERSION,
                build_timestamp: VERGEN_BUILD_TIMESTAMP,
                cargo_features: "VERGEN_CARGO_FEATURES",
                git_sha: VERGEN_GIT_SHA,
                target_triple: "VERGEN_CARGO_TARGET_TRIPLE",
                build_profile: "BUILD_PROFILE_NAME",
            },
            ChainSpecInfo {
                name: chain_spec.chain().id().to_string(),
            },
            task_executor.clone(),
            hooks,
        );
        MetricServer::new(config).serve().await?;
    }
    Ok(())
}