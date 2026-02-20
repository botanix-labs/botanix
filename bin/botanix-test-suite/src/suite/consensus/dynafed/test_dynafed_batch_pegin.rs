use std::time::Duration;

use bitcoin::{hashes::Hash, Amount};
use bitcoincore_rpc::RpcApi;
use botanix_btc_server_client::{
    BtcServerExtendedApi, BtcServerExtendedClient, Empty, GetPublicKeyRequest,
};
use botanix_chainspec::constants::BOTANIX_TESTNET;
use botanix_types::{MultisigId, LEGACY_MULTISIG_ID};
use ethers::{
    prelude::Provider,
    providers::{Http, Middleware},
};

use crate::{
    it_info_print, it_warn_print,
    suite::consensus::{
        common::pegin::{run_batch_pegin, BatchPeginConfig},
        ConsensusIntegrationTestSuite,
    },
};

/// Test batch pegin in a dynafed context.
///
/// Uses the same dynafed setup as parallel_dkg: waits for DKG to complete for
/// the new multisig (ID 1), verifies legacy multisig (ID 0) is pre-saved, then
/// sends multiple pegins in a single batch and verifies balances and UTXO tracking.
#[allow(clippy::too_many_lines)]
pub async fn dynafed_batch_pegin(
    suite: &ConsensusIntegrationTestSuite,
) -> anyhow::Result<()> {
    it_info_print!("Starting dynafed batch pegin test");

    let test_fed_members = suite
        .local_context
        .poa_nodes
        .as_ref()
        .expect("test federation member configurations")
        .clone();

    // Dynafed setup: wait for DKG to complete for multisig ID 1 (new federation)
    let target_multisig_id = MultisigId::new(LEGACY_MULTISIG_ID.as_u32() + 1);
    it_info_print!(
        "Waiting for DKG completion for multisig ID",
        target_multisig_id.as_u32()
    );

    let mut dkg_completed = vec![];
    for (index, fed_member) in test_fed_members.iter() {
        let btc_server_url =
            format!("http://{}", fed_member.bitcoin_server_url);
        let mut btc_client = BtcServerExtendedClient::new(btc_server_url, None)
            .await
            .expect("Failed to create BTC server client");

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

    // All nodes must agree on the same aggregate public key for multisig ID 1
    let first_pubkey = &dkg_completed[0].1;
    for (index, pubkey) in &dkg_completed {
        anyhow::ensure!(
            pubkey == first_pubkey,
            "Node {} has different aggregate public key for multisig ID {}",
            index,
            target_multisig_id.as_u32()
        );
    }
    it_info_print!(format!(
        "All nodes completed DKG with matching aggregate public key for multisig ID {}",
        target_multisig_id.as_u32()
    ));

    // Verify legacy multisig (ID 0) is pre-saved
    it_info_print!("Verifying pre-saved legacy multisig keys (ID 0)");
    for (index, fed_member) in test_fed_members.iter() {
        let btc_server_url =
            format!("http://{}", fed_member.bitcoin_server_url);
        let mut btc_client = BtcServerExtendedClient::new(btc_server_url, None)
            .await
            .expect("Failed to create BTC server client");

        btc_client
            .get_public_key(GetPublicKeyRequest {
                multisig_id: LEGACY_MULTISIG_ID.as_u32(),
            })
            .await
            .expect("Legacy multisig should be pre-saved");

        it_info_print!(format!(
            "Node {} has pre-saved legacy multisig (ID {})",
            index,
            LEGACY_MULTISIG_ID.as_u32()
        ));
    }

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
        .get_all_utxos(Empty {})
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
