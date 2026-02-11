//! Shared pegin flow helper for consensus tests.
//!
//! Runs the full pegin: get gateway address → send BTC → generate blocks →
//! build headers/PMT/PeginMeta → validate → mint → await MINT_TOPIC → balance check.

use std::{str::FromStr, time::Duration};

use alloy_primitives::Address as AlloyAddress;
use bitcoin::{hashes::Hash, merkle_tree::PartialMerkleTree, Amount};
use bitcoincore_rpc::RpcApi;
use botanix_authority_peg::{
    mint_validation::MINT_TOPIC,
    peg_contract::{PeginData, PeginMeta, PeginMetaV0},
    utils::AmountExt,
};
use ethers::{
    prelude::Provider,
    providers::{Http, Middleware},
    types::{Address as EtherAddress, NameOrAddress, U256},
};

use crate::{
    it_info_print,
    suite::consensus::common::{
        botanix_client::BotanixEthClient,
        events::{await_botanix_event, GatewayAddressResponse},
        poa_node::Notifications,
    },
    utils::{generate_blocks, get_gateway_address_with_retry},
};

/// Result of a single pegin, for use in pegout or assertions.
#[derive(Clone, Debug)]
pub struct PeginResult {
    pub pegin_tx: bitcoin::Transaction,
    pub vout: u32,
    pub amount: U256,
    pub eth_destination: EtherAddress,
    pub btc_address: bitcoin::Address,
    pub bitcoin_block_height: u32,
    pub aggregate_public_key: String,
    pub gateway_address_response: GatewayAddressResponse,
}

/// Runs the full pegin flow: gateway address → send BTC → blocks → headers/PMT/PeginMeta →
/// validate → mint → await MINT_TOPIC → assert balance.
///
/// Uses random `eth_destination` and `Amount::ONE_BTC` when not provided.
pub async fn run_pegin(
    bitcoind_rpc: &bitcoincore_rpc::Client,
    provider: Provider<Http>,
    mint_client: &BotanixEthClient,
    rx: &mut tokio::sync::broadcast::Receiver<Notifications>,
    pegin_conf_depth: u32,
    eth_destination: Option<EtherAddress>,
    amount_btc: Option<Amount>,
) -> anyhow::Result<PeginResult> {
    let eth_destination =
        eth_destination.unwrap_or_else(ethers::core::types::Address::random);
    let amount_btc = amount_btc.unwrap_or(Amount::ONE_BTC);

    let gateway_address_response = get_gateway_address_with_retry(
        provider.clone(),
        AlloyAddress::from_slice(eth_destination.as_bytes()),
        3,
    )
    .await
    .map_err(|e| anyhow::anyhow!("gateway address not available: {:?}", e))?;
    it_info_print!("Gateway Address Response", gateway_address_response);

    let balance = bitcoind_rpc
        .get_balance(None, None)
        .map_err(|e| anyhow::anyhow!("bitcoind get_balance failed: {}", e))?;
    it_info_print!("Bitcoin balance", balance);

    let btc_address = bitcoin::Address::from_str(
        gateway_address_response.gateway_address.as_str(),
    )
    .map_err(|e| anyhow::anyhow!("invalid gateway btc address: {}", e))?
    .assume_checked();

    let pegin_txid = bitcoind_rpc
        .send_to_address(
            &btc_address,
            amount_btc,
            None,
            None,
            Some(true),
            None,
            Some(1),
            None,
        )
        .map_err(|e| anyhow::anyhow!("send_to_address failed: {}", e))?;

    generate_blocks(bitcoind_rpc, 1 + pegin_conf_depth).await;
    tokio::time::sleep(Duration::from_secs(5)).await;

    let tx_res = bitcoind_rpc
        .get_transaction(&pegin_txid, None)
        .map_err(|e| anyhow::anyhow!("get_transaction failed: {}", e))?;
    anyhow::ensure!(tx_res.info.confirmations > 1, "pegin tx not confirmed");
    let pegin_tx = tx_res
        .transaction()
        .map_err(|e| anyhow::anyhow!("get pegin tx failed: {}", e))?;
    it_info_print!("Bitcoin pegin Tx", pegin_tx);
    it_info_print!("Gateway Data", gateway_address_response);

    let eth_account = AlloyAddress::from_slice(eth_destination.as_bytes());
    let (vout_index, pegin_output) = pegin_tx
        .output
        .iter()
        .enumerate()
        .find(|(_, o)| o.script_pubkey == btc_address.script_pubkey())
        .ok_or_else(|| anyhow::anyhow!("pegin output not found in tx"))?;
    let vout = vout_index as u32;
    let amount = pegin_output.value.to_wei();
    it_info_print!("Btc Amount", amount);

    let conf_hash = tx_res
        .info
        .blockhash
        .ok_or_else(|| anyhow::anyhow!("pegin tx has no blockhash"))?;
    let tip = bitcoind_rpc
        .get_best_block_hash()
        .map_err(|e| anyhow::anyhow!("get_best_block_hash failed: {}", e))?;
    it_info_print!("Bitcoin Chain Tip", tip);
    let tip_header = bitcoind_rpc
        .get_block_header(&tip)
        .map_err(|e| anyhow::anyhow!("get_block_header failed: {}", e))?;

    let mut headers = vec![];
    let mut cursor = tip_header;
    let mut stopgap = 200;
    loop {
        stopgap -= 1;
        anyhow::ensure!(
            stopgap > 0
                && cursor.prev_blockhash != bitcoin::BlockHash::all_zeros(),
            "confirmation block not found"
        );
        headers.push(cursor);
        if cursor.block_hash() == conf_hash {
            break;
        }
        cursor = bitcoind_rpc
            .get_block_header(&cursor.prev_blockhash)
            .map_err(|e| anyhow::anyhow!("get_block_header failed: {}", e))?;
    }
    headers.reverse();
    it_info_print!("Number of pegin_headers:", headers.len());

    let conf_block_info = bitcoind_rpc
        .get_block_info(&conf_hash)
        .map_err(|e| anyhow::anyhow!("get_block_info failed: {}", e))?;
    it_info_print!("Block info", conf_block_info);
    let pegin_txid_computed = pegin_tx.compute_txid();
    let merkle_match: Vec<bool> = conf_block_info
        .tx
        .iter()
        .map(|id| *id == pegin_txid_computed)
        .collect();
    let pmt = PartialMerkleTree::from_txids(&conf_block_info.tx, &merkle_match);

    let bitcoin_block_height = conf_block_info.height as u32;
    let aggregate_pubkey = secp256k1::PublicKey::from_str(
        gateway_address_response.aggregate_public_key.as_str(),
    )
    .map_err(|e| anyhow::anyhow!("invalid aggregate public key: {}", e))?;

    let meta = PeginMeta::V0(PeginMetaV0 {
        version: 0,
        outpoint: bitcoin::OutPoint::new(pegin_txid_computed, vout),
        address: eth_account,
        aggregate_publickey: aggregate_pubkey,
        tx: pegin_tx.clone(),
        merkle_proof: pmt,
        block_headers: headers,
    });

    let pegin_data = PeginData {
        account: AlloyAddress::from_slice(eth_destination.as_bytes()),
        amount,
        bitcoin_block_height,
        meta: vec![meta.clone()],
    };
    let tip_height = bitcoind_rpc
        .get_block_count()
        .map_err(|e| anyhow::anyhow!("get_block_count failed: {}", e))?;
    let checkpoint_height = tip_height - pegin_conf_depth as u64;
    let checkpoint_hash = bitcoind_rpc
        .get_block_hash(checkpoint_height)
        .map_err(|e| anyhow::anyhow!("get_block_hash failed: {}", e))?;
    let checkpoint_header =
        bitcoind_rpc.get_block_header(&checkpoint_hash).map_err(|e| {
            anyhow::anyhow!("get_block_header for checkpoint failed: {}", e)
        })?;
    let checkpoint = (checkpoint_header, checkpoint_height as u32);
    pegin_data.validate(&checkpoint, &aggregate_pubkey).map_err(|e| {
        anyhow::anyhow!("pegin data validation failed: {:?}", e)
    })?;
    it_info_print!("Pegindata successfully validated");

    it_info_print!(
        "Sending pegin tx: block headers=",
        meta.block_headers().iter().map(|h| h.block_hash()).collect::<Vec<_>>()
    );
    let serialized_pegin_meta = meta
        .serialize()
        .map_err(|e| anyhow::anyhow!("pegin meta serialize failed: {:?}", e))?;
    it_info_print!(
        "Serialized pegin meta: ",
        hex::encode(serialized_pegin_meta.clone())
    );

    let metadata = ethers::core::types::Bytes::from(serialized_pegin_meta);
    mint_client
        .mint(
            eth_destination.clone(),
            amount,
            bitcoin_block_height,
            metadata,
            ethers::core::types::Address::random(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("mint tx failed: {:?}", e))?;

    it_info_print!("Waiting for botanix event after mint call");
    await_botanix_event(rx, *MINT_TOPIC).await;
    tokio::time::sleep(Duration::from_secs(5)).await;

    let eth_address = NameOrAddress::from_str(&eth_account.to_string())
        .map_err(|e| anyhow::anyhow!("eth_address from_str failed: {}", e))?;
    let eth_address_balance = provider
        .get_balance(eth_address, None)
        .await
        .map_err(|e| anyhow::anyhow!("get_balance failed: {}", e))?;
    anyhow::ensure!(
        !eth_address_balance.is_zero(),
        "pegin balance is zero after mint"
    );
    it_info_print!("✅ Pegin successful - ETH balance received");

    Ok(PeginResult {
        pegin_tx,
        vout,
        amount,
        eth_destination,
        btc_address,
        bitcoin_block_height,
        aggregate_public_key: gateway_address_response
            .aggregate_public_key
            .clone(),
        gateway_address_response,
    })
}
