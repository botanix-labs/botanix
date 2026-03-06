use super::*;
use crate::dkg::Stage;

pub fn complete_stage_three(
    alice_addr: frost::Identifier,
    bob_addr: frost::Identifier,
    eve_addr: frost::Identifier,
    //
    mut alice: DkgStateMachine,
    mut bob: DkgStateMachine,
    mut eve: DkgStateMachine,
    //
    now: Instant,
) -> (DkgStateMachine, DkgStateMachine, DkgStateMachine) {
    assert_eq!(alice.stage(), Stage::RoundThree);
    assert_eq!(bob.stage(), Stage::RoundThree);
    assert_eq!(eve.stage(), Stage::RoundThree);

    // Bob and Eve are waiting for Alice to send the initial message.
    assert!(bob.send(now).is_none());
    assert!(eve.send(now).is_none());

    assert!(alice.timeout(now).is_none());
    assert!(bob.timeout(now).is_none());
    assert!(eve.timeout(now).is_none());

    {
        let [a1, a2] = CheckedSend::new(&mut alice, now)
            // round3(Alice) -> Eve
            .round3(alice_addr, eve_addr)
            // round3(Alice) -> Bob
            .round3(alice_addr, bob_addr)
            .msgs();

        eve.recv(a1).unwrap();
        bob.recv(a2).unwrap();
    }

    assert!(alice.timeout(now).is_some());
    assert!(bob.timeout(now).is_none());
    assert!(eve.timeout(now).is_none());

    {
        let [b1, b2] = CheckedSend::new(&mut bob, now)
            // ack3(Alice) -> Alice
            .ack_round3(alice_addr, alice_addr)
            // round3(Bob) -> Alice
            .round3(bob_addr, alice_addr)
            .msgs();

        alice.recv(b1).unwrap();
        alice.recv(b2).unwrap();
    }

    {
        let [e1, e2] = CheckedSend::new(&mut eve, now)
            // ack3(Alice) -> Alice
            .ack_round3(alice_addr, alice_addr)
            // round3(Eve) -> Alice
            .round3(eve_addr, alice_addr)
            .msgs();

        alice.recv(e1).unwrap();
        alice.recv(e2).unwrap();
    }

    assert!(alice.timeout(now).is_none());
    assert!(bob.timeout(now).is_some());
    assert!(eve.timeout(now).is_some());

    {
        // NOTE: We only read up to two messages, since Alice immediately
        // kicks-off the Round4 messages, which will be checked in the next
        // stage test.
        let [a1, a2] = CheckedSend::new_max_msg(&mut alice, now, 2)
            // ack3(Bob) -> Bob
            .ack_round3(bob_addr, bob_addr)
            // ack3(Eve) -> Eve
            .ack_round3(eve_addr, eve_addr)
            .msgs();

        bob.recv(a1).unwrap();
        eve.recv(a2).unwrap();
    }

    assert!(alice.timeout(now).is_none());
    assert!(bob.timeout(now).is_none());
    assert!(eve.timeout(now).is_none());

    assert_eq!(alice.stage(), Stage::RoundFour);
    assert_eq!(bob.stage(), Stage::AwaitingRoundFour);
    assert_eq!(eve.stage(), Stage::AwaitingRoundFour);

    (alice, bob, eve)
}

#[test]
fn dkg_complete_stage_three() {
    let (alice_addr, bob_addr, eve_addr, alice, bob, eve) =
        setup(test_config());

    let now = Instant::now();

    let (alice, bob, eve) = complete_stage_one(
        alice_addr, bob_addr, eve_addr, alice, bob, eve, now,
    );

    let (alice, bob, eve) = complete_stage_two(
        alice_addr, bob_addr, eve_addr, alice, bob, eve, now,
    );

    let (alice, bob, eve) = complete_stage_three(
        alice_addr, bob_addr, eve_addr, alice, bob, eve, now,
    );

    // All member reproduced the same public key!
    let (_, alice_pub) = alice.aggregate_key_packages().unwrap();
    let (_, bob_pub) = bob.aggregate_key_packages().unwrap();
    let (_, eve_pub) = eve.aggregate_key_packages().unwrap();

    assert_eq!(alice_pub, bob_pub);
    assert_eq!(alice_pub, eve_pub);
    assert_eq!(bob_pub, eve_pub);

    // Attestations are not ready yet.
    assert!(alice.attestation().is_none());
    assert!(bob.attestation().is_none());
    assert!(eve.attestation().is_none());
}
