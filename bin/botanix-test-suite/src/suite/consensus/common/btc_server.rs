use crate::{
    context::GlobalContext, it_info_print,
    suite::consensus::common::is_port_free,
};
use alloy_primitives::Address;
use anyhow::Context;
use botanix_configs::hash::compute_config_hash;
use botanix_consensus_common::utils::unix_timestamp;
use botanix_types::{MultisigId, LEGACY_MULTISIG_ID};
use btcserverlib::{
    database::Db as BtcDatabase,
    federation_args::{
        FedMemberPubKey, FederationRole, FederationTomlConfig, MultisigConfig,
    },
    frost_id,
};
use frost_secp256k1_tr as frost;
use reth_network_peers::PeerId;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    vec,
};
use tokio::process::Child;

use super::{kill_process_at_port, spawn_child_process, Scope};

pub const BTC_SERVER_START_PORT: u16 = 8000;
pub const BTC_SERVER_HTTP_PORT: u16 = 7000;

#[derive(Debug)]
pub struct SpawnedBtcServerProcess {
    pub btc_server_port: u16,
    pub db_path: PathBuf,
    pub child_process: Child,
}

impl SpawnedBtcServerProcess {
    pub async fn destroy_all_async(&mut self) {
        // kill the process
        let _ = self.child_process.kill().await;
        // additionally make sure all ports used are freed
        kill_process_at_port(self.btc_server_port);
        // delete the created db
        if let Err(e) = std::fs::remove_dir_all(&self.db_path) {
            warn!(
                "Couldn't remove btc server db dir at {}: {}",
                self.db_path.display(),
                e
            );
        }
    }

    pub async fn destroy_all_sync(&self) {
        // kill the process
        let pid = self.child_process.id().expect("Expected a process id");
        let _ = std::process::Command::new("kill")
            .arg("-9") // Use SIGKILL for immediate termination
            .arg(format!("{pid}"))
            .output();
        // additionally make sure all ports used are freed
        kill_process_at_port(self.btc_server_port);
        // delete the created db
        if let Err(e) = std::fs::remove_dir_all(&self.db_path) {
            warn!(
                "Couldn't remove btc server db dir at {}: {}",
                self.db_path.display(),
                e
            );
        }
    }
}

/// Pre-save FROST key packages for a federation member.
///
/// This allows nodes to skip DKG for these multisigs during testing.
///
/// # Arguments
/// * `db_path` - Path to the BTC server database directory
/// * `multisig_ids` - Slice of multisig IDs to pre-populate with keys
/// * `frost_identifiers` - The list of FROST identifiers for all federation members.
///   These must match what BtcServer uses at runtime (derived via `frost_id!` macro).
/// * `member_index` - The index of this federation member (0-based), used to look up
///   the correct identifier from `frost_identifiers`
/// * `min_signers` - Minimum threshold of signers for FROST
fn presave_multisig_keys(
    db_path: &Path,
    multisig_ids: &[MultisigId],
    frost_identifiers: &[frost::Identifier],
    member_index: u16,
    min_signers: u16,
) -> anyhow::Result<()> {
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    let max_signers = frost_identifiers.len() as u16;

    // Open the BTC server database
    let btc_db = BtcDatabase::open(db_path)?;

    for multisig_id in multisig_ids {
        // Generate dummy FROST keys using trusted dealer with deterministic seed
        // In a real scenario, all members would generate these together through DKG
        // For testing, we use a deterministic seed based on multisig_id to ensure
        // all nodes generate the same keys for the same multisig_id
        let mut seed = [0u8; 32];
        seed[0..4].copy_from_slice(&multisig_id.as_u32().to_le_bytes());
        let mut rng = StdRng::from_seed(seed);

        let (shares, pubkeys): (
            std::collections::BTreeMap<
                frost::Identifier,
                frost::keys::SecretShare,
            >,
            frost::keys::PublicKeyPackage,
        ) = frost::keys::generate_with_dealer(
            max_signers,
            min_signers,
            frost::keys::IdentifierList::Custom(frost_identifiers),
            &mut rng,
        )?;

        // Get the key package for this specific member using their identifier
        let my_identifier =
            frost_identifiers.get(member_index as usize).ok_or_else(|| {
                anyhow::anyhow!(
                    "Member index {} out of bounds for {} identifiers",
                    member_index,
                    frost_identifiers.len()
                )
            })?;

        let key_package = frost::keys::KeyPackage::try_from(
            shares
                .get(my_identifier)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Share not found for member {}",
                        member_index
                    )
                })?
                .clone(),
        )?;

        // Save both the key package and public key package to the database
        btc_db.set_key_package_by_id(*multisig_id, key_package)?;
        btc_db.set_pubkey_package_by_id(*multisig_id, pubkeys)?;

        it_info_print!(
            "Pre-saved FROST keys for BTC server {} multisig ID {}",
            member_index,
            multisig_id
        );
    }

    Ok(())
}

fn spawn_btc_server_process(
    global_context: Arc<GlobalContext>,
    members_keypairs: &Vec<(
        secp256k1::SecretKey,
        secp256k1::PublicKey,
        PeerId,
        Address,
    )>,
    id: u16,
    btc_server_port: u16,
    db_path: PathBuf,
    multisig_configs: Vec<MultisigConfig>,
) -> anyhow::Result<SpawnedBtcServerProcess> {
    let db_path_arg = db_path.display().to_string();

    let mut working_directory = std::env::current_dir().unwrap();
    for _ in 0..2 {
        working_directory.pop();
    }

    let identifier = id.to_string();
    let coordinator = 0u16.to_string();
    let address = format!("0.0.0.0:{}", btc_server_port);
    let _http_port = (BTC_SERVER_HTTP_PORT + id).to_string();

    let command = "target/debug/botanix-btc-server";
    let binary_abs_path = working_directory.join(Path::new(command));
    if !std::fs::exists(&binary_abs_path)? {
        return Err(anyhow::anyhow!("botanix-btc-server binary not found at {}. Please compile it first before running the test-suite", binary_abs_path.display().to_string()));
    }

    // Write federation config to tempfile with the provided multisig configs
    let federation_config = FederationTomlConfig::new(
        multisig_configs,
        String::new(), // Not needed
        String::new(), // Not needed
        String::new(), // Not needed
    )
    .expect("valid federation config");

    let mut temp_federation = tempfile::NamedTempFile::new().unwrap();
    let federation_toml = toml::to_string(&federation_config)?;
    let config_hash = compute_config_hash(&federation_toml);
    std::io::Write::write_all(
        &mut temp_federation,
        federation_toml.as_bytes(),
    )?;

    // Write the secret key to a tempfile
    let my_secret_key = members_keypairs
        .get(id as usize)
        .cloned()
        .expect("To have keypair information")
        .0;

    let mut temp_secret_key = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(
        &mut temp_secret_key,
        my_secret_key.display_secret().to_string().as_bytes(),
    )?;

    let federation_path = temp_federation.path().to_str().unwrap().to_owned();
    let secret_key_path = temp_secret_key.path().to_str().unwrap().to_owned();

    let args = vec![
        "--btc-network",
        "regtest",
        "--db",
        db_path_arg.as_str(),
        "--identifier",
        identifier.as_str(),
        "--coordinator",
        coordinator.as_str(),
        "--federation-config-path",
        federation_path.as_str(),
        "--config-hash",
        config_hash.as_str(),
        "--p2p-secret-key",
        secret_key_path.as_str(),
        "--address",
        address.as_str(),
        "--toml",
        "./bin/botanix-btc-server/config.toml",
        "--bitcoind-url",
        global_context.bitcoind_url.as_str(),
        "--bitcoind-user",
        global_context.bitcoind_user.as_str(),
        "--bitcoind-pass",
        global_context.bitcoind_pass.as_str(),
        "--fee-rate-diff-percentage",
        "50",
        "--fall-back-fee-rate-sat-per-vbyte",
        "5",
    ];

    // Keep the temp files alive for the duration of the test
    std::mem::forget(temp_federation);
    std::mem::forget(temp_secret_key);

    Ok(SpawnedBtcServerProcess {
        child_process: spawn_child_process(
            Scope::BtcServer(id),
            command,
            args,
            working_directory,
        )?,
        db_path,
        btc_server_port,
    })
}

pub fn spawn_n_btc_server_processes(
    global_context: Arc<GlobalContext>,
    members_keypairs: &Vec<(
        secp256k1::SecretKey,
        secp256k1::PublicKey,
        PeerId,
        Address,
    )>,
    num_multisigs: u16,
    presave_multisigs: &[MultisigId],
) -> anyhow::Result<Vec<SpawnedBtcServerProcess>> {
    let mut processes = vec![];

    // Create multisig configs for all multisigs
    let mut multisig_configs = vec![];
    for offset in 0..num_multisigs {
        let mut fed_members = vec![];
        for i in 0..global_context.fed_instances {
            let public_key = members_keypairs
                .get(i as usize)
                .cloned()
                .expect("To have keypair information")
                .1;

            fed_members.push(FedMemberPubKey {
                key: public_key.to_string(),
                socket_addr: String::new(),
                role: FederationRole::Continuing,
            });
        }

        let multisig_id =
            MultisigId::new(LEGACY_MULTISIG_ID.as_u32() + offset as u32);
        multisig_configs.push(MultisigConfig::new(
            multisig_id,
            global_context.min_signers,
            global_context.max_signers,
            fed_members,
        ));
    }

    // Generate FROST identifiers once for all federation members.
    // These must match what BtcServer uses at runtime (derived via frost_id! macro).
    let frost_identifiers: Vec<frost::Identifier> =
        (0..global_context.max_signers).map(|i| frost_id!(i)).collect();

    for i in 0..global_context.fed_instances {
        let temp_db_path = tempfile::TempDir::new()
            .context("error creating tempdir")?
            .keep()
            .join(format!("_{}", unix_timestamp().to_string()));
        std::fs::create_dir_all(&temp_db_path)
            .context("failed to create tempdir with db subdir")?;
        let db_path = Path::new(&temp_db_path).join(format!("db{}", i));
        std::fs::create_dir_all(&db_path)
            .context("failed to create tempdir with db subdir")?;

        // PRE-SAVE DUMMY KEYS BEFORE SPAWNING THE PROCESS
        if !presave_multisigs.is_empty() {
            presave_multisig_keys(
                &db_path,
                presave_multisigs,
                &frost_identifiers,
                i,
                global_context.min_signers,
            )?;
        }

        let btc_server_port = BTC_SERVER_START_PORT + i;

        if !is_port_free(btc_server_port) {
            return Err(anyhow::anyhow!(
                "❌ BTC Server {} needs port {} but it's already in use by another process",
                i,
                btc_server_port
            ));
        }

        let child_process = spawn_btc_server_process(
            global_context.clone(),
            members_keypairs,
            i,
            btc_server_port,
            db_path.clone(),
            multisig_configs.clone(),
        )?;
        processes.push(child_process);
    }
    Ok(processes)
}
