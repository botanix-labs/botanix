use std::time::Duration;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

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

/// Test dynafed when M2 has exactly one more member than M1.
///
/// M1 is the legacy multisig and is pre-saved.
/// M2 is a new multisig that runs DKG.
///
/// This test verifies that:
/// 1. Every M2 member completes DKG and agrees on one aggregate key.
/// 2. The discovered membership sets satisfy M1 ⊂ M2 and |M2| = |M1| + 1.
/// 3. M1 and M2 aggregate public keys are different.
/// 4. Pegin and pegout still work using an M1 member.
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

    let m1_multisig_id = LEGACY_MULTISIG_ID;
    let m2_multisig_id = MultisigId::new(LEGACY_MULTISIG_ID.as_u32() + 1);
    let m2_probe_timeout = Duration::from_secs(180);
    let probe_interval = Duration::from_secs(2);

    // ========================================================================
    // 1. Observe key availability and wait for M2 DKG completion
    // ========================================================================

    it_info_print!(
        "Waiting for DKG completion for multisig ID",
        m2_multisig_id.as_u32()
    );

    let mut m2_pubkeys: BTreeMap<u16, String> = BTreeMap::new();
    let mut m1_pubkeys: BTreeMap<u16, String> = BTreeMap::new();

    for (idx, fed_member) in &test_fed_members {
        let btc_server_url =
            format!("http://{}", fed_member.bitcoin_server_url);
        let mut btc_client = BtcServerExtendedClient::new(btc_server_url, None)
            .await
            .expect("Failed to create BTC server client");

        // M1 is pre-saved for its members. Non-members should return an error.
        if let Ok(pub_key) = btc_client
            .get_public_key(GetPublicKeyRequest {
                multisig_id: m1_multisig_id.as_u32(),
            })
            .await
        {
            m1_pubkeys.insert(*idx, pub_key.publickey);
        }

        // M2 runs DKG. Poll until this node has the M2 key or timeout.
        let start = Instant::now();
        let m2_pub_key = loop {
            match btc_client
                .get_public_key(GetPublicKeyRequest {
                    multisig_id: m2_multisig_id.as_u32(),
                })
                .await
            {
                Ok(pub_key) => {
                    it_info_print!(format!(
                        "DKG completed for node {} multisig ID {}",
                        *idx,
                        m2_multisig_id.as_u32()
                    ));
                    break pub_key;
                }
                Err(_) => {
                    if start.elapsed() >= m2_probe_timeout {
                        anyhow::bail!(
                            "Timed out waiting for DKG on node {} for multisig ID {}",
                            *idx,
                            m2_multisig_id.as_u32()
                        );
                    }
                    it_warn_print!(format!(
                        "DKG pending for node {} multisig ID {}",
                        *idx,
                        m2_multisig_id.as_u32()
                    ));
                    tokio::time::sleep(probe_interval).await;
                }
            }
        };

        m2_pubkeys.insert(*idx, m2_pub_key.publickey.clone());
    }

    anyhow::ensure!(
        m2_pubkeys.len() == test_fed_members.len(),
        "Expected all nodes to have M2 key, got {}/{}",
        m2_pubkeys.len(),
        test_fed_members.len()
    );

    let first_m2_pubkey =
        m2_pubkeys.values().next().expect("at least one M2 key is expected");
    for (idx, pubkey) in &m2_pubkeys {
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
    // 2. Validate membership relation: M1 ⊂ M2 and |M2| = |M1| + 1
    // ========================================================================

    let m1_members: BTreeSet<u16> = m1_pubkeys.keys().copied().collect();
    let m2_members: BTreeSet<u16> = m2_pubkeys.keys().copied().collect();

    anyhow::ensure!(
        m2_members.is_superset(&m1_members),
        "Expected M1 members to be a subset of M2 members. M1={:?}, M2={:?}",
        m1_members,
        m2_members
    );
    anyhow::ensure!(
        m2_members.len() == m1_members.len() + 1,
        "Expected M2 to have exactly one more member than M1. |M1|={}, |M2|={}",
        m1_members.len(),
        m2_members.len()
    );

    let new_members: Vec<u16> =
        m2_members.difference(&m1_members).copied().collect();
    anyhow::ensure!(
        new_members.len() == 1,
        "Expected exactly one new M2 member compared to M1. New members: {:?}",
        new_members
    );
    let new_member = new_members[0];
    it_info_print!(format!(
        "✅ Discovered new member {} (present in M2, absent in M1)",
        new_member
    ));

    anyhow::ensure!(
        !m1_pubkeys.is_empty(),
        "Expected at least one M1 member with a pre-saved key"
    );
    let first_m1_pubkey =
        m1_pubkeys.values().next().expect("at least one M1 key is expected");
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
    // 3. M1 and M2 should have different aggregate public keys
    // ========================================================================

    anyhow::ensure!(
        first_m1_pubkey != first_m2_pubkey,
        "M1 and M2 should have different aggregate public keys"
    );
    it_info_print!("✅ M1 and M2 have different aggregate public keys");

    // ========================================================================
    // 4. Pegin and pegout using M1 to confirm it still works
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

    let m1_member_index =
        *m1_members.iter().next().expect("at least one M1 member is required");
    let provider = Provider::<Http>::try_from(format!(
        "http://localhost:{}",
        test_fed_members
            .get(&m1_member_index)
            .expect("selected M1 member should exist")
            .rpc_port
    ))
    .expect("could not instantiate HTTP Provider");

    let mint_client = test_fed_members
        .get(&m1_member_index)
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
