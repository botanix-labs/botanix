# Dynafed Migration Playbook: M1 → M2

**Purpose**: Coordinator-focused playbook for migrating from the current multisig federation (M1) to a new federation (M2). Each phase includes verification steps to confirm successful completion before proceeding.

---

## High-Level Flow

```text
Config rollout → DKG → Validation → Grace period → End grace → Final sweep → M1 decommission
```

---

## Phase 1: Config Rollout

**Goal**: All nodes have new config with both M1 and M2. No behavior change yet.

### 1.1 Create new config file

- [ ] Create config with M1 and M2 entries

### 1.2 Test on a non-coordinator node first

- [ ] Restart one non-coordinator node with the new config
- [ ] Verify: node behaves the same as other nodes and participates in FROST signing

### 1.3 Distribute new release with new config to other node operators

- [ ] Send new release with new config file to other operators
- [ ] _TBD: Mechanism for nodes to pull config automatically (e.g. artefact)?_

### 1.4 All nodes restarted with new config

- [ ] Verify: other nodes are running correct release and config
- [ ] Verify: M2 nodes are online
- [ ] Confirm everything working smoothly, no behavior change

---

## Phase 2: DKG

**Goal**: M2 key shares generated; new aggregate pubkey available.

### 2.1 Trigger DKG

- [ ] Coordinator calls endpoint / CLI to initiate DKG for M2
- [ ] _Endpoint TBD_

### 2.2 Monitor DKG completion

- [ ] Verify attestation was submitted to blockchain
- [ ] Verify all nodes' MultisigManager has M2 in funding state and M1 in degrading state

---

## Phase 3: Pre-Migration Validation

**Goal**: Confirm M2 can do pegins and pegouts before we start directing funds to it.

At this point, pegins to M2 are allowed by the minting contract, but M2 gateway addresses have not been advertised yet. We keep M2 "unlisted" until we can verify it works manually—only risking our own funds. Once pegins and pegouts are verified, we restart the coordinator node with a flag that sets the change address to M2, spends from M1, and updates the `GetGatewayAddress` endpoint to return the M2 address (see Phase 4).

### 3.1 Manual test with M2 pegin and pegout

- [ ] Perform pegin to M2 gateway address (obtain address via internal/coordinator; not yet public)
- [ ] Perform pegout spending from M2 pegin (above)
- [ ] Verify both succeed

---

## Phase 4: Start Migration (Grace Period)

**Goal**: Direct funds to M2. M1 is in a ready state to sweep.

### 4.1 Restart coordinator and begin migration

- [ ] Restart coordinator node with flag: fund pegouts using M1, set change address to M2, update `GetGatewayAddress` to return M2 address
- [ ] Bridge now gives out M2 gateway addresses for new pegins
- [ ] Monitor UTXOs held by M1

### 4.2 Reach out to communities

- [ ] Warn about needing to use the new gateway address

### 4.3 Prepare M1 UTXOs for sweep

- [ ] As we approach the end of the grace period, we can control the UTXOs left in M1 by setting the change output to M1 or M2. Aim for at most ~80 remaining UTXOs in M1 in order to perform the sweep transaction smoothly
- [ ] We can also perform pegins and pegouts to increase or decrease (consolidate) the M1 UTXOs, respectively

---

## Phase 5: End Grace Period

**Goal**: Stop accepting M1 pegins; prepare for final sweep.

### 5.1 Verify M1 UTXO count

- [ ] Verify M1 has fewer than 80 UTXOs

### 5.2 Begin sunsetting M1

- [ ] Coordinator calls endpoint / CLI to begin sunsetting M1
- [ ] _Endpoint TBD_

### 5.3 Confirm minting contract and pegout state

- [ ] Minting contract validation only accepts pegins to M2 gateway addresses
- [ ] Verify that pegouts are temporarily halted

---

## Phase 6: Final Sweep

**Goal**: All remaining M1 UTXOs swept to M2; migration complete.

### 6.1 Initiate final sweep transaction

- [ ] Coordinator calls endpoint / CLI to initiate final sweep transaction
- [ ] _Endpoint TBD_
- [ ] Verify sweep transaction was broadcast successfully

### 6.2 Track sweep and resume pegouts

- [ ] When sweep tx is deeply (18 blocks) confirmed, confirm that the new UTXO is part of the DB's available UTXOs.

---

## Phase 7: M1 Decommissioning

**Goal**: M1 nodes shut down; keys sent to recovery service.

### 7.1 Send encrypted key to Pegin Recovery Service

- [ ] M1 node operators use CLI tool to export their encrypted private key share
- [ ] Import the private key share into the Pegin Recovery Service
- [ ] When we have a threshold number of key shares, verify the pegin recovery works by recovering funds sent to an M1 gateway address
- [ ] M1 node operators are free to shut down or delete their node instances.
