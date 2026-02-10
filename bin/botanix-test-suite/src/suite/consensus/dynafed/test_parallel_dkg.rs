use std::{str::FromStr, time::Duration};

use alloy_primitives::Address;
use bitcoin::{hashes::Hash, merkle_tree::PartialMerkleTree, Amount};
use bitcoincore_rpc::RpcApi;
use botanix_authority_peg::{
    mint_validation::{BURN_TOPIC, MINT_TOPIC},
    peg_contract::{PeginData, PeginMeta, PeginMetaV0, PegoutData},
    utils::AmountExt,
};
use botanix_btc_server_client::{
    AbortDkgRequest, BtcServerExtendedApi, BtcServerExtendedClient,
    GetPublicKeyRequest, StartNewDkgRequest,
};
use botanix_chainspec::constants::BOTANIX_TESTNET;
use botanix_types::{MultisigId, LEGACY_MULTISIG_ID};
use ethers::{
    prelude::Provider,
    providers::{Http, Middleware},
    types::NameOrAddress,
};

use crate::{
    it_info_print, it_warn_print,
    suite::consensus::{
        common::events::await_botanix_event,
        common::poa_node::FederationMemberTestConfig,
        ConsensusIntegrationTestSuite,
    },
    utils::{generate_blocks, get_gateway_address_with_retry},
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

    // Set up regtest connection
    let bitcoind_rpc = suite.global_context.bitcoind_rpc();
    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut rx = suite
        .local_context
        .poa_notification
        .as_ref()
        .expect("poa notifs")
        .subscribe();

    // generate mint contract test instances
    let mut mint_contract_instances = Vec::new();
    for (index, _) in test_fed_members.iter() {
        let botanix_eth_client = test_fed_members
            .get(index)
            .cloned()
            .unwrap()
            .botanix_eth_client
            .clone();
        mint_contract_instances.push(botanix_eth_client);
    }

    // Set up dummy eth address
    let eth_destination = ethers::core::types::Address::random();

    // Provider to one of the federation members
    let provider = Provider::<Http>::try_from(format!(
        "http://localhost:{}",
        test_fed_members.get(&0).unwrap().rpc_port
    ))
    .expect("could not instantiate HTTP Provider");

    // get gateway address
    let gateway_address_response = get_gateway_address_with_retry(
        provider.clone(),
        eth_destination.0.into(),
        3,
    )
    .await
    .map_err(|_| super::error::Error::GatewayAddressNotAvailable)?;
    it_info_print!("Gateway Address Response", gateway_address_response);

    // print balance
    let balance = bitcoind_rpc.get_balance(None, None).expect("get balance");
    it_info_print!("Bitcoin balance", balance);

    // Send some bitcoin to that gateway address
    let btc_address = bitcoin::Address::from_str(
        gateway_address_response.gateway_address.as_str(),
    )
    .expect("valid btc_address")
    .assume_checked();
    let pegin_txid = bitcoind_rpc
        .send_to_address(
            &btc_address,
            Amount::ONE_BTC,
            None,
            None,
            Some(true),
            None,
            Some(1),
            None,
        )
        .expect("valid send");
    // Generate some block to confirm it
    generate_blocks(&bitcoind_rpc, 1 + pegin_conf_depth).await;
    tokio::time::sleep(Duration::from_secs(5)).await;

    // retrieve the transaction
    let tx_res =
        bitcoind_rpc.get_transaction(&pegin_txid, None).expect("valid tx");
    assert!(tx_res.info.confirmations > 1);
    let pegin_tx = tx_res.transaction().expect("valid tx");
    it_info_print!("Bitcoin pegin Tx", pegin_tx);
    it_info_print!("Gateway Data", gateway_address_response);
    it_info_print!(
        "Gateway Data Pub key",
        gateway_address_response.aggregate_public_key
    );

    let eth_account = Address::from_slice(eth_destination.as_bytes());
    let (vout, pegin_output) = pegin_tx
        .output
        .iter()
        .enumerate()
        .find(|(_, o)| o.script_pubkey == btc_address.script_pubkey())
        .unwrap();
    let amount = pegin_output.value.to_wei();
    it_info_print!("Btc Amount", amount);

    // get block headers
    // first we need the block hash of the block with the conf'd pegin tx
    let conf_hash = tx_res.info.blockhash.expect("pegin confirmed");
    let tip = bitcoind_rpc.get_best_block_hash().unwrap();
    it_info_print!("Bitcoin Chain Tip", tip);
    let tip_header =
        bitcoind_rpc.get_block_header(&tip).expect("valid block header");
    // We will collect all the headers all the way up to the tip which is not needed, but allowed.
    // In theory, we only need to collect headers from the block our pegin is in, to the finalized
    // block (the one in the mainchain commitment).
    let mut headers = vec![];
    let mut cursor = tip_header;
    let mut stopgap = 200; // just to make sure we don't infinite loop until genesis
    loop {
        stopgap -= 1;
        if stopgap == 0
            || cursor.prev_blockhash == bitcoin::BlockHash::all_zeros()
        {
            panic!("confirmation block not found...");
        }

        headers.push(cursor);
        if cursor.block_hash() == conf_hash {
            break;
        }
        cursor = bitcoind_rpc.get_block_header(&cursor.prev_blockhash).unwrap();
    }
    headers.reverse();
    it_info_print!("Number of pegin_headers:", headers.len());

    let conf_block_info =
        bitcoind_rpc.get_block_info(&conf_hash).expect("valid txids");
    it_info_print!("Block info", conf_block_info);
    let pmt =
        PartialMerkleTree::from_txids(&conf_block_info.tx, &[false, true]);

    // create pegin meta
    let bitcoin_block_height = conf_block_info.height;
    let meta = PeginMeta::V0(PeginMetaV0 {
        version: 0,
        outpoint: bitcoin::OutPoint::new(pegin_tx.compute_txid(), vout as u32),
        address: eth_account,
        aggregate_publickey: secp256k1::PublicKey::from_str(
            gateway_address_response.aggregate_public_key.as_str(),
        )
        .expect("valid public key"),
        tx: pegin_tx.clone(),
        merkle_proof: pmt,
        block_headers: headers,
    });

    // validate the pegin data first offchain before submitting
    let pegin_data = PeginData {
        account: Address::from_slice(eth_destination.as_bytes()),
        amount,
        bitcoin_block_height: bitcoin_block_height as u32,
        meta: vec![meta.clone()],
    };
    let checkpoint = {
        let tip = bitcoind_rpc.get_block_count().unwrap();
        let height = tip - pegin_conf_depth as u64;
        let hash = bitcoind_rpc.get_block_hash(height).unwrap();
        (bitcoind_rpc.get_block_header(&hash).unwrap(), height as u32)
    };
    pegin_data
        .validate(
            &checkpoint,
            &secp256k1::PublicKey::from_str(
                gateway_address_response.aggregate_public_key.as_str(),
            )
            .unwrap(),
        )
        .expect("pegin data should be valid!");
    it_info_print!("Pegindata successfully validated");

    // send the pegin transactions to all fed members
    it_info_print!(
        "Sending pegin tx: block headers=",
        meta.block_headers().iter().map(|h| h.block_hash()).collect::<Vec<_>>()
    );
    let serialized_pegin_meta = meta.serialize().unwrap();
    it_info_print!(
        "Serialized pegin meta: ",
        hex::encode(serialized_pegin_meta.clone())
    );
    let mint_contract = mint_contract_instances
        .first()
        .cloned()
        .unwrap()
        .expect("Botanix Client must be initialized");
    let metadata =
        ethers::core::types::Bytes::from(serialized_pegin_meta.clone());
    let tx_receipt = mint_contract
        .mint(
            eth_destination.clone(),
            amount,
            bitcoin_block_height as u32,
            metadata,
            ethers::core::types::Address::random(),
        )
        .await
        .unwrap();
    it_info_print!("Mint Tx Receipt ", tx_receipt);

    // wait for a few blocks to make sure the tx got included and mined
    it_info_print!("Waiting for botanix event after mint call");
    await_botanix_event(&mut rx, *MINT_TOPIC).await;
    tokio::time::sleep(Duration::from_secs(5)).await;

    // make sure we have received the botanix btc on botanix
    let eth_address =
        NameOrAddress::from_str(&eth_account.to_string()).unwrap();
    let eth_address_balance =
        provider.get_balance(eth_address, None).await.unwrap();
    assert!(!eth_address_balance.is_zero());
    it_info_print!("✅ Pegin successful - ETH balance received");

    // ========================================================================
    // Pegout verification
    // ========================================================================

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
    assert_eq!(pegout_tx.input[0].previous_output.vout, vout as u32);
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
    let total_input_value = pegin_tx.output[vout].value;
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
