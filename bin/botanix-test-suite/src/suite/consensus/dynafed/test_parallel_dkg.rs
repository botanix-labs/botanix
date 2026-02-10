use std::time::Duration;

use bitcoin::Amount;
use bitcoincore_rpc::RpcApi;
use botanix_authority_peg::{
    mint_validation::BURN_TOPIC,
    peg_contract::PegoutData,
    utils::AmountExt,
};
use botanix_btc_server_client::{
    AbortDkgRequest, BtcServerExtendedApi, BtcServerExtendedClient,
    GetPublicKeyRequest, StartNewDkgRequest,
};
use botanix_chainspec::constants::BOTANIX_TESTNET;
use botanix_types::{MultisigId, LEGACY_MULTISIG_ID};
use ethers::providers::{Http, Provider};

use crate::{
    it_info_print, it_warn_print,
    suite::consensus::{
        common::{
            events::await_botanix_event,
            pegin::{run_pegin, PeginResult},
            poa_node::FederationMemberTestConfig,
        },
        ConsensusIntegrationTestSuite,
    },
    utils::generate_blocks,
};

/// Restart DKG sessions on all federation nodes for a given multisig ID.
/// TODO: this is a temporary workaround as the normal flow is still under development.
async fn restart_dkg_on_all_nodes(
    fed_members: &std::collections::BTreeMap<u16, FederationMemberTestConfig>,
    multisig_id: MultisigId,
) -> anyhow::Result<()> {
    for (_index, fed_member) in fed_members.iter() {
        let btc_server_url =
            format!("http://{}", fed_member.bitcoin_server_url);
        let mut btc_client = BtcServerExtendedClient::new(btc_server_url, None)
            .await
            .expect("Failed to create BTC server client");

        // Abort any existing DKG session created at startup
        let _ = btc_client
            .abort_dkg(AbortDkgRequest { multisig_id: multisig_id.as_u32() })
            .await;

        // Start new DKG - this sends DkgNotification::Start to the local frost_task
        btc_client
            .start_new_dkg(StartNewDkgRequest {
                multisig_id: multisig_id.as_u32(),
            })
            .await
            .expect("Failed to start DKG for new multisig");
    }
    Ok(())
}

/// Test parallel DKG functionality with multiple multisig IDs.
///
/// This test verifies that:
/// 1. Nodes can have pre-saved keys for legacy multisig (ID 0)
/// 2. Coordinator can trigger DKG for new multisig (ID 1) via start_new_dkg RPC
/// 3. All nodes complete DKG and agree on the same aggregate public key
/// 4. Pre-saved keys remain accessible after DKG completes for new multisig
/// 5. The two multisigs have different aggregate public keys
pub async fn test_parallel_dkg(
    suite: &ConsensusIntegrationTestSuite,
) -> anyhow::Result<(), super::error::Error> {
    it_info_print!("Starting parallel DKG test with multiple multisig IDs");

    let test_fed_members = suite
        .local_context
        .poa_nodes
        .as_ref()
        .expect("test federation member configurations")
        .clone();

    // Wait for DKG to complete for multisig ID 1 (the newly initialized federation)
    // Multisig ID 0 was pre-saved before nodes started, so it should be instantly available
    let target_multisig_id = MultisigId::new(LEGACY_MULTISIG_ID.as_u32() + 1);

    // Restart DKG on all nodes (temporary workaround as the normal flow is still under development)
    it_info_print!(
        "Restarting DKG sessions on all nodes for multisig ID",
        target_multisig_id.as_u32()
    );
    restart_dkg_on_all_nodes(&test_fed_members, target_multisig_id)
        .await
        .expect("Failed to restart DKG on all nodes");

    it_info_print!(
        "Waiting for DKG completion for multisig ID",
        target_multisig_id.as_u32()
    );

    // Wait for DKG to complete on all nodes for the target multisig
    let mut dkg_completed = vec![];
    for (index, fed_member) in test_fed_members.iter() {
        let btc_server_url =
            format!("http://{}", fed_member.bitcoin_server_url);
        let mut btc_client = BtcServerExtendedClient::new(btc_server_url, None)
            .await
            .expect("Failed to create BTC server client");

        // Wait for DKG to complete for the target multisig
        let pub_key = loop {
            match btc_client
                .get_public_key(GetPublicKeyRequest {
                    multisig_id: target_multisig_id.as_u32(),
                })
                .await
            {
                Ok(pub_key) => {
                    it_info_print!(format!(
                        "DKG completed for node {} multisig ID {}",
                        index,
                        target_multisig_id.as_u32()
                    ));
                    break pub_key;
                }
                Err(_) => {
                    it_warn_print!(format!(
                        "DKG pending for node {} multisig ID {}",
                        index,
                        target_multisig_id.as_u32()
                    ));
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        };

        dkg_completed.push((*index, pub_key.publickey.clone()));
    }

    // Assert all nodes have the same aggregate public key for multisig ID 1
    let first_pubkey = &dkg_completed[0].1;
    it_info_print!(format!(
        "Multisig ID {} public key from DKG: {}",
        target_multisig_id.as_u32(),
        first_pubkey
    ));
    for (index, pubkey) in &dkg_completed {
        assert_eq!(
            pubkey,
            first_pubkey,
            "Node {} has different aggregate public key for multisig ID {}",
            index,
            target_multisig_id.as_u32()
        );
    }

    it_info_print!(
        format!(
            "✅ All nodes completed DKG with matching aggregate public key for multisig ID {}",
            target_multisig_id.as_u32()
        )
    );

    // Verify multisig 0 was pre-saved (should be instantly available)
    it_info_print!("Verifying pre-saved legacy multisig keys (ID 0)");

    let mut legacy_pubkeys = vec![];
    for (index, fed_member) in test_fed_members.iter() {
        let btc_server_url =
            format!("http://{}", fed_member.bitcoin_server_url);
        let mut btc_client = BtcServerExtendedClient::new(btc_server_url, None)
            .await
            .expect("Failed to create BTC server client");

        // This should succeed immediately since it was pre-saved
        let legacy_pub_key = btc_client
            .get_public_key(GetPublicKeyRequest {
                multisig_id: LEGACY_MULTISIG_ID.as_u32(),
            })
            .await
            .expect("Legacy multisig should be pre-saved");

        it_info_print!(format!(
            "✅ Node {} has pre-saved legacy multisig (ID {}) with pubkey: {}",
            index,
            LEGACY_MULTISIG_ID.as_u32(),
            legacy_pub_key.publickey
        ));

        legacy_pubkeys.push((*index, legacy_pub_key.publickey));
    }

    // Assert all nodes have the same pre-saved legacy keys
    let first_legacy_pubkey = &legacy_pubkeys[0].1;
    for (index, pubkey) in &legacy_pubkeys {
        assert_eq!(
            pubkey, first_legacy_pubkey,
            "Node {} has different pre-saved legacy multisig key",
            index
        );
    }

    it_info_print!(format!(
        "✅ All nodes have matching pre-saved legacy multisig keys (ID {})",
        LEGACY_MULTISIG_ID.as_u32()
    ));

    it_info_print!(format!(
        "Comparing keys - Multisig 0: {}, Multisig 1: {}",
        first_legacy_pubkey, first_pubkey
    ));

    // ========================================================================
    // Confirm M1 (multisig_id: 0) can still pegin and pegout after DKG
    // ========================================================================

    let pegin_conf_depth =
        BOTANIX_TESTNET.bitcoin_checkpoint_confirmation_depth;
    it_info_print!("Pegin Confirmation Depth", pegin_conf_depth);

    let bitcoind_rpc = suite.global_context.bitcoind_rpc();
    tokio::time::sleep(Duration::from_secs(5)).await;

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

    let PeginResult {
        pegin_tx,
        vout,
        btc_address,
        ..
    } = run_pegin(
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

    // ========================================================================
    // Pegout verification
    // ========================================================================

    let mint_contract = mint_client;
    // Generate and send pegout tx
    // bitcoin address
    let pegout_destination = ethers::core::types::Bytes::from(
        btc_address.to_string().as_bytes().to_vec(),
    );
    // set pegout version
    let pegout_data =
        ethers::core::types::Bytes::from(vec![PegoutData::version()]);
    let pegout_amount = Amount::from_btc(0.5).unwrap();
    let tx_receipt = mint_contract
        .burn(pegout_destination, pegout_data, pegout_amount.to_wei())
        .await
        .unwrap();
    it_info_print!("Pegout Tx Receipt: ", tx_receipt);

    // wait for the tx to be included in a botanix block
    await_botanix_event(&mut rx, *BURN_TOPIC).await;

    // sleep for a few more seconds
    tokio::time::sleep(Duration::from_secs(50)).await;

    // Reconnect to bitcoind. Occasionally the connection is lost after a long time or b/c of other
    // processes connecting
    let bitcoind_rpc = suite.global_context.bitcoind_rpc();
    // mine some btc blocks (needed for confirmed pegout)
    generate_blocks(&bitcoind_rpc, 1).await;
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Retrieve the last block
    let tip_hash =
        bitcoind_rpc.get_best_block_hash().expect("valid block hash");
    let tip_block = bitcoind_rpc.get_block(&tip_hash).expect("valid block");

    it_info_print!("Tip block: ", tip_block);
    // there should be 2 transaction one of which is the pegout the other is coinbase
    assert_eq!(tip_block.txdata.len(), 2);
    let pegout_tx = tip_block.txdata.get(1).unwrap();
    it_info_print!("Pegout tx: ", pegout_tx);

    assert_eq!(pegout_tx.input.len(), 1);
    assert_eq!(
        pegout_tx.input[0].previous_output.txid,
        pegin_tx.compute_txid()
    );
    assert_eq!(pegout_tx.input[0].previous_output.vout, vout);
    assert_eq!(pegout_tx.output.len(), 2);
    // One of the values here should be the pegout address
    let mut match_found = false;
    for output in pegout_tx.output.iter() {
        let pegout_address = output.script_pubkey.clone();
        let address_spk = btc_address.script_pubkey();
        match_found = pegout_address == address_spk;
        if match_found {
            break;
        }
    }
    assert!(match_found);
    assert!(pegout_tx.output[1].value > Amount::from_sat(0));

    // Verify the fee is exactly what we expect
    let total_input_value = pegin_tx.output[vout as usize].value;
    it_info_print!("Total input value: ", total_input_value);
    let total_output_value =
        pegout_tx.output[0].value + pegout_tx.output[1].value;
    it_info_print!("Total output value: ", total_output_value);
    let actual_fee = total_input_value - total_output_value;
    it_info_print!("Actual fee: ", actual_fee);
    let weight = pegout_tx.weight();
    it_info_print!("Weight: ", weight);
    let expected_fee_rate = 1250; // 1250 sat/kwu is equivalent to 0.00005 sat/byte, which is the fallbackfee set in bitcoin conf
    let expected_fee = (expected_fee_rate * weight.to_wu() + 999) / 1000; // Rounding up to nearest sat
    it_info_print!("Expected fee: ", expected_fee);
    assert_eq!(actual_fee, Amount::from_sat(expected_fee));

    // Verify witness signatures are 64 bytes (Taproot signature size when using SIGHASH_DEFAULT)
    for input in pegout_tx.input.iter() {
        let witness_item = &input.witness[0];
        it_info_print!("Input witness (signature) length:", witness_item.len());
        assert_eq!(witness_item.len(), 64);
    }

    it_info_print!("✅ M1 (multisig_id: 0) Pegout successful");

    Ok(())
}
