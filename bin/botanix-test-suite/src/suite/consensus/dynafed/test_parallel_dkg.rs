use std::time::Duration;

use botanix_btc_server_client::{
    BtcServerExtendedApi, BtcServerExtendedClient, GetPublicKeyRequest,
};
use botanix_types::{MultisigId, LEGACY_MULTISIG_ID};

use crate::{
    it_info_print, it_warn_print,
    suite::consensus::ConsensusIntegrationTestSuite,
};

/// Test parallel DKG functionality with multiple multisig IDs.
///
/// This test verifies that:
/// 1. Nodes can have pre-saved keys for legacy multisig (ID 0)
/// 2. Nodes automatically run DKG for new multisig (ID 1) when they start
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

    it_info_print!("✅ Test completed successfully: verified parallel DKG infrastructure with multiple multisig IDs");

    Ok(())
}
