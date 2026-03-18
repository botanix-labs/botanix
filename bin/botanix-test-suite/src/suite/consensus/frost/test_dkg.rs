use super::error::Error;
use crate::suite::consensus::ConsensusIntegrationTestSuite;

use botanix_btc_server_client::{self, BtcServerClient};
use botanix_types::LEGACY_MULTISIG_ID;
use btcserverlib::frost_id;
use frost_secp256k1_tr as frost;
use std::{
    collections::{BTreeMap, VecDeque},
    str::FromStr,
    vec,
};
use tonic::transport::Channel;

pub async fn dkg_flow(
    suite: &ConsensusIntegrationTestSuite,
) -> Result<(), Error> {
    // create btc server clients
    let mut clients: Vec<BtcServerClient<Channel>> = vec![];
    for instance in 0..suite.global_context.fed_instances {
        let port = suite
            .local_context
            .btc_processes
            .as_ref()
            .and_then(|process| {
                process
                    .iter()
                    .nth(instance as usize)
                    .map(|val| val.btc_server_port)
            })
            .ok_or_else(|| Error::InvalidBtcServerPort)?;

        let c = botanix_btc_server_client::BtcServerClient::connect(format!(
            "http://localhost:{}",
            port
        ))
        .await
        .map_err(Error::ServerConnect)?;

        clients.push(c);
    }

    // Getting public key should fail for all clients
    for client in clients.iter_mut() {
        let pk = client
            .get_public_key(tonic::Request::new(
                botanix_btc_server_client::GetPublicKeyRequest {
                    multisig_id: *LEGACY_MULTISIG_ID,
                },
            ))
            .await;
        assert!(pk.is_err());
        let err = pk.err().unwrap();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("Missing key package"));
    }

    // now do the dkg
    do_dkg(&mut clients).await?;

    // Get the pubkey should succeed for all clients
    let mut pkeys: Vec<String> = vec![];
    for client in &mut clients {
        let pk = client
            .get_public_key(tonic::Request::new(
                botanix_btc_server_client::GetPublicKeyRequest {
                    multisig_id: *LEGACY_MULTISIG_ID,
                },
            ))
            .await
            .map_err(Error::Request)?
            .into_inner();

        // Ensure all pks can be serialized as secp public keys
        let _ = bitcoin::secp256k1::PublicKey::from_str(&pk.publickey)
            .map_err(Error::PubKeyParse)?;

        pkeys.push(pk.publickey);
    }

    // Ensure everyone got the same pks
    pkeys.dedup();
    if pkeys.len() != 1 {
        return Err(Error::PublicKeyMismatch);
    }

    Ok(())
}

pub async fn do_dkg(
    clients: &mut [botanix_btc_server_client::BtcServerClient<Channel>],
) -> Result<(), Error> {
    // Create a mapping of client index to frost identifier
    let mut frost_id_map = BTreeMap::new();

    for (idx, client) in clients.iter_mut().enumerate() {
        // Retrieve the Frost Id directly from the btc-server
        let resp = client
            .get_frost_id(tonic::Request::new(
                botanix_btc_server_client::Empty {},
            ))
            .await
            .map_err(Error::Request)?
            .into_inner();

        let frost_id = frost::Identifier::deserialize(&resp.frost_id)
            .expect("frost Id returned from btc-server must be valid");

        frost_id_map.insert(frost_id, idx);
    }

    let mut queue: VecDeque<botanix_btc_server_client::DkgPayload> =
        VecDeque::new();

    // Kick-off the initial DKG payloads; only the coordinator will have
    // payloads to send.
    for client in clients.iter_mut() {
        let p = client
            .get_dkg_payloads(tonic::Request::new(
                botanix_btc_server_client::GetDkgPayloadsRequest {
                    multisig_id: *LEGACY_MULTISIG_ID,
                },
            ))
            .await
            .map_err(Error::Request)?
            .into_inner();

        for p in p.payloads {
            queue.push_back(p);
        }
    }

    let mut attestations = Vec::new();

    // Forward each payload to the correct client, and push the resulting
    // payloads back into the queue.
    while let Some(p) = queue.pop_front() {
        // Find the corresponding client.
        let frost_id = frost::Identifier::deserialize(&p.recipient).unwrap();
        let idx = frost_id_map.get(&frost_id).unwrap();
        let recipient = clients.get_mut(*idx).unwrap();

        let p = recipient
            .new_dkg_payload(p)
            .await
            .map_err(Error::Request)?
            .into_inner();

        for p in p.payloads {
            queue.push_back(p);
        }

        // Track any resulting Dkg attestation.
        if let Some(att) = p.attestation {
            attestations.push(att);
        }
    }

    // Validate that the DKG attestations are equal for all participants.
    attestations.dedup();
    if attestations.len() != 1 {
        return Err(Error::DkgAttestationMismatch);
    }

    let att = attestations.into_iter().next().unwrap();
    let multisig_id = att.multisig_id;

    // Explicitly mark the multisig as finalized for each client. Normally this
    // is done directly via the [`NewConsensusCheckpoint`] endpoint.
    for client in clients.iter_mut() {
        let req = botanix_btc_server_client::MarkMultisigFinalizedRequest {
            multisig_id,
        };

        client.mark_multisig_finalized(req).await.map_err(Error::Request)?;
    }

    Ok(())
}
