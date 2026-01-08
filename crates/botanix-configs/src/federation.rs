use displaydoc::Display as DisplayDoc;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Write},
    net::{SocketAddr, ToSocketAddrs},
    path::{Path, PathBuf},
    str::FromStr,
};
use thiserror::Error;

/// Error type for genesis config
#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Open config file: {0}
    #[allow(dead_code)]
    OpenConfig(std::io::Error),
    /// Failed to parse config: {0}
    ParseConfig(toml::de::Error),
    /// Failed to serialize parse config: {0}
    ParseSerializeConfig(toml::ser::Error),
    /// Failed to parse config as utf-8: {0}
    #[allow(dead_code)]
    ParseUtf8(std::string::FromUtf8Error),
    /// Failed to read config file: {0}
    #[allow(dead_code)]
    ReadConfig(std::io::Error),
    /// Failed to read config metadata: {0}
    #[allow(dead_code)]
    ReadMeta(std::io::Error),
    /// Failed to read public key: {0}
    #[allow(dead_code)]
    InvalidPublicKeyFormat(#[from] secp256k1::Error),
    /// Failed to read config socket address: {0}
    #[allow(dead_code)]
    InvalidSocketAddress(#[from] std::net::AddrParseError),
    /// Failed to resolve socket address via DNS lookup: {0}
    #[allow(dead_code)]
    SocketAddressResolution(std::io::Error),
    /// Invalid federation configuration: {0}
    #[allow(dead_code)]
    InvalidConfig(String),
    /// Missing multisig configuration entries
    #[allow(dead_code)]
    MissingMultisigs,
}
/// Federation member public key and socket address
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct FedMemberPubKey {
    /// The pub key of the member
    pub key: String,
    /// The socket address of the member
    pub socket_addr: String,
    /// The role of the member during federation transitions
    #[serde(default)]
    pub role: FederationRole,
}

/// Member role (outgoing/continuing/incoming)
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Deserialize,
    Serialize,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
)]
#[serde(rename_all = "kebab-case")]
pub enum FederationRole {
    Incoming,
    #[default]
    Continuing,
    Outgoing,
}

/// Multisig definition and its members
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct MultisigConfig {
    /// Identifier for this multisig (0 = pre-dynafed, 1 = next, ...)
    pub multisig_id: u32,
    /// Threshold for this multisig
    pub min_signers: u16,
    /// Total number of signers for this multisig (defaults to member count)
    #[serde(default)]
    pub max_signers: Option<u16>,
    /// Members participating in this multisig
    pub federation_member_public_key: Vec<FedMemberPubKey>,
}

impl MultisigConfig {
    #[allow(dead_code)]
    pub fn new(
        multisig_id: u32,
        min_signers: u16,
        max_signers: u16,
        federation_member_public_key: Vec<FedMemberPubKey>,
    ) -> Self {
        Self {
            multisig_id,
            min_signers,
            max_signers: Some(max_signers),
            federation_member_public_key,
        }
    }

    /// Returns max_signers if set, otherwise defaults to the member count.
    pub fn effective_max_signers(&self) -> u16 {
        self.max_signers
            .unwrap_or(self.federation_member_public_key.len() as u16)
    }
}

/// Configuration for the genesis block (toml)
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct FederationTomlConfig {
    /// List of multisig definitions
    #[serde(default)]
    pub multisig: Vec<MultisigConfig>,
    /// botanix fee recipient
    pub botanix_fee_recipient: String,
    /// The precompiled Minting contract bytecode
    pub minting_contract_bytecode: String,
    /// LST fee receiver
    pub lst_fee_receiver: String,
}

impl FederationTomlConfig {
    #[allow(dead_code)]
    pub(crate) async fn new_from_path(
        path: impl AsRef<Path> + Send,
    ) -> Result<Self, Error> {
        read_to_string(path)?.parse()
    }

    /// Create a new genesis config
    pub fn new(
        multisig: Vec<MultisigConfig>,
        botanix_fee_recipient: String,
        minting_contract_bytecode: String,
        lst_fee_receiver: String,
    ) -> Result<Self, Error> {
        let mut config = Self {
            multisig,
            botanix_fee_recipient,
            minting_contract_bytecode,
            lst_fee_receiver,
        };
        config.finalize()?;
        Ok(config)
    }
    /// Write the config to a file
    pub fn write_to_path(
        &self,
        path: impl AsRef<Path> + Send,
    ) -> Result<(), Error> {
        let toml =
            toml::to_string(self).map_err(Error::ParseSerializeConfig)?;
        let mut file = File::create(path).map_err(Error::OpenConfig)?;
        file.write_all(toml.as_bytes()).map_err(Error::ReadConfig)
    }

    /// Convert the config to a string
    pub fn to_string(&self) -> Result<String, Error> {
        toml::to_string(self).map_err(Error::ParseSerializeConfig)
    }

    fn finalize(&mut self) -> Result<(), Error> {
        self.validate()?;
        Ok(())
    }

    /// Extracts federation public keys and socket addresses for a specific
    /// multisig id.
    pub fn get_federation_pks_for_multisig(
        &self,
        multisig_id: u32,
    ) -> Result<Vec<(secp256k1::PublicKey, SocketAddr)>, Error> {
        self.get_federation_pks_internal(multisig_id)
    }

    fn get_federation_pks_internal(
        &self,
        multisig_id: u32,
    ) -> Result<Vec<(secp256k1::PublicKey, SocketAddr)>, Error> {
        let multisig = self.select_multisig(multisig_id)?;
        let federation_members = multisig
            .federation_member_public_key
            .iter()
            .map(|key| {
                let public_key = secp256k1::PublicKey::from_str(&key.key)
                    .map_err(Error::from)?;

                let soc_addr = match key.socket_addr.parse::<SocketAddr>() {
                    Ok(addr) => addr,
                    Err(_) => key
                        .socket_addr
                        .to_socket_addrs()
                        .map_err(Error::SocketAddressResolution)?
                        .next()
                        .ok_or_else(|| {
                            Error::SocketAddressResolution(std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                format!(
                                    "No addresses resolved for {}",
                                    key.socket_addr
                                ),
                            ))
                        })?,
                };

                Ok((public_key, soc_addr))
            })
            .collect::<Result<Vec<(secp256k1::PublicKey, SocketAddr)>, Error>>(
            )?;

        Ok(federation_members)
    }

    fn select_multisig(
        &self,
        multisig_id: u32,
    ) -> Result<&MultisigConfig, Error> {
        if self.multisig.is_empty() {
            return Err(Error::MissingMultisigs);
        }

        let selected = self
            .multisig
            .iter()
            .find(|m| m.multisig_id == multisig_id)
            .ok_or_else(|| {
                Error::InvalidConfig(format!(
                    "missing multisig id {}",
                    multisig_id
                ))
            })?;

        Ok(selected)
    }

    pub fn get_config_by_multisig_id(
        &self,
        multisig_id: u32,
    ) -> Option<&MultisigConfig> {
        self.multisig.iter().find(|m| m.multisig_id == multisig_id)
    }

    fn validate(&self) -> Result<(), Error> {
        if self.multisig.is_empty() {
            return Err(Error::MissingMultisigs);
        }
        if self.multisig.len() > 2 {
            return Err(Error::InvalidConfig(format!(
                "invalid number of multisigs: expected 1 or 2, found {}",
                self.multisig.len()
            )));
        }

        let mut seen_ids = BTreeSet::new();
        for multisig in &self.multisig {
            if !seen_ids.insert(multisig.multisig_id) {
                return Err(Error::InvalidConfig(format!(
                    "duplicate multisig id {}",
                    multisig.multisig_id
                )));
            }
            Self::validate_signer_constraints(multisig)?;
        }

        if self.multisig.len() == 2 {
            let mut sorted_multisigs =
                self.multisig.iter().collect::<Vec<&MultisigConfig>>();
            sorted_multisigs.sort_by_key(|m| m.multisig_id);
            let current = sorted_multisigs[0];
            let next = sorted_multisigs[1];
            Self::validate_dynafed_roles(current, next)?;
        }

        Ok(())
    }

    fn validate_signer_constraints(
        multisig: &MultisigConfig,
    ) -> Result<(), Error> {
        let member_count = multisig.federation_member_public_key.len();
        if member_count < 2 {
            return Err(Error::InvalidConfig(format!(
                "multisig {} must list at least two members",
                multisig.multisig_id
            )));
        }

        let max_signers = multisig.effective_max_signers();
        if max_signers as usize != member_count {
            return Err(Error::InvalidConfig(format!(
                "multisig {} max-signers ({}) must equal listed members ({})",
                multisig.multisig_id, max_signers, member_count
            )));
        }

        if multisig.min_signers < 2 || multisig.min_signers > max_signers {
            return Err(Error::InvalidConfig(format!(
                "multisig {} min-signers ({}) must be within 2..={} (member count)",
                multisig.multisig_id, multisig.min_signers, max_signers
            )));
        }

        Ok(())
    }

    fn validate_dynafed_roles(
        current: &MultisigConfig,
        next: &MultisigConfig,
    ) -> Result<(), Error> {
        // Current multisig must not have incoming members
        if current
            .federation_member_public_key
            .iter()
            .any(|member| member.role == FederationRole::Incoming)
        {
            return Err(Error::InvalidConfig(format!(
                "incoming members must not be defined in multisig {}",
                current.multisig_id
            )));
        }

        // Next multisig must not have outgoing members
        if next
            .federation_member_public_key
            .iter()
            .any(|member| member.role == FederationRole::Outgoing)
        {
            return Err(Error::InvalidConfig(format!(
                "outgoing members must not be defined in multisig {}",
                next.multisig_id
            )));
        }

        // Continuing members must be identical across multisigs
        let continuing_current = Self::continuing_member_set(current);
        let continuing_next = Self::continuing_member_set(next);
        if continuing_current != continuing_next {
            return Err(Error::InvalidConfig(format!(
                "continuing members must be identical across multisigs {} and {}",
                current.multisig_id, next.multisig_id
            )));
        }

        // Outgoing members cannot rejoin as incoming in the same transition
        let outgoing_keys =
            Self::member_key_set_by_role(current, FederationRole::Outgoing);
        let incoming_keys =
            Self::member_key_set_by_role(next, FederationRole::Incoming);
        if !outgoing_keys.is_disjoint(&incoming_keys) {
            return Err(Error::InvalidConfig(
                "outgoing members cannot rejoin as incoming in the same transition".to_string(),
            ));
        }

        Ok(())
    }

    fn continuing_member_set(
        multisig: &MultisigConfig,
    ) -> BTreeSet<(String, String)> {
        multisig
            .federation_member_public_key
            .iter()
            .filter(|member| member.role == FederationRole::Continuing)
            .map(|member| (member.key.clone(), member.socket_addr.clone()))
            .collect()
    }

    fn member_key_set_by_role(
        multisig: &MultisigConfig,
        role: FederationRole,
    ) -> BTreeSet<String> {
        multisig
            .federation_member_public_key
            .iter()
            .filter(|member| member.role == role)
            .map(|member| member.key.clone())
            .collect()
    }
}
impl FromStr for FederationTomlConfig {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut config: Self = toml::from_str(s).map_err(Error::ParseConfig)?;
        config.finalize()?;
        Ok(config)
    }
}

#[allow(dead_code)]
fn read_to_string(path: impl AsRef<Path> + Send) -> Result<String, Error> {
    let mut file = File::open(path).map_err(Error::OpenConfig)?;
    let meta = file.metadata().map_err(Error::ReadMeta)?;
    let mut contents =
        Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(0));
    file.read_to_end(&mut contents).map_err(Error::ReadConfig)?;
    String::from_utf8(contents).map_err(Error::ParseUtf8)
}

/// Writes random bytes to a filepath
#[allow(dead_code)]
pub(crate) fn write_data_to_file(
    path: impl AsRef<Path> + Send,
    data: &[u8],
) -> Result<(), Error> {
    let mut file = File::create(path).map_err(Error::OpenConfig)?;
    file.write_all(data).map_err(Error::ReadConfig)
}

/// Load the federation setup toml
pub fn load_federation_config_toml(
    path: &PathBuf,
) -> eyre::Result<FederationTomlConfig> {
    let _ = fs::metadata(path)?;
    let raw = fs::read_to_string(path)?;
    let genesis_toml_config = FederationTomlConfig::from_str(&raw)?;
    Ok(genesis_toml_config)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test constant matching LEGACY_MULTISIG_ID (0) from btcserverlib
    const TEST_LEGACY_MULTISIG_ID: u32 = 0;

    #[test]
    fn parses_multisig_format() {
        let toml = r#"
botanix-fee-recipient = "0x08b9676Eb48F02060BB6A98c1829d58Db5Bc2413"
minting-contract-bytecode = "0x00"
lst-fee-receiver = "0x1BAdd95a3c52baBecDF7ebb8BeE264005ddAa458"

[[multisig]]
multisig-id = 0
min-signers = 2
max-signers = 3

[[multisig.federation-member-public-key]]
key = "0268c9ee781a5f06434eb96ae54569b6354428d3e4f88822333d48f2b0bfd69ba4"
socket-addr = "172.22.1.1:30303"
role = "continuing"

[[multisig.federation-member-public-key]]
key = "0222202b198245e4e30019d2677b05f1b13ed673f961679aa219a379f1cec67913"
socket-addr = "172.22.2.1:30303"
role = "continuing"

[[multisig.federation-member-public-key]]
key = "03649234cdffe9a115d37a31a07c5f09e014539c90001ce8764a5815fd676d404a"
socket-addr = "172.22.3.1:30303"
role = "continuing"

[[multisig]]
multisig-id = 1
min-signers = 2
max-signers = 3

[[multisig.federation-member-public-key]]
key = "0268c9ee781a5f06434eb96ae54569b6354428d3e4f88822333d48f2b0bfd69ba4"
socket-addr = "172.22.1.1:30303"
role = "continuing"

[[multisig.federation-member-public-key]]
key = "0222202b198245e4e30019d2677b05f1b13ed673f961679aa219a379f1cec67913"
socket-addr = "172.22.2.1:30303"
role = "continuing"

[[multisig.federation-member-public-key]]
key = "03649234cdffe9a115d37a31a07c5f09e014539c90001ce8764a5815fd676d404a"
socket-addr = "172.22.3.1:30303"
role = "continuing"
"#;

        let config = FederationTomlConfig::from_str(toml)
            .expect("multisig config parses");
        assert_eq!(config.multisig.len(), 2);
        assert_eq!(config.multisig[0].multisig_id, 0);
        assert_eq!(config.multisig[1].multisig_id, 1);
        assert_eq!(config.multisig[0].min_signers, 2);
        assert_eq!(config.multisig[1].min_signers, 2);
        assert_eq!(config.multisig[0].max_signers, Some(3));
        assert_eq!(config.multisig[1].max_signers, Some(3));
        assert_eq!(config.multisig[0].federation_member_public_key.len(), 3);
        assert_eq!(config.multisig[1].federation_member_public_key.len(), 3);
    }

    fn member(key: &str, role: FederationRole) -> FedMemberPubKey {
        FedMemberPubKey {
            key: key.to_string(),
            socket_addr: "127.0.0.1:1".into(),
            role,
        }
    }

    #[test]
    fn dynafed_roles_pass_with_consistent_continuing() {
        let current = MultisigConfig {
            multisig_id: TEST_LEGACY_MULTISIG_ID,
            min_signers: 2,
            max_signers: Some(3),
            federation_member_public_key: vec![
                member("02aaa", FederationRole::Continuing),
                member("03bbb", FederationRole::Continuing),
                member("04ccc", FederationRole::Outgoing),
            ],
        };
        let next = MultisigConfig {
            multisig_id: 1,
            min_signers: 2,
            max_signers: Some(3),
            federation_member_public_key: vec![
                member("02aaa", FederationRole::Continuing),
                member("03bbb", FederationRole::Continuing),
                member("05ddd", FederationRole::Incoming),
            ],
        };

        assert!(FederationTomlConfig::validate_dynafed_roles(&current, &next)
            .is_ok());
    }

    #[test]
    fn dynafed_roles_fail_when_continuing_differs() {
        let current = MultisigConfig {
            multisig_id: TEST_LEGACY_MULTISIG_ID,
            min_signers: 2,
            max_signers: Some(2),
            federation_member_public_key: vec![
                member("02aaa", FederationRole::Continuing),
                member("03bbb", FederationRole::Continuing),
            ],
        };
        let next = MultisigConfig {
            multisig_id: 1,
            min_signers: 2,
            max_signers: Some(2),
            federation_member_public_key: vec![
                member("02aaa", FederationRole::Continuing),
                member("06eee", FederationRole::Continuing),
            ],
        };

        assert!(FederationTomlConfig::validate_dynafed_roles(&current, &next)
            .is_err());
    }

    #[test]
    fn dynafed_roles_fail_when_current_has_incoming() {
        let current = MultisigConfig {
            multisig_id: TEST_LEGACY_MULTISIG_ID,
            min_signers: 2,
            max_signers: Some(2),
            federation_member_public_key: vec![
                member("02aaa", FederationRole::Incoming),
                member("03bbb", FederationRole::Continuing),
            ],
        };
        let next = MultisigConfig {
            multisig_id: 1,
            min_signers: 2,
            max_signers: Some(2),
            federation_member_public_key: vec![
                member("02aaa", FederationRole::Continuing),
                member("03bbb", FederationRole::Continuing),
            ],
        };

        assert!(FederationTomlConfig::validate_dynafed_roles(&current, &next)
            .is_err());
    }

    #[test]
    fn dynafed_roles_fail_when_next_has_outgoing() {
        let current = MultisigConfig {
            multisig_id: TEST_LEGACY_MULTISIG_ID,
            min_signers: 2,
            max_signers: Some(2),
            federation_member_public_key: vec![
                member("02aaa", FederationRole::Continuing),
                member("03bbb", FederationRole::Outgoing),
            ],
        };
        let next = MultisigConfig {
            multisig_id: 1,
            min_signers: 2,
            max_signers: Some(2),
            federation_member_public_key: vec![
                member("02aaa", FederationRole::Continuing),
                member("03bbb", FederationRole::Outgoing),
            ],
        };

        assert!(FederationTomlConfig::validate_dynafed_roles(&current, &next)
            .is_err());
    }
}
