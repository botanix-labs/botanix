use std::{
    collections::{BTreeMap, HashMap},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    dkg,
    pegout_scheduler::{
        TX_NOT_FOUND_BITCOIND_ERROR, TX_NOT_IN_MEMPOOL_BITCOIND_ERROR,
    },
    wallet::{
        address::generate_taproot_change_scriptpubkey, psbt::PsbtInputExt,
        util::VerifyingKeyExt,
    },
};
use bitcoin::{
    absolute::LockTime, block::Header, blockdata::transaction::TxOut,
    hashes::Hash, psbt::Psbt, secp256k1,
    taproot::Signature as TaprootSignature, Amount, Block, FeeRate, OutPoint,
    ScriptBuf, Sequence, TapSighashType, Transaction, TxIn, Txid, Witness,
};
use bitcoincore_rpc::json::{
    EstimateMode, EstimateSmartFeeResult, StringOrStringArray,
};
use botanix_configs::federation::AuthorityMultisigConfig;
use botanix_types::MultisigId;
use frost_secp256k1_tr as frost;
use rand::{rngs::OsRng, thread_rng, RngCore};
use serde::ser::Error;
use tempfile::TempDir;

use crate::{database, pegout_id::PegoutId, pegout_scheduler::PegoutRequest};

#[macro_export]
macro_rules! frost_id {
    ($index:expr) => {
        frost::Identifier::derive(($index as u16).to_le_bytes().as_slice())
            .expect("valid id")
    };
}

const NETWORK: bitcoin::Network = bitcoin::Network::Regtest;
const FEERATE: FeeRate = FeeRate::from_sat_per_kwu(5 * 250);

pub struct MockBitcoind {
    utxo_set: std::sync::Arc<
        std::sync::Mutex<
            std::collections::HashMap<
                OutPoint,
                bitcoincore_rpc::json::GetTxOutResult,
            >,
        >,
    >,
}

impl MockBitcoind {
    pub fn new() -> Self {
        Self {
            utxo_set: std::sync::Arc::new(
                std::sync::Mutex::new(HashMap::new()),
            ),
        }
    }

    /// Remove an output from the UTXO set (simulating a spent UTXO)
    pub fn remove_utxo(&self, outpoint: OutPoint) {
        self.utxo_set.lock().unwrap().remove(&outpoint);
    }

    /// Add a output to the UTXO set
    pub fn add_utxo(
        &self,
        outpoint: OutPoint,
        value: Amount,
        script_pubkey: ScriptBuf,
    ) {
        let script_hex = hex::encode(script_pubkey.to_bytes());
        let json_str = format!(
            r#"{{
            "bestblock": "0000000000000000000000000000000000000000000000000000000000000000",
            "confirmations": 6,
            "value": {},
            "scriptPubKey": {{
                "asm": "",
                "hex": "{}",
                "type": "nonstandard"
            }},
            "coinbase": false
        }}"#,
            value.to_btc(),
            script_hex
        );

        let result: bitcoincore_rpc::json::GetTxOutResult =
            serde_json::from_str(&json_str).expect("valid JSON");
        self.utxo_set.lock().unwrap().insert(outpoint, result);
    }
}

impl bitcoincore_rpc::RpcApi for MockBitcoind {
    fn get_tx_out(
        &self,
        txid: &Txid,
        vout: u32,
        _include_mempool: Option<bool>,
    ) -> bitcoincore_rpc::Result<Option<bitcoincore_rpc::json::GetTxOutResult>>
    {
        let outpoint = OutPoint::new(*txid, vout);
        let utxo_set = self.utxo_set.lock().unwrap();
        Ok(utxo_set.get(&outpoint).cloned())
    }

    fn get_block_count(&self) -> Result<u64, bitcoincore_rpc::Error> {
        Ok(1)
    }

    fn get_block_hash(
        &self,
        _height: u64,
    ) -> bitcoincore_rpc::Result<bitcoin::BlockHash> {
        Ok(bitcoin::BlockHash::all_zeros())
    }

    fn estimate_smart_fee(
        &self,
        _conf_target: u16,
        _estimate_mode: Option<EstimateMode>,
    ) -> Result<EstimateSmartFeeResult, bitcoincore_rpc::Error> {
        let fee_rate = FeeRate::from_sat_per_vb(3).expect("valid fee rate");
        Ok(EstimateSmartFeeResult {
            fee_rate: Some(Amount::from_sat(fee_rate.to_sat_per_kwu() * 4)),
            errors: None,
            blocks: 1,
        })
    }

    fn get_blockchain_info(
        &self,
    ) -> bitcoincore_rpc::Result<bitcoincore_rpc::json::GetBlockchainInfoResult>
    {
        Ok(bitcoincore_rpc::json::GetBlockchainInfoResult {
            initial_block_download: false,
            // Rest of the fields are unused in application code
            chain: bitcoin::Network::Regtest,
            blocks: 1,
            headers: 1,
            difficulty: 1.0,
            pruned: false,
            warnings: StringOrStringArray::String("".to_string()),
            best_block_hash: bitcoin::BlockHash::all_zeros(),
            median_time: 0,
            verification_progress: 1.0,
            chain_work: vec![],
            size_on_disk: 0,
            prune_height: None,
            automatic_pruning: None,
            prune_target_size: None,
            softforks: HashMap::new(),
        })
    }

    fn call<T: for<'a> serde::de::Deserialize<'a>>(
        &self,
        method: &str,
        params: &[serde_json::Value],
    ) -> Result<T, bitcoincore_rpc::Error> {
        println!("call: {:?}, {:?}", method, params);

        let mut raw_args = Vec::new();
        if !params.is_empty() {
            raw_args = params
                .iter()
                .map(|a| {
                    let json_string = serde_json::to_string(a)?;
                    serde_json::value::RawValue::from_string(json_string)
                })
                .map(|a| a.map_err(bitcoincore_rpc::Error::Json))
                .collect::<Result<Vec<_>, _>>()?;
        }

        if method == "getblockchaininfo" {
            return Ok(serde_json::from_str(
                "{\"initialblockdownload\": false}",
            )
            .unwrap());
        }
        if method == "getbestblockhash" {
            let block_hash = bitcoin::BlockHash::all_zeros();
            return Ok(
                serde_json::from_str(&format!("\"{block_hash}\"",)).unwrap()
            );
        }
        if method == "getblockheaderinfo" {
            let block_hash = bitcoin::BlockHash::all_zeros();
            let current_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            return Ok(serde_json::from_str(
                    &format!("{{\"hash\": \"{block_hash}\", \"confirmations\": 1, \"height\": 1, \"version\": 1, \"version_hex\": \"01000000\", \"merkleroot\": \"{block_hash}\", \"time\": {current_time}, \"mediantime\": {current_time}, \"nonce\": 1, \"bits\": \"1d00ffff\", \"difficulty\": 1, \"chainwork\": \"0000000000000000000000000000000000000000000000000000000000000001\", \"n_tx\": 1, \"previousblockhash\": \"{block_hash}\", \"nextblockhash\": \"{block_hash}\"}}",),
                ).unwrap());
        }
        if method == "getblockheader" {
            let block_hash = bitcoin::BlockHash::all_zeros();
            let current_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            return Ok(serde_json::from_str(
                    &format!("{{\"hash\": \"{block_hash}\", \"confirmations\": 1, \"height\": 1, \"version\": 1, \"version_hex\": \"01000000\", \"merkleroot\": \"{block_hash}\", \"time\": {current_time}, \"mediantime\": {current_time}, \"nonce\": 1, \"bits\": \"1d00ffff\", \"difficulty\": 1, \"chainwork\": \"0000000000000000000000000000000000000000000000000000000000000001\", \"nTx\": 1, \"previousblockhash\": \"{block_hash}\", \"nextblockhash\": \"{block_hash}\"}}",),
                ).unwrap());
        }
        if method == "getmempoolentry" {
            // error case is triggered by a specific txid
            // used by test `track_mempool_should_untrack_and_add_back_pegout_when_not_in_mempool`
            let error_txid =
                String::from("855b53d27666779a179ec93d88dbe28f456040155c4b712a1261ad211f4ba6f2");
            if !raw_args.is_empty()
                && raw_args[0].get().to_string().trim_matches('\"')
                    == error_txid
            {
                return Err(bitcoincore_rpc::Error::Json(
                    serde_json::error::Error::custom(
                        TX_NOT_IN_MEMPOOL_BITCOIND_ERROR,
                    ),
                ));
            }

            let txid = Txid::from_byte_array([0u8; 32]);
            return Ok(serde_json::from_str(&format!("{{\"size\": 250, \"weight\": 1000, \"time\": 1680000000, \"height\": 680000, \"descendantcount\": 2, \"descendantsize\": 500, \"ancestorcount\": 1, \"ancestorsize\": 250, \"wtxid\": \"{txid}\", \"fees\": {{\"base\": 1000, \"modified\": 1100, \"ancestor\": 1200, \"descendant\": 1300}}, \"depends\": [\"{txid}\"], \"spentby\": [\"{txid}\"], \"bip125-replaceable\": true, \"unbroadcast\": false}}",),
                ).unwrap());
        }
        if method == "getrawtransaction" {
            // error cases are triggered by specific txids
            // used by test `track_mempool_should_untrack_and_add_back_pegout_when_not_in_mempool`
            let error_txid_1 =
                String::from("855b53d27666779a179ec93d88dbe28f456040155c4b712a1261ad211f4ba6f2");
            if !raw_args.is_empty()
                && raw_args[0].get().to_string().trim_matches('\"')
                    == error_txid_1
            {
                return Err(bitcoincore_rpc::Error::Json(
                    serde_json::error::Error::custom(
                        TX_NOT_FOUND_BITCOIND_ERROR,
                    ),
                ));
            }

            // used by test `track_mempool_should_not_add_back_pegout_when_still_in_mempool`
            let error_txid_2 =
                String::from("26bbaab2e585d465cceecc2acc7b398069aa85fc4dd1f52e39666a65e54a4569");
            if !raw_args.is_empty()
                && raw_args[0].get().to_string().trim_matches('\"')
                    == error_txid_2
            {
                return Err(bitcoincore_rpc::Error::Json(
                    serde_json::error::Error::custom("Tx in mempool"),
                ));
            }

            let txid = Txid::from_byte_array([0u8; 32]);
            // return Ok(serde_json::from_str(&format!("{{\"size\": 250, \"weight\": 1000, \"time\":
            // 1680000000, \"height\": 680000, \"descendantcount\": 2, \"descendantsize\": 500,
            // \"ancestorcount\": 1, \"ancestorsize\": 250, \"wtxid\": \"{txid}\", \"fees\":
            // {{\"base\": 1000, \"modified\": 1100, \"ancestor\": 1200, \"descendant\": 1300}},
            // \"depends\": [\"{txid}\"], \"spentby\": [\"{txid}\"], \"bip125-replaceable\": true,
            // \"unbroadcast\": false}}",),     ).unwrap());
            return Ok(serde_json::from_str(&format!("{{\"hex\": \"01000000010000000000000000000000000000000000000000000000000000000000000000000000000000ffffffff0100000000000000000000000000\", \"txid\": \"{txid}\", \"hash\": \"{txid}\", \"size\": 250, \"vsize\": 141, \"version\": 1, \"locktime\": 0, \"vin\": [{{\"txid\": \"{txid}\", \"vout\": 0, \"scriptSig\": {{\"asm\": \"coinbase\", \"hex\": \"\"}}, \"sequence\": 4294967295}}], \"vout\": [{{\"value\": 0.0, \"n\": 0, \"scriptPubKey\": {{\"asm\": \"\", \"hex\": \"\", \"type\": \"nonstandard\"}}}}], \"blockhash\": \"0000000000000000000000000000000000000000000000000000000000000000\", \"confirmations\": 680000, \"time\": 1680000000, \"blocktime\": 1680000000}}", txid = txid)).unwrap());
        }

        if method == "getblock" {
            let txid = Txid::from_byte_array([0u8; 32]);
            return Ok(serde_json::from_str(&format!("{{\"hash\": \"0000000000000000000000000000000000000000000000000000000000000000\", \"confirmations\": 680000, \"size\": 1024, \"strippedsize\": 1000, \"weight\": 4000, \"height\": 680000, \"version\": 1, \"version_hex\": \"01000000\", \"merkleroot\": \"0000000000000000000000000000000000000000000000000000000000000000\", \"tx\": [\"{}\"], \"time\": 1680000000, \"mediantime\": 1679999500, \"nonce\": 123456789, \"bits\": \"1a00ffff\", \"difficulty\": 1.0, \"chainwork\": \"0000000000000000000000000000000000000000000000000000000000000000\", \"nTx\": 1, \"previousblockhash\": \"0000000000000000000000000000000000000000000000000000000000000000\", \"nextblockhash\": \"0000000000000000000000000000000000000000000000000000000000000000\"}}", txid)).unwrap());
        }

        unimplemented!()
    }
}

impl Default for MockBitcoind {
    fn default() -> Self {
        Self::new()
    }
}

/* Some Test utils. Should probably be in a separate file */

pub fn create_random_pegout_id() -> PegoutId {
    let mut rng = thread_rng();
    let mut pegout_id = [0u8; 36];
    rng.fill_bytes(&mut pegout_id);
    PegoutId::from_bytes(&pegout_id).unwrap()
}

pub fn pegout_requests_from_tx(
    tx: &Transaction,
    pegout_idxs: &[usize],
) -> Vec<PegoutRequest> {
    let mut pegout_requests = Vec::new();
    for idx in pegout_idxs {
        pegout_requests.push(PegoutRequest {
            spk: tx.output[*idx].script_pubkey.clone(),
            value: tx.output[*idx].value,
            id: create_random_pegout_id(),
            botanix_height: 0,
            timestamp: None,
        });
    }
    pegout_requests
}

pub fn setup_db() -> (database::Db, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db = database::Db::open(temp_dir.path()).unwrap();
    (db, temp_dir)
}

pub fn random_compute_txid() -> Txid {
    let mut rng = thread_rng();
    let mut txid = [0u8; 32];
    rng.fill_bytes(&mut txid);
    Txid::from_slice(&txid).unwrap()
}

pub fn eth_vector_to_fixed_bytes(eth: Vec<u8>) -> [u8; 20] {
    let mut eth_addr = [0u8; 20];
    eth_addr.copy_from_slice(&eth);
    eth_addr
}

pub fn random_p2tr_keyspend_script() -> ScriptBuf {
    let secp = bitcoin::secp256k1::Secp256k1::new();
    let (_key_pair_priv, key_pair_pub) = secp.generate_keypair(&mut OsRng);
    let serialized_pkey = key_pair_pub.serialize();
    generate_taproot_change_scriptpubkey(serialized_pkey)
}

// FIXME: This creates P2WPKH script code (for spending), not scriptpubkey (for outputs).
// Use `random_p2wpkh_scriptpubkey()` instead. Not fixing immediately to avoid breaking tests.
pub fn random_p2wpkh_script() -> ScriptBuf {
    let secp = bitcoin::secp256k1::Secp256k1::new();
    let sk = bitcoin::PrivateKey::generate(NETWORK);
    sk.public_key(&secp).p2wpkh_script_code().unwrap()
}

pub fn random_p2wpkh_scriptpubkey() -> ScriptBuf {
    let secp = bitcoin::secp256k1::Secp256k1::new();
    let sk = bitcoin::PrivateKey::generate(NETWORK);
    let wpk = sk.public_key(&secp).wpubkey_hash().unwrap();
    ScriptBuf::new_p2wpkh(&wpk)
}

pub fn trusted_dealer_setup_from_config(
    config: &AuthorityMultisigConfig,
) -> (
    BTreeMap<frost::Identifier, frost::keys::SecretShare>,
    frost::keys::PublicKeyPackage,
) {
    let rng: rand::prelude::ThreadRng = thread_rng();
    let ids: Vec<_> = config.authorities.keys().copied().collect();
    frost::keys::generate_with_dealer(
        config.max_signers,
        config.min_signers,
        frost::keys::IdentifierList::Custom(&ids),
        rng,
    )
    .expect("valid key package")
}

pub fn trusted_dealer_setup(
    min_signers: u16,
    max_signers: u16,
) -> (
    BTreeMap<frost::Identifier, frost::keys::SecretShare>,
    frost::keys::PublicKeyPackage,
) {
    let rng: rand::prelude::ThreadRng = thread_rng();
    let ids = (0..max_signers).map(|i| frost_id!(i)).collect::<Vec<_>>();
    frost::keys::generate_with_dealer(
        max_signers,
        min_signers,
        frost::keys::IdentifierList::Custom(&ids),
        rng,
    )
    .expect("valid key package")
}

// Util function to create a btc tx with random inputs and outputs as defined by fn params
pub fn create_tx(
    num_inputs: usize,
    num_outputs: usize,
    change: Option<TxOut>,
) -> Transaction {
    let txid = random_compute_txid();

    let mut inputs = vec![];
    for i in 0..num_inputs {
        let op = OutPoint::new(txid, i as u32);
        inputs.push(TxIn {
            previous_output: op,
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Default::default(),
        });
    }

    let mut outputs = vec![];
    for _ in 0..num_outputs {
        outputs.push(TxOut {
            value: Amount::from_sat(1000),
            script_pubkey: random_p2wpkh_scriptpubkey(),
        });
    }

    if let Some(change) = change {
        outputs.push(change);
    }

    Transaction {
        version: bitcoin::transaction::Version(2),
        lock_time: LockTime::ZERO,
        input: inputs,
        output: outputs,
    }
}

pub fn create_block(
    txs: Vec<Transaction>,
    prev_hash: bitcoin::BlockHash,
) -> Block {
    let coin_base_input = TxIn {
        previous_output: OutPoint::new(
            Txid::from_byte_array([0u8; 32]),
            0xFFFFFFFF,
        ),
        script_sig: bitcoin::Script::builder()
            .push_opcode(bitcoin::opcodes::all::OP_PUSHBYTES_3)
            // This hardcodes the height of the block. Could change in the future
            .push_slice([10u8; 3])
            .into_script(),
        sequence: bitcoin::Sequence::MAX,
        witness: bitcoin::Witness::default(),
    };
    let coinbase_tx = Transaction {
        version: bitcoin::transaction::Version(2),
        lock_time: LockTime::ZERO,
        input: vec![coin_base_input],
        output: vec![],
    };
    let mut txdata = vec![coinbase_tx];
    txdata.extend(txs);
    Block {
        header: Header {
            version: bitcoin::blockdata::block::Version::TWO,
            prev_blockhash: prev_hash,
            merkle_root: bitcoin::TxMerkleNode::all_zeros(),
            time: 100,
            bits: bitcoin::CompactTarget::from_consensus(0),
            nonce: 0,
        },
        txdata,
    }
}

pub fn create_psbt(
    num_inputs: usize,
    num_outputs: usize,
    change: Option<TxOut>,
) -> Psbt {
    let tx = create_tx(num_inputs, num_outputs, change);

    let weight = tx.weight();
    let fee = FEERATE * weight;
    let input_needed =
        fee.to_sat() + tx.output.iter().map(|o| o.value.to_sat()).sum::<u64>();
    let value_per_input = input_needed / num_inputs as u64 + 1;

    let mut psbt = Psbt::from_unsigned_tx(tx).expect("valid psbt");
    for i in 0..num_inputs {
        psbt.inputs[i].witness_utxo = Some(TxOut {
            value: Amount::from_sat(value_per_input),
            script_pubkey: ScriptBuf::new(),
        });
    }
    psbt
}

/// Create a sweep PSBT for testing.
///
/// - `num_inputs`: number of inputs (UTXOs being swept)
/// - `source_multisig_id`: the multisig ID that owns the input UTXOs
/// - `change_output`: the single output (should be derived from target multisig)
pub fn create_sweep_psbt(
    num_inputs: usize,
    source_multisig_id: MultisigId,
    change_output: TxOut,
) -> Psbt {
    // Create random outpoints for inputs
    let inputs: Vec<TxIn> = (0..num_inputs)
        .map(|_| {
            let mut txid_bytes = [0u8; 32];
            thread_rng().fill_bytes(&mut txid_bytes);
            TxIn {
                previous_output: OutPoint {
                    txid: Txid::from_slice(&txid_bytes).expect("valid txid"),
                    vout: 0,
                },
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                script_sig: ScriptBuf::new(),
                witness: Witness::default(),
            }
        })
        .collect();

    // Create transaction with single change output
    let tx = Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: LockTime::ZERO,
        input: inputs,
        output: vec![change_output.clone()],
    };

    // Calculate value per input to cover output + fees
    let weight = tx.weight();
    let fee = FEERATE * weight;
    let total_needed = fee.to_sat() + change_output.value.to_sat();
    let value_per_input = total_needed / num_inputs as u64 + 1;

    // Create PSBT and set input metadata
    let mut psbt = Psbt::from_unsigned_tx(tx).expect("valid psbt");
    for i in 0..num_inputs {
        psbt.inputs[i].witness_utxo = Some(TxOut {
            value: Amount::from_sat(value_per_input),
            script_pubkey: ScriptBuf::new(),
        });
        psbt.inputs[i].set_multisig_id(source_multisig_id);
    }

    psbt
}

/// Set up key packages for the given multisig IDs.
///
/// Creates a new trusted dealer key package for each multisig ID and stores it in the database.
pub fn setup_key_packages(db: &database::Db, multisig_ids: &[MultisigId]) {
    for &multisig_id in multisig_ids {
        let (shares, pk_package) = trusted_dealer_setup(2, 2);
        let key_package =
            frost::keys::KeyPackage::try_from(shares[&frost_id!(1)].clone())
                .expect("valid key package");
        db.set_key_package_by_id(multisig_id, key_package)
            .expect("set key package");
        db.set_pubkey_package_by_id(multisig_id, pk_package)
            .expect("set public key package");
    }
}

/// Mark a multisig as the funding multisig by setting its attestation as finalized.
///
/// Must be called after `setup_key_packages` for the given multisig.
pub fn setup_funding_multisig(db: &database::Db, multisig_id: MultisigId) {
    db.set_multisig_attestation(multisig_id, sample_dkg_attestation())
        .expect("set multisig attestation");
    db.mark_multisig_attestation_finalized(multisig_id)
        .expect("mark attestation finalized");
}

pub fn create_change(
    db: &database::Db,
    multisig_id: MultisigId,
    value: Amount,
) -> TxOut {
    let secp_pk = db
        .get_public_key_package_by_id(multisig_id)
        .expect("valid key package")
        .expect("key package exists")
        .verifying_key()
        .to_secp_pk()
        .expect("valid secp pk");
    let serialized_pkey = secp_pk.serialize();
    let change_script =
        crate::wallet::address::generate_taproot_change_scriptpubkey(
            serialized_pkey,
        );
    TxOut { value, script_pubkey: change_script }
}

pub fn get_change(db: &database::Db) -> TxOut {
    let secp_pk = db
        .get_public_key_package()
        .expect("valid key package")
        .expect("key package exists")
        .verifying_key()
        .to_secp_pk()
        .expect("valid secp pk");
    let serialized_pkey = secp_pk.serialize();
    let change_script =
        crate::wallet::address::generate_taproot_change_scriptpubkey(
            serialized_pkey,
        );
    TxOut { value: Amount::from_sat(500), script_pubkey: change_script }
}

pub fn store_pending_pegout(db: &database::Db) -> PegoutId {
    let pegout_id = create_random_pegout_id();
    let pegout_request = PegoutRequest {
        id: pegout_id,
        value: Amount::from_sat(1000),
        spk: random_p2wpkh_scriptpubkey(),
        botanix_height: 0,
        timestamp: None,
    };
    let _ = db.store_pending_pegout(&pegout_request);

    pegout_id
}

// Add dummy signatures to a PSBT to help calculate weight and fee rate
pub fn add_dummy_signatures_to_psbt(
    psbt: &mut Psbt,
    sighash_type: TapSighashType,
) {
    for input in psbt.inputs.iter_mut() {
        // For Taproot (P2TR) transactions
        if let Some(_witness_utxo) = &input.witness_utxo {
            let dummy_schnorr_sig_bytes = vec![0x42u8; 64];
            let dummy_schnorr_sig = secp256k1::schnorr::Signature::from_slice(
                &dummy_schnorr_sig_bytes,
            )
            .expect("Valid dummy signature");

            let taproot_sig =
                TaprootSignature { signature: dummy_schnorr_sig, sighash_type };

            // Set the taproot signature
            input.tap_key_sig = Some(taproot_sig.clone());

            // Create the witness item with the signature
            let witness = Witness::from_slice(&[taproot_sig.to_vec()]);
            input.final_script_witness = Some(witness);
        }
    }
}

/// Returns a structurally valid [`dkg::Attestation`] for use in tests that need
/// an attestation but don't care about its cryptographic content.
pub fn sample_dkg_attestation() -> dkg::Attestation {
    let public_key_package = frost::keys::PublicKeyPackage::deserialize(&[
        0, 35, 15, 138, 179, 3, 12, 223, 219, 150, 131, 74, 116, 233, 236, 196,
        96, 205, 96, 130, 121, 192, 88, 21, 198, 81, 210, 88, 18, 92, 42, 123,
        15, 91, 242, 12, 143, 253, 2, 48, 20, 207, 98, 73, 200, 184, 2, 37,
        182, 195, 78, 253, 240, 207, 168, 162, 116, 88, 191, 170, 184, 100,
        170, 66, 133, 51, 212, 104, 135, 235, 86, 52, 39, 160, 118, 176, 185,
        222, 25, 54, 213, 2, 171, 233, 2, 75, 75, 154, 59, 199, 10, 16, 208,
        24, 249, 238, 56, 171, 146, 37, 245, 114, 93, 3, 108, 238, 81, 170, 95,
        40, 187, 22, 250, 137, 122, 92, 184, 254, 229, 184, 45, 28, 214, 168,
        238, 33, 88, 238, 179, 107, 39, 117, 131, 203, 40, 24, 172, 197, 159,
        249, 40, 76, 205, 49, 208, 14, 123, 169, 145, 252, 96, 128, 142, 96,
        26, 2, 128, 79, 6, 59, 90, 29, 133, 161, 26, 217, 244, 230, 3, 43, 16,
        131, 11, 77, 107, 124, 76, 57, 61, 243, 223, 158, 106, 26, 53, 206,
        109, 88, 216, 237, 241, 160, 231, 195, 147, 240, 172, 229, 29, 194, 34,
        2, 36, 59, 219, 55, 224, 24, 53, 50, 136, 203, 216, 231, 213, 160, 143,
        32, 8, 216, 131, 241, 163, 140, 87, 110, 14, 32, 15, 104, 4, 81, 92,
        191,
    ])
    .unwrap();

    let signing_package = frost::SigningPackage::deserialize(&[
        0, 35, 15, 138, 179, 3, 12, 223, 219, 150, 131, 74, 116, 233, 236, 196,
        96, 205, 96, 130, 121, 192, 88, 21, 198, 81, 210, 88, 18, 92, 42, 123,
        15, 91, 242, 12, 143, 253, 0, 35, 15, 138, 179, 3, 39, 184, 47, 116,
        183, 31, 255, 15, 233, 151, 64, 122, 182, 160, 213, 150, 101, 18, 221,
        237, 206, 233, 101, 68, 187, 31, 193, 156, 108, 90, 41, 186, 3, 156,
        183, 208, 154, 76, 237, 228, 18, 98, 61, 214, 29, 76, 255, 71, 134,
        110, 133, 73, 140, 182, 50, 112, 57, 137, 21, 138, 179, 84, 42, 143,
        127, 52, 39, 160, 118, 176, 185, 222, 25, 54, 213, 2, 171, 233, 2, 75,
        75, 154, 59, 199, 10, 16, 208, 24, 249, 238, 56, 171, 146, 37, 245,
        114, 93, 0, 35, 15, 138, 179, 3, 31, 145, 82, 204, 152, 202, 2, 115,
        141, 62, 109, 126, 139, 134, 192, 7, 17, 75, 177, 229, 158, 117, 245,
        72, 104, 18, 109, 177, 83, 40, 112, 65, 2, 133, 94, 139, 196, 35, 66,
        249, 88, 36, 125, 128, 82, 130, 220, 141, 227, 115, 47, 97, 177, 210,
        70, 169, 102, 175, 182, 12, 133, 192, 177, 224, 252, 172, 197, 159,
        249, 40, 76, 205, 49, 208, 14, 123, 169, 145, 252, 96, 128, 142, 96,
        26, 2, 128, 79, 6, 59, 90, 29, 133, 161, 26, 217, 244, 230, 0, 35, 15,
        138, 179, 3, 97, 207, 188, 213, 163, 68, 61, 110, 42, 206, 77, 178, 97,
        135, 172, 210, 33, 212, 198, 10, 194, 151, 190, 236, 108, 159, 100,
        245, 176, 209, 219, 117, 3, 37, 17, 68, 209, 1, 248, 88, 32, 159, 130,
        157, 33, 68, 74, 92, 154, 206, 137, 218, 220, 5, 100, 56, 133, 63, 98,
        88, 143, 248, 92, 117, 65, 32, 234, 158, 96, 44, 98, 10, 20, 137, 190,
        200, 199, 79, 7, 52, 201, 151, 12, 94, 235, 140, 220, 107, 60, 138, 32,
        78, 44, 33, 72, 220, 2, 118,
    ])
    .unwrap();

    let aggregated_signature = frost::Signature::deserialize(&[
        156, 149, 44, 230, 187, 219, 226, 33, 127, 70, 67, 71, 157, 96, 86,
        141, 187, 135, 210, 29, 249, 125, 232, 34, 26, 136, 49, 47, 185, 18,
        181, 34, 124, 141, 77, 138, 158, 178, 191, 137, 57, 253, 131, 241, 2,
        199, 237, 73, 79, 82, 171, 12, 59, 104, 205, 81, 5, 13, 26, 223, 16,
        114, 204, 123,
    ])
    .unwrap();

    dkg::Attestation {
        multisig_id: MultisigId::from(0),
        public_key_package,
        signing_package,
        signatures: Default::default(),
        aggregated_signature,
    }
}
