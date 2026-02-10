use std::{collections::{HashMap, HashSet}, time::Duration};

use bitcoin::Amount;
use botanix_authority_peg::{mint_validation::BURN_TOPIC, peg_contract::PegoutData, utils::AmountExt};
use botanix_btc_server_client::{
    BtcServerClient, GetFinalizedPegoutIdsRequest,
};
use botanix_chainspec::constants::BOTANIX_TESTNET;
use ethers::providers::{Http, Provider};
use futures::StreamExt;
use tonic::transport::Channel;

use crate::{
    it_info_print,
    suite::consensus::{
        common::{
            events::{await_botanix_event, await_epoch_block},
            pegin::{run_pegin, PeginResult},
            poa_node::TestSignal,
        },
        ConsensusIntegrationTestSuite,
    },
    utils::generate_blocks,
};

pub async fn get_finalized_pegout_ids_from_peers(
    mut btc_servers: Vec<BtcServerClient<Channel>>,
) -> HashMap<usize, HashSet<Vec<u8>>> {
    let mut peers_finalized_pegout_ids: HashMap<usize, HashSet<Vec<u8>>> =
        HashMap::new();
    for (index, db_provider) in btc_servers.iter_mut().enumerate() {
        let mut pegout_ids_stream = db_provider
            .get_finalized_pegout_ids(GetFinalizedPegoutIdsRequest {
                chunk_size: 10,
            })
            .await
            .unwrap()
            .into_inner();
        while let Some(pegout_ids_chunk) = pegout_ids_stream.next().await {
            match pegout_ids_chunk {
                Ok(pegout_ids_chunk) => {
                    let _ =
                        peers_finalized_pegout_ids
                            .entry(index)
                            .or_insert_with(HashSet::new)
                            .extend(pegout_ids_chunk.data.into_iter().map(
                                |finalized_pegout_id| finalized_pegout_id.id,
                            ));
                }
                Err(_) => {
                    continue;
                }
            }
        }
    }
    peers_finalized_pegout_ids
}

// This test doesn't perform as needed so not including it in the test suite yet
// The signer intended to be dropped isn't actually dropped
// TODO: kill the signer process and restart it
#[allow(clippy::too_many_lines)]
pub async fn test_wallet_sync_dynamic(
    suite: &mut ConsensusIntegrationTestSuite,
) -> anyhow::Result<(), anyhow::Error> {
    // Non-happy path:
    // Non happy path where a signer drops and misses a finalized block:
    // Create a pegout, sign, and broadcast
    // Drop a signer so they miss the finalized block
    // Then generate deeply confirmed blocks to finalize the pegout
    // Bring signer back online
    // Wait for an epoch block and sync
    // Get finalized pegouts list from all peers again
    // Confirm the finalized pegouts list is the same as before

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

    let PeginResult { btc_address, .. } = run_pegin(
        &bitcoind_rpc,
        provider,
        &mint_client,
        &mut rx,
        pegin_conf_depth,
        None,
        None,
    )
    .await?;

    let mint_contract = mint_client;
    // Generate and send pegout tx
    // bitcoin address
    let pegout_destination = ethers::core::types::Bytes::from(
        btc_address.to_string().as_bytes().to_vec(),
    );
    // set pegout version
    let pegout_data =
        ethers::core::types::Bytes::from(vec![PegoutData::version()]);
    let pegout_amount = Amount::from_btc(0.4).unwrap();
    let tx_receipt = mint_contract
        .burn(
            pegout_destination.clone(),
            pegout_data.clone(),
            pegout_amount.to_wei(),
        )
        .await
        .unwrap();
    it_info_print!("Pegout Tx Receipt: ", tx_receipt);

    // wait for the tx to be included in a botanix block
    await_botanix_event(&mut rx, *BURN_TOPIC).await;

    // drop one of the signers so it misses the signing
    let dropped_signer = 1;
    let test_fed_members = suite.local_context.poa_nodes.as_ref().unwrap();
    // now disconnect the peers of fed member 1
    test_fed_members
        .get(&dropped_signer)
        .cloned()
        .unwrap()
        .send_test_signal(TestSignal::DisconnectAll());

    // Reconnect to bitcoind. Occasionally the connection is lost after a long time or b/c of other
    // processes connecting
    let bitcoind_rpc = suite.global_context.bitcoind_rpc();
    // mine some btc blocks (needed for confirmed pegout)
    generate_blocks(&bitcoind_rpc, 20).await;

    // now reconnect the peers of fed member 1
    test_fed_members
        .get(&dropped_signer)
        .cloned()
        .unwrap()
        .send_test_signal(TestSignal::ReconnectAll());

    // sleep for 20s
    tokio::time::sleep(Duration::from_secs(20)).await;

    // bring signer back up
    let test_fed_members = suite.local_context.poa_nodes.as_ref().unwrap();
    // now reconnect the peers of fed member 1
    test_fed_members
        .get(&1)
        .cloned()
        .unwrap()
        .send_test_signal(TestSignal::ReconnectAll());

    // wait for an epoch since this is when the pegout scheduler
    // determines if tracked txs are finalized
    await_epoch_block(&mut rx, BOTANIX_TESTNET.epoch_length).await;

    // get all finalized pegout ids before the poa epoch (before wallets sync)
    let peers_finalized_pegout_ids_before =
        get_finalized_pegout_ids_from_peers(
            suite.local_context.btc_server_clients.clone().unwrap(),
        )
        .await;

    // make sure we have all equal pegout ids before
    let first_peer_finalized_pegout_ids =
        peers_finalized_pegout_ids_before.get(&0).cloned().unwrap_or_default();
    for (_peer_id, peer_finalized_pegout_ids) in
        peers_finalized_pegout_ids_before
    {
        assert!(
            first_peer_finalized_pegout_ids.len()
                == peer_finalized_pegout_ids.len()
        );
        assert!(first_peer_finalized_pegout_ids == peer_finalized_pegout_ids);
    }

    await_epoch_block(&mut rx, BOTANIX_TESTNET.epoch_length).await;

    it_info_print!("Waiting for wallets to sync");
    loop {
        // get all finalized pegout ids after the poa epoch
        let peers_finalized_pegout_ids_after =
            get_finalized_pegout_ids_from_peers(
                suite.local_context.btc_server_clients.clone().unwrap(),
            )
            .await;

        let first_peer_finalized_pegout_ids = peers_finalized_pegout_ids_after
            .get(&0)
            .cloned()
            .unwrap_or_default();
        // wait until wallets synced and finalized pegouts list is not empty
        if first_peer_finalized_pegout_ids.is_empty() {
            it_info_print!("finalized pegout ids empty");

            // sleep for 10s
            tokio::time::sleep(Duration::from_secs(10)).await;
            continue;
        }
        it_info_print!(
            "First peer finalized pegout ids",
            first_peer_finalized_pegout_ids
        );
        // assert that all peers have the same list
        for (_peer_id, peer_finalized_pegout_ids) in
            peers_finalized_pegout_ids_after
        {
            assert!(
                first_peer_finalized_pegout_ids.len()
                    == peer_finalized_pegout_ids.len()
            );
            assert!(
                first_peer_finalized_pegout_ids == peer_finalized_pegout_ids
            );
        }

        break;
    }

    Ok(())
}
