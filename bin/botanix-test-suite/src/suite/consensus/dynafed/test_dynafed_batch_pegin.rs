use std::time::Duration;

use bitcoin::{hashes::Hash, Amount};
use bitcoincore_rpc::RpcApi;
use botanix_btc_server_client;
use botanix_chainspec::constants::BOTANIX_TESTNET;
use ethers::{
    prelude::Provider,
    providers::{Http, Middleware},
};

use crate::{
    it_info_print,
    suite::consensus::{
        common::pegin::{run_batch_pegin, BatchPeginConfig},
        ConsensusIntegrationTestSuite,
    },
};

/// Test batch pegin in a dynafed context.
///
/// Sends multiple pegins (each with a unique ETH destination and gateway address)
/// in a single batch, then verifies balances and UTXO tracking on the btc-server.
#[allow(clippy::too_many_lines)]
pub async fn dynafed_batch_pegin(
    suite: &ConsensusIntegrationTestSuite,
) -> anyhow::Result<()> {
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
        .unwrap()
        .botanix_eth_client
        .clone()
        .expect("Botanix Client must be initialized");

    let batch_size = 10;

    let config = BatchPeginConfig {
        count: batch_size,
        amount_btc: Some(Amount::from_sat(100_000)),
    };

    it_info_print!("Running batch pegin", batch_size);

    let results = run_batch_pegin(
        &bitcoind_rpc,
        provider.clone(),
        &mint_client,
        &mut rx,
        pegin_conf_depth,
        config,
    )
    .await?;

    anyhow::ensure!(
        results.len() == batch_size,
        "Expected {} results, got {}",
        batch_size,
        results.len()
    );

    // Verify each pegin destination received a non-zero balance
    for (idx, result) in results.iter().enumerate() {
        let balance = provider
            .get_balance(
                ethers::types::NameOrAddress::Address(result.eth_destination),
                None,
            )
            .await?;

        anyhow::ensure!(
            !balance.is_zero(),
            "Pegin balance is zero for pegin {} (address {:?})",
            idx,
            result.eth_destination
        );
    }

    // Verify UTXOs are tracked by btc-server
    let utxos = suite
        .local_context
        .btc_server_clients
        .clone()
        .expect("btc server clients")[0]
        .get_all_utxos(botanix_btc_server_client::Empty {})
        .await?
        .into_inner()
        .utxos;

    for (idx, result) in results.iter().enumerate() {
        let pegin_txid = result.pegin_tx.compute_txid();
        let utxo_found = utxos.iter().any(|utxo| {
            bitcoin::Txid::from_slice(
                utxo.outpoint.as_ref().unwrap().txid.as_slice(),
            )
            .map(|txid| txid == pegin_txid)
            .unwrap_or(false)
        });

        anyhow::ensure!(
            utxo_found,
            "UTXO not found for pegin {} with txid {}",
            idx,
            pegin_txid
        );
    }

    it_info_print!("All balances and UTXOs verified", batch_size);

    Ok(())
}
