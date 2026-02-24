use std::time::Duration;

use botanix_btc_server_client::{
    BtcServerExtendedApi, BtcServerExtendedClient, GetPublicKeyRequest,
};
use botanix_chainspec::constants::BOTANIX_TESTNET;
use botanix_types::{MultisigId, LEGACY_MULTISIG_ID};
use ethers::providers::{Http, Provider};

use crate::{
    it_info_print, it_warn_print,
    suite::consensus::{
        common::{pegin::run_pegin, pegout::run_pegout},
        ConsensusIntegrationTestSuite,
    },
};

/// Test dynafed with a new member joining the federation.
///
/// M1 (multisig ID 0) has members [0, 1, 2, 3] (pre-saved).
/// M2 (multisig ID 1) has members [0, 1, 2, 3, 4] — M1's members plus node 4.
///
/// Due to BtcServer deriving FROST identifiers from member list positions,
/// M2 must be a superset of M1 (members at their original positions) so that
/// each node's `--identifier` matches its position in the member list.
///
/// This test verifies that:
/// 1. M2 members (all 5 nodes) complete DKG and agree on the aggregate public key
/// 2. Node 4 (not in M1) does NOT have a key for M1
/// 3. M1 members (nodes 0-3) have pre-saved keys for M1
/// 4. M1 and M2 have different aggregate public keys
/// 5. Pegin and pegout still work using M1
#[allow(clippy::too_many_lines)]
pub async fn test_dynafed_new_member(
    suite: &ConsensusIntegrationTestSuite,
) -> anyhow::Result<()> {
    it_info_print!("Starting dynafed new member test");

    let test_fed_members = suite
        .local_context
        .poa_nodes
        .as_ref()
        .expect("test federation member configurations")
        .clone();

    let m1_member_indices: Vec<u16> = vec![0, 1, 2, 3];
    let m2_member_indices: Vec<u16> = vec![0, 1, 2, 3, 4];
    let new_member: u16 = 4;

    let m1_multisig_id = LEGACY_MULTISIG_ID;
    let m2_multisig_id = MultisigId::new(LEGACY_MULTISIG_ID.as_u32() + 1);

    // ========================================================================
    // 1. Wait for DKG to complete on all M2 members (nodes 0, 1, 2, 3, 4)
    // ========================================================================

    it_info_print!(
        "Waiting for DKG completion for multisig ID",
        m2_multisig_id.as_u32()
    );

    let mut m2_dkg_completed = vec![];
    for &idx in &m2_member_indices {
        let fed_member = test_fed_members.get(&idx).unwrap_or_else(|| {
            panic!("Node {} not found in test federation", idx)
        });
        let btc_server_url =
            format!("http://{}", fed_member.bitcoin_server_url);
        let mut btc_client = BtcServerExtendedClient::new(btc_server_url, None)
            .await
            .expect("Failed to create BTC server client");

        let pub_key = loop {
            match btc_client
                .get_public_key(GetPublicKeyRequest {
                    multisig_id: m2_multisig_id.as_u32(),
                })
                .await
            {
                Ok(pub_key) => {
                    it_info_print!(format!(
                        "DKG completed for node {} multisig ID {}",
                        idx,
                        m2_multisig_id.as_u32()
                    ));
                    break pub_key;
                }
                Err(_) => {
                    it_warn_print!(format!(
                        "DKG pending for node {} multisig ID {}",
                        idx,
                        m2_multisig_id.as_u32()
                    ));
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        };

        m2_dkg_completed.push((idx, pub_key.publickey.clone()));
    }

    let first_m2_pubkey = &m2_dkg_completed[0].1;
    for (idx, pubkey) in &m2_dkg_completed {
        anyhow::ensure!(
            pubkey == first_m2_pubkey,
            "Node {} has different aggregate public key for multisig ID {}",
            idx,
            m2_multisig_id.as_u32()
        );
    }
    it_info_print!(format!(
        "✅ All M2 members completed DKG with matching aggregate public key for multisig ID {}",
        m2_multisig_id.as_u32()
    ));

    // ========================================================================
    // 2. Verify the new member (node 4) does NOT have a key for M1
    // ========================================================================

    {
        let fed_member =
            test_fed_members.get(&new_member).expect("Node 4 must exist");
        let btc_server_url =
            format!("http://{}", fed_member.bitcoin_server_url);
        let mut btc_client = BtcServerExtendedClient::new(btc_server_url, None)
            .await
            .expect("Failed to create BTC server client");

        let result = btc_client
            .get_public_key(GetPublicKeyRequest {
                multisig_id: m1_multisig_id.as_u32(),
            })
            .await;

        anyhow::ensure!(
            result.is_err(),
            "Node {} should NOT have a key for M1 (multisig ID {}), but it does",
            new_member,
            m1_multisig_id.as_u32()
        );
        it_info_print!(format!(
            "✅ Node {} (new member) correctly does not have a key for M1 (multisig ID {})",
            new_member,
            m1_multisig_id.as_u32()
        ));
    }

    // ========================================================================
    // 3. Verify M1 members (nodes 0, 1, 2, 3) have pre-saved keys for M1
    // ========================================================================

    it_info_print!("Verifying pre-saved legacy multisig keys (ID 0)");
    let mut m1_pubkeys = vec![];
    for &idx in &m1_member_indices {
        let fed_member = test_fed_members
            .get(&idx)
            .unwrap_or_else(|| panic!("Node {} not found", idx));
        let btc_server_url =
            format!("http://{}", fed_member.bitcoin_server_url);
        let mut btc_client = BtcServerExtendedClient::new(btc_server_url, None)
            .await
            .expect("Failed to create BTC server client");

        let pub_key = btc_client
            .get_public_key(GetPublicKeyRequest {
                multisig_id: m1_multisig_id.as_u32(),
            })
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "Node {} should have pre-saved key for M1 (multisig ID {})",
                    idx,
                    m1_multisig_id.as_u32()
                )
            });

        it_info_print!(format!(
            "Node {} has pre-saved M1 key (multisig ID {})",
            idx,
            m1_multisig_id.as_u32()
        ));
        m1_pubkeys.push((idx, pub_key.publickey.clone()));
    }

    let first_m1_pubkey = &m1_pubkeys[0].1;
    for (idx, pubkey) in &m1_pubkeys {
        anyhow::ensure!(
            pubkey == first_m1_pubkey,
            "Node {} has different pre-saved key for M1",
            idx
        );
    }
    it_info_print!(format!(
        "✅ All M1 members have matching pre-saved keys for multisig ID {}",
        m1_multisig_id.as_u32()
    ));

    // ========================================================================
    // 4. M1 and M2 should have different aggregate public keys
    // ========================================================================

    anyhow::ensure!(
        first_m1_pubkey != first_m2_pubkey,
        "M1 and M2 should have different aggregate public keys"
    );
    it_info_print!("✅ M1 and M2 have different aggregate public keys");

    // ========================================================================
    // 5. Pegin and pegout using M1 to confirm it still works
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

    let pegin_result = run_pegin(
        &bitcoind_rpc,
        provider,
        &mint_client,
        &mut rx,
        pegin_conf_depth,
        None,
        None,
    )
    .await?;

    let bitcoind_rpc = suite.global_context.bitcoind_rpc();
    run_pegout(&mint_client, &mut rx, &pegin_result, &bitcoind_rpc, None)
        .await?;

    it_info_print!("✅ Pegin and pegout with M1 completed successfully");

    Ok(())
}
