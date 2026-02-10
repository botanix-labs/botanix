use std::time::Duration;

use bitcoin::Amount;
use botanix_authority_peg::{mint_validation::BURN_TOPIC, peg_contract::PegoutData, utils::AmountExt};
use botanix_chainspec::constants::BOTANIX_TESTNET;
use ethers::providers::{Http, Provider};

use crate::{
    it_info_print,
    suite::consensus::{
        common::{
            events::await_botanix_event,
            pegin::{run_pegin, PeginResult},
        },
        ConsensusIntegrationTestSuite,
    },
    utils::generate_blocks,
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
    // TODO We could do a precise amounts check here
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

    Ok(())
}
