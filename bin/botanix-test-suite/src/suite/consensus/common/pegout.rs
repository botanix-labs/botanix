//! Shared pegout flow helper for consensus tests (parallel DKG flow).
//!
//! Runs: burn → await BURN_TOPIC → wait → mine 1 block → get tip → assert pegout tx structure, fee, witness.

use std::time::Duration;

use bitcoin::Amount;
use bitcoincore_rpc::RpcApi;
use botanix_authority_peg::{
    mint_validation::BURN_TOPIC, peg_contract::PegoutData, utils::AmountExt,
};

use crate::{
    it_info_print,
    suite::consensus::common::{
        botanix_client::BotanixEthClient, events::await_botanix_event,
        pegin::PeginResult, poa_node::Notifications,
    },
    utils::generate_blocks,
};

/// Runs the full pegout flow from the parallel DKG test: burn → BURN_TOPIC → 50s sleep →
/// mine 1 block → get tip → assert pegout tx (input, outputs, fee, witness).
///
/// Uses `pegout_amount_btc` or default 0.5 BTC. Caller may pass a fresh `bitcoind_rpc`
/// after the wait (e.g. reconnect) if desired.
pub async fn run_pegout(
    mint_client: &BotanixEthClient,
    rx: &mut tokio::sync::broadcast::Receiver<Notifications>,
    pegin_result: &PeginResult,
    bitcoind_rpc: &bitcoincore_rpc::Client,
    pegout_amount_btc: Option<Amount>,
) -> anyhow::Result<()> {
    let pegout_amount = pegout_amount_btc
        .unwrap_or_else(|| Amount::from_btc(0.5).expect("0.5 btc"));
    let pegout_destination = ethers::core::types::Bytes::from(
        pegin_result.btc_address.to_string().as_bytes().to_vec(),
    );
    let pegout_data =
        ethers::core::types::Bytes::from(vec![PegoutData::version()]);

    let tx_receipt = mint_client
        .burn(pegout_destination, pegout_data, pegout_amount.to_wei())
        .await
        .map_err(|e| anyhow::anyhow!("burn failed: {:?}", e))?;
    it_info_print!("Pegout Tx Receipt: ", tx_receipt);

    await_botanix_event(rx, *BURN_TOPIC).await;

    tokio::time::sleep(Duration::from_secs(50)).await;

    generate_blocks(bitcoind_rpc, 1).await;
    tokio::time::sleep(Duration::from_secs(5)).await;

    let tip_hash = bitcoind_rpc
        .get_best_block_hash()
        .map_err(|e| anyhow::anyhow!("get_best_block_hash failed: {}", e))?;
    let tip_block = bitcoind_rpc
        .get_block(&tip_hash)
        .map_err(|e| anyhow::anyhow!("get_block failed: {}", e))?;

    it_info_print!("Tip block: ", tip_block);
    anyhow::ensure!(
        tip_block.txdata.len() == 2,
        "tip block should have 2 txs (coinbase + pegout), got {}",
        tip_block.txdata.len()
    );
    let pegout_tx = tip_block.txdata.get(1).expect("pegout at index 1");
    it_info_print!("Pegout tx: ", pegout_tx);

    anyhow::ensure!(
        pegout_tx.input.len() == 1,
        "pegout tx should have 1 input, got {}",
        pegout_tx.input.len()
    );
    anyhow::ensure!(
        pegout_tx.input[0].previous_output.txid
            == pegin_result.pegin_tx.compute_txid(),
        "pegout input should spend pegin tx"
    );
    anyhow::ensure!(
        pegout_tx.input[0].previous_output.vout == pegin_result.vout,
        "pegout input vout mismatch"
    );
    anyhow::ensure!(
        pegout_tx.output.len() == 2,
        "pegout tx should have 2 outputs, got {}",
        pegout_tx.output.len()
    );

    let address_spk = pegin_result.btc_address.script_pubkey();
    let match_found =
        pegout_tx.output.iter().any(|o| o.script_pubkey == address_spk);
    anyhow::ensure!(match_found, "pegout outputs should include btc_address");
    anyhow::ensure!(
        pegout_tx.output[1].value > Amount::from_sat(0),
        "pegout output[1] value should be positive"
    );

    let total_input_value =
        pegin_result.pegin_tx.output[pegin_result.vout as usize].value;
    it_info_print!("Total input value: ", total_input_value);
    let total_output_value =
        pegout_tx.output[0].value + pegout_tx.output[1].value;
    it_info_print!("Total output value: ", total_output_value);
    let actual_fee = total_input_value - total_output_value;
    it_info_print!("Actual fee: ", actual_fee);
    let weight = pegout_tx.weight();
    it_info_print!("Weight: ", weight);
    let expected_fee_rate = 1250;
    let expected_fee = (expected_fee_rate * weight.to_wu() + 999) / 1000;
    it_info_print!("Expected fee: ", expected_fee);
    anyhow::ensure!(
        actual_fee == Amount::from_sat(expected_fee),
        "pegout fee mismatch: actual {} expected {}",
        actual_fee,
        Amount::from_sat(expected_fee)
    );

    for input in pegout_tx.input.iter() {
        let witness_item = &input.witness[0];
        it_info_print!("Input witness (signature) length:", witness_item.len());
        anyhow::ensure!(
            witness_item.len() == 64,
            "witness signature should be 64 bytes (Taproot), got {}",
            witness_item.len()
        );
    }

    it_info_print!("✅ M1 (multisig_id: 0) Pegout successful");

    Ok(())
}
