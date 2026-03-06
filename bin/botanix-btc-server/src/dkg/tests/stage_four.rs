use super::*;
use crate::dkg::Stage;

pub fn complete_stage_four(
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
    assert_eq!(alice.stage(), Stage::RoundFour);
    assert_eq!(bob.stage(), Stage::AwaitingRoundFour);
    assert_eq!(eve.stage(), Stage::AwaitingRoundFour);

    // Bob and Eve are waiting for Alice to send the initial message.
    assert!(bob.send(now).is_none());
    assert!(eve.send(now).is_none());

    assert!(alice.timeout(now).is_none());
    assert!(bob.timeout(now).is_none());
    assert!(eve.timeout(now).is_none());

    {
        let [a1, a2] = CheckedSend::new(&mut alice, now)
            // round4(Alice) -> Eve
            .round4(alice_addr, eve_addr)
            // round4(Alice) -> Bob
            .round4(alice_addr, bob_addr)
            .msgs();

        eve.recv(a1).unwrap();
        bob.recv(a2).unwrap();
    }

    assert!(alice.timeout(now).is_some());
    assert!(bob.timeout(now).is_none());
    assert!(eve.timeout(now).is_none());

    {
        let [b1, b2] = CheckedSend::new(&mut bob, now)
            // ack4(Alice) -> Alice
            .ack_round4(alice_addr, alice_addr)
            // round4(Bob) -> Alice
            .round4(bob_addr, alice_addr)
            .msgs();

        alice.recv(b1).unwrap();
        alice.recv(b2).unwrap();
    }

    {
        let [e1, e2] = CheckedSend::new(&mut eve, now)
            // ack4(Alice) -> Alice
            .ack_round4(alice_addr, alice_addr)
            // round4(Eve) -> Alice
            .round4(eve_addr, alice_addr)
            .msgs();

        alice.recv(e1).unwrap();
        alice.recv(e2).unwrap();
    }

    assert!(alice.timeout(now).is_none());
    assert!(bob.timeout(now).is_some());
    assert!(eve.timeout(now).is_some());

    {
        let [a1, a2, a3, a4] = CheckedSend::new(&mut alice, now)
            // ack4(Bob) -> Bob
            .ack_round4(bob_addr, bob_addr)
            // round4(Bob) -> Eve (forwarded)
            .round4(bob_addr, eve_addr)
            // ack4(Eve) -> Eve
            .ack_round4(eve_addr, eve_addr)
            // round4(Eve) -> Bob (forwarded)
            .round4(eve_addr, bob_addr)
            .msgs();

        bob.recv(a1).unwrap();
        eve.recv(a2).unwrap();
        eve.recv(a3).unwrap();
        bob.recv(a4).unwrap();
    }

    {
        let [b1] = CheckedSend::new(&mut bob, now)
            // ack4(Eve) -> Alice
            .ack_round4(eve_addr, alice_addr)
            .msgs();

        alice.recv(b1).unwrap();
    }

    {
        let [e1] = CheckedSend::new(&mut eve, now)
            // ack4(Bob) -> Alice
            .ack_round4(bob_addr, alice_addr)
            .msgs();

        alice.recv(e1).unwrap();
    }

    assert!(alice.timeout(now).is_none());
    assert!(bob.timeout(now).is_none());
    assert!(eve.timeout(now).is_none());

    assert_eq!(alice.stage(), Stage::Finalized);
    assert_eq!(bob.stage(), Stage::Finalized);
    assert_eq!(eve.stage(), Stage::Finalized);

    (alice, bob, eve)
}

#[test]
fn dkg_complete_stage_four() {
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

    let (alice, bob, eve) = complete_stage_four(
        alice_addr, bob_addr, eve_addr, alice, bob, eve, now,
    );

    // All member reproduced the same public key!
    let (_, alice_pub) = alice.aggregate_key_packages().unwrap();
    let (_, bob_pub) = bob.aggregate_key_packages().unwrap();
    let (_, eve_pub) = eve.aggregate_key_packages().unwrap();

    assert_eq!(alice_pub, bob_pub);
    assert_eq!(alice_pub, eve_pub);
    assert_eq!(bob_pub, eve_pub);

    // All members reproduced the same attestation!
    let alice_attest = alice.attestation().unwrap();
    let bob_attest = bob.attestation().unwrap();
    let eve_attest = eve.attestation().unwrap();

    assert_eq!(alice_attest, bob_attest);
    assert_eq!(alice_attest, eve_attest);
    assert_eq!(bob_attest, eve_attest);
}
