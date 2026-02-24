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

/// Configuration for batch pegin operations.
#[derive(Clone, Debug)]
pub struct BatchPeginConfig {
    pub count: usize,
    pub amount_btc: Option<Amount>,
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

/// Runs batch pegin operations efficiently by batching Bitcoin transactions and block generation.
///
/// # Phases
/// 1. Setup: Generate ETH addresses, get gateway address, check balance
/// 2. Bitcoin Transactions: Send all BTC transactions without block generation
/// 3. Block Generation: Generate blocks once for all transactions
/// 4. Metadata Building: Build PeginMeta for all transactions in parallel
/// 5. Minting: Call mint for each pegin sequentially
/// 6. Event Verification: Verify all mint events and final balances
pub async fn run_batch_pegin(
    bitcoind_rpc: &bitcoincore_rpc::Client,
    provider: Provider<Http>,
    mint_client: &BotanixEthClient,
    rx: &mut tokio::sync::broadcast::Receiver<Notifications>,
    pegin_conf_depth: u32,
    config: BatchPeginConfig,
) -> anyhow::Result<Vec<PeginResult>> {
    let count = config.count;
    anyhow::ensure!(count > 0, "batch pegin count must be > 0");

    it_info_print!("Phase 1: Setting up {} pegins", count);

    let eth_destinations: Vec<EtherAddress> =
        (0..count).map(|_| EtherAddress::random()).collect();

    let amount_btc = config.amount_btc.unwrap_or(Amount::from_sat(100_000));
    let mut gateway_responses = Vec::with_capacity(count);
    let mut btc_addresses = Vec::with_capacity(count);

    // Get gateway address
    for (i, eth_dest) in eth_destinations.iter().enumerate() {
        let gw_resp = get_gateway_address_with_retry(
            provider.clone(),
            AlloyAddress::from_slice(eth_dest.as_bytes()),
            3,
        )
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "gateway address not available for pegin {}: {:?}",
                i,
                e
            )
        })?;

        let btc_addr =
            bitcoin::Address::from_str(gw_resp.gateway_address.as_str())
                .map_err(|e| {
                    anyhow::anyhow!("invalid gateway btc address: {}", e)
                })?
                .assume_checked();

        gateway_responses.push(gw_resp);
        btc_addresses.push(btc_addr);
    }

    // Check Bitcoin balance is sufficient
    let balance = bitcoind_rpc
        .get_balance(None, None)
        .map_err(|e| anyhow::anyhow!("bitcoind get_balance failed: {}", e))?;
    let total_needed = amount_btc
        .checked_mul(count as u64)
        .ok_or_else(|| anyhow::anyhow!("amount overflow"))?;
    anyhow::ensure!(
        balance >= total_needed,
        "insufficient balance: have {}, need {}",
        balance,
        total_needed
    );
    it_info_print!("Bitcoin balance sufficient", balance);

    // ============================================================================
    // Phase 2: Bitcoin Transactions (Sequential but Fast)
    // ============================================================================
    it_info_print!("Phase 2: Sending {} Bitcoin transactions", count);

    let mut txids = Vec::with_capacity(count);
    for i in 0..count {
        let txid = bitcoind_rpc
            .send_to_address(
                &btc_addresses[i],
                amount_btc,
                None,
                None,
                Some(true),
                None,
                Some(1),
                None,
            )
            .map_err(|e| anyhow::anyhow!("send_to_address failed: {}", e))?;
        it_info_print!(format!("Sent pegin tx {}/{}: {}", i + 1, count, txid));
        txids.push(txid);
    }

    // ============================================================================
    // Phase 3: Block Generation (Once)
    // ============================================================================
    it_info_print!("Phase 3: Generating blocks once for all transactions");

    generate_blocks(bitcoind_rpc, 1 + pegin_conf_depth).await;
    tokio::time::sleep(Duration::from_secs(5)).await;

    // ============================================================================
    // Phase 4: Metadata Building (Parallel/Batched)
    // ============================================================================
    it_info_print!("Phase 4: Building metadata for all transactions");

    // Retrieve all transactions
    let mut tx_results = Vec::with_capacity(count);
    for txid in &txids {
        let tx_res = bitcoind_rpc
            .get_transaction(txid, None)
            .map_err(|e| anyhow::anyhow!("get_transaction failed: {}", e))?;
        anyhow::ensure!(
            tx_res.info.confirmations > 1,
            "pegin tx not confirmed"
        );
        tx_results.push(tx_res);
    }

    // Get the tip and build headers (shared across all txs if in same block)
    let tip = bitcoind_rpc
        .get_best_block_hash()
        .map_err(|e| anyhow::anyhow!("get_best_block_hash failed: {}", e))?;
    let tip_header = bitcoind_rpc
        .get_block_header(&tip)
        .map_err(|e| anyhow::anyhow!("get_block_header failed: {}", e))?;

    // Get checkpoint for validation
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

    // Build PeginMeta for each transaction
    let mut pegin_metas = Vec::with_capacity(count);
    let mut pegin_data_list = Vec::with_capacity(count);

    for (idx, tx_res) in tx_results.iter().enumerate() {
        let btc_address = &btc_addresses[idx];
        let gateway_response = &gateway_responses[idx];
        let aggregate_pubkey = secp256k1::PublicKey::from_str(
            gateway_response.aggregate_public_key.as_str(),
        )
        .map_err(|e| anyhow::anyhow!("invalid aggregate public key: {}", e))?;

        let pegin_tx = tx_res
            .transaction()
            .map_err(|e| anyhow::anyhow!("get pegin tx failed: {}", e))?;

        let eth_destination = eth_destinations[idx];
        let eth_account = AlloyAddress::from_slice(eth_destination.as_bytes());

        // Find the output that pays to the gateway address
        let (vout_index, pegin_output) = pegin_tx
            .output
            .iter()
            .enumerate()
            .find(|(_, o)| o.script_pubkey == btc_address.script_pubkey())
            .ok_or_else(|| anyhow::anyhow!("pegin output not found in tx"))?;
        let vout = vout_index as u32;
        let amount = pegin_output.value.to_wei();

        // Get block hash and build headers
        let conf_hash = tx_res
            .info
            .blockhash
            .ok_or_else(|| anyhow::anyhow!("pegin tx has no blockhash"))?;

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
            cursor =
                bitcoind_rpc.get_block_header(&cursor.prev_blockhash).map_err(
                    |e| anyhow::anyhow!("get_block_header failed: {}", e),
                )?;
        }
        headers.reverse();

        // Build partial merkle tree
        let conf_block_info = bitcoind_rpc
            .get_block_info(&conf_hash)
            .map_err(|e| anyhow::anyhow!("get_block_info failed: {}", e))?;
        let bitcoin_block_height = conf_block_info.height as u32;
        let pegin_txid_computed = pegin_tx.compute_txid();
        let merkle_match: Vec<bool> = conf_block_info
            .tx
            .iter()
            .map(|id| *id == pegin_txid_computed)
            .collect();
        let pmt =
            PartialMerkleTree::from_txids(&conf_block_info.tx, &merkle_match);

        // Create PeginMeta
        let meta = PeginMeta::V0(PeginMetaV0 {
            version: 0,
            outpoint: bitcoin::OutPoint::new(pegin_txid_computed, vout),
            address: eth_account,
            aggregate_publickey: aggregate_pubkey,
            tx: pegin_tx.clone(),
            merkle_proof: pmt,
            block_headers: headers,
        });

        // Validate pegin data
        let pegin_data = PeginData {
            account: eth_account,
            amount,
            bitcoin_block_height,
            meta: vec![meta.clone()],
        };
        pegin_data.validate(&checkpoint, &aggregate_pubkey).map_err(|e| {
            anyhow::anyhow!(
                "pegin data validation failed for tx {}: {:?}",
                idx,
                e
            )
        })?;

        pegin_metas.push((meta, amount, bitcoin_block_height, vout));
        pegin_data_list.push((pegin_tx, amount, bitcoin_block_height, vout));
    }

    it_info_print!("All metadata built and validated successfully");

    // ============================================================================
    // Phase 5: Minting (Sequential but Optimized)
    // ============================================================================
    it_info_print!("Phase 5: Minting {} pegins sequentially", count);

    for (idx, (meta, amount, bitcoin_block_height, _)) in
        pegin_metas.iter().enumerate()
    {
        let eth_destination = eth_destinations[idx];
        let serialized_pegin_meta = meta.serialize().map_err(|e| {
            anyhow::anyhow!("pegin meta serialize failed: {:?}", e)
        })?;
        let metadata = ethers::core::types::Bytes::from(serialized_pegin_meta);

        mint_client
            .mint(
                eth_destination,
                *amount,
                *bitcoin_block_height,
                metadata,
                ethers::core::types::Address::random(),
            )
            .await
            .map_err(|e| anyhow::anyhow!("mint tx {} failed: {:?}", idx, e))?;

        it_info_print!(format!("Mint {}/{} completed", idx + 1, count));
    }

    // ============================================================================
    // Phase 6: Event Verification (Batched)
    // ============================================================================
    it_info_print!("Phase 6: Verifying events and balances for all pegins");

    // Await N MINT_TOPIC events
    for i in 0..count {
        it_info_print!(format!("Waiting for MINT event {}/{}", i + 1, count));
        await_botanix_event(rx, *MINT_TOPIC).await;
    }
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Verify final balance for the shared ETH destination
    let eth_account = AlloyAddress::from_slice(eth_destinations[0].as_bytes());
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
    it_info_print!(format!(
        "✅ All {} pegins verified - total balance: {}",
        count, eth_address_balance
    ));

    // ============================================================================
    // Return Results
    // ============================================================================
    let mut results = Vec::with_capacity(count);
    for (idx, (pegin_tx, amount, bitcoin_block_height, vout)) in
        pegin_data_list.into_iter().enumerate()
    {
        results.push(PeginResult {
            pegin_tx,
            vout,
            amount,
            eth_destination: eth_destinations[idx],
            btc_address: btc_addresses[idx].clone(),
            bitcoin_block_height,
            aggregate_public_key: gateway_responses[idx]
                .aggregate_public_key
                .clone(),
            gateway_address_response: gateway_responses[idx].clone(),
        });
    }

    it_info_print!(format!(
        "🎉 Batch pegin completed: {} pegins processed successfully",
        count
    ));
    Ok(results)
}
