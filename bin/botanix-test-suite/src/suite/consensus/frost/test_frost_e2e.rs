use std::time::Duration;

use botanix_chainspec::constants::BOTANIX_TESTNET;
use ethers::providers::{Http, Provider};

use crate::{
    it_info_print,
    suite::consensus::{
        common::{
            pegin::{run_pegin, PeginResult},
            pegout::run_pegout,
        },
        ConsensusIntegrationTestSuite,
    },
};

#[allow(clippy::too_many_lines)]
pub async fn frost_e2e_stable(
    suite: &ConsensusIntegrationTestSuite,
) -> anyhow::Result<(), super::error::Error> {
    let pegin_conf_depth =
        BOTANIX_TESTNET.bitcoin_checkpoint_confirmation_depth;
    it_info_print!("Pegin Confirmation Depth", pegin_conf_depth);

    let bitcoind_rpc = suite.global_context.bitcoind_rpc();
    tokio::time::sleep(Duration::from_secs(5)).await;

    let test_fed_members = suite
        .local_context
        .poa_nodes
        .as_ref()
        .expect("test federation member configurations")
        .clone();
    let mut rx = suite
        .local_context
        .poa_notification
        .as_ref()
        .expect("poa notifs")
        .subscribe();

    let provider = Provider::<Http>::try_from(format!(
        "http://localhost:{}",
        test_fed_members.get(&0).unwrap().rpc_port
    ))
    .expect("could not instantiate HTTP Provider");

    let mint_client = test_fed_members
        .get(&0)
        .cloned()
        .unwrap()
        .botanix_eth_client
        .clone()
        .expect("Botanix Client must be initialized");

    let pegin_result = run_pegin(
        &bitcoind_rpc,
        provider,
        &mint_client,
        &mut rx,
        pegin_conf_depth,
        None,
        None,
    )
    .await
    .map_err(|e| super::error::Error::TestVectorExport(e.to_string()))?;

    let bitcoind_rpc = suite.global_context.bitcoind_rpc();
    run_pegout(&mint_client, &mut rx, &pegin_result, &bitcoind_rpc, None)
        .await
        .map_err(|e| super::error::Error::TestVectorExport(e.to_string()))?;

    Ok(())
}
