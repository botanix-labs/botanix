use displaydoc::Display as DisplayDoc;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Write},
    mem,
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
    InvalidConfig(String),
    /// Missing multisig configuration entries
    MissingMultisigs,
    /// Missing multisig version: {0}
    MissingMultisigVersion(String),
}

const PRIMARY_MULTISIG_VERSION: &str = "m1";

/// Federation member public key and socket address
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct FedMemberPubKey {
    /// The pub key of the member
    pub key: String,
    /// The socket address of the member
    pub socket_addr: String,
    /// The role of the member during federation transitions
    #[serde(default = "default_role")]
    pub role: FederationRole,
}

/// Member role (outgoing/continuing/incoming)
#[derive(
    Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "kebab-case")]
pub enum FederationRole {
    Incoming,
    Continuing,
    Outgoing,
}

fn default_role() -> FederationRole {
    FederationRole::Continuing
}

impl Default for FederationRole {
    fn default() -> Self {
        Self::Continuing
    }
}

/// Multisig definition and its members
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct MultisigConfig {
    /// Version identifier (m1, m2, ...)
    pub version: String,
    /// Threshold for this multisig
    #[serde(default)]
    pub min_signers: Option<u16>,
    /// Total number of signers for this multisig
    #[serde(default)]
    pub max_signers: Option<u16>,
    /// Members participating in this multisig
    #[serde(rename = "federation-member-public-key")]
    pub federation_member_public_key: Vec<FedMemberPubKey>,
}

impl MultisigConfig {
    #[allow(dead_code)]
    pub fn new(
        version: impl Into<String>,
        min_signers: u16,
        max_signers: u16,
        federation_member_public_key: Vec<FedMemberPubKey>,
    ) -> Self {
        Self {
            version: version.into(),
            min_signers: Some(min_signers),
            max_signers: Some(max_signers),
            federation_member_public_key,
        }
    }
}

/// Configuration for the genesis block (toml)
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct FederationTomlConfig {
    /// List of multisig definitions
    #[serde(default)]
    pub multisig: Vec<MultisigConfig>,
    /// Legacy federation entries
    #[serde(
        rename = "federation-member-public-key",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub legacy_federation_member_public_key: Vec<FedMemberPubKey>,
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
    pub const fn new(
        multisig: Vec<MultisigConfig>,
        botanix_fee_recipient: String,
        minting_contract_bytecode: String,
        lst_fee_receiver: String,
    ) -> Self {
        Self {
            multisig,
            legacy_federation_member_public_key: Vec::new(),
            botanix_fee_recipient,
            minting_contract_bytecode,
            lst_fee_receiver,
        }
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

    fn upgrade_legacy_entries(&mut self) -> Result<(), Error> {
        if self.multisig.is_empty() &&
            !self.legacy_federation_member_public_key.is_empty()
        {
            let members =
                mem::take(&mut self.legacy_federation_member_public_key);
            let max_signers = u16::try_from(members.len()).map_err(|_| {
                Error::InvalidConfig(
                    "too many federation members declared in legacy format"
                        .to_string(),
                )
            })?;
            self.multisig.push(MultisigConfig {
                version: PRIMARY_MULTISIG_VERSION.to_string(),
                min_signers: None,
                max_signers: Some(max_signers),
                federation_member_public_key: members,
            });
        }
        Ok(())
    }

    /// Extracts federation public keys and socket addresses from the config
    pub fn get_federation_pks_from_path(
        &self,
    ) -> Result<Vec<(secp256k1::PublicKey, SocketAddr)>, Error> {
        self.get_federation_pks_internal(None)
    }

    /// Extracts federation public keys and socket addresses for a specific
    /// multisig version.
    pub fn get_federation_pks_for_version(
        &self,
        version: &str,
    ) -> Result<Vec<(secp256k1::PublicKey, SocketAddr)>, Error> {
        self.get_federation_pks_internal(Some(version))
    }

    fn get_federation_pks_internal(
        &self,
        version: Option<&str>,
    ) -> Result<Vec<(secp256k1::PublicKey, SocketAddr)>, Error> {
        let multisig = self.select_multisig(version)?;
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
        version: Option<&str>,
    ) -> Result<&MultisigConfig, Error> {
        if self.multisig.is_empty() {
            return Err(Error::MissingMultisigs);
        }

        let selected = if let Some(version) = version {
            self.multisig.iter().find(|m| m.version == version).ok_or_else(
                || Error::MissingMultisigVersion(version.to_string()),
            )?
        } else {
            self.multisig
                .iter()
                .find(|m| m.version == PRIMARY_MULTISIG_VERSION)
                .or_else(|| self.multisig.first())
                .ok_or(Error::MissingMultisigs)?
        };

        Ok(selected)
    }

    pub fn get_multisig_by_version(
        &self,
        version: &str,
    ) -> Option<&MultisigConfig> {
        self.multisig.iter().find(|m| m.version == version)
    }

    fn validate(&self) -> Result<(), Error> {
        if self.multisig.is_empty() {
            return Err(Error::MissingMultisigs);
        }

        for multisig in &self.multisig {
            if let Some(max_signers) = multisig.max_signers {
                if multisig.federation_member_public_key.len() !=
                    max_signers as usize
                {
                    return Err(Error::InvalidConfig(format!(
                        "multisig '{}' max-signers ({}) must equal listed members ({})",
                        multisig.version,
                        max_signers,
                        multisig.federation_member_public_key.len()
                    )));
                }
            }

            if let (Some(min_signers), Some(max_signers)) =
                (multisig.min_signers, multisig.max_signers)
            {
                if !(1..=max_signers).contains(&min_signers) {
                    return Err(Error::InvalidConfig(format!(
                        "multisig '{}' min-signers ({}) must be within 1..=max-signers ({})",
                        multisig.version, min_signers, max_signers
                    )));
                }
            }
        }

        if let (Some(m1), Some(m2)) = (
            self.get_multisig_by_version("m1"),
            self.get_multisig_by_version("m2"),
        ) {
            Self::validate_dynafed_roles(m1, m2)?;
        }

        Ok(())
    }

    fn validate_dynafed_roles(
        m1: &MultisigConfig,
        m2: &MultisigConfig,
    ) -> Result<(), Error> {
        if m1
            .federation_member_public_key
            .iter()
            .any(|member| member.role == FederationRole::Incoming)
        {
            return Err(Error::InvalidConfig(
                "incoming members must not be defined in multisig 'm1'"
                    .to_string(),
            ));
        }

        if m2
            .federation_member_public_key
            .iter()
            .any(|member| member.role == FederationRole::Outgoing)
        {
            return Err(Error::InvalidConfig(
                "outgoing members must not be defined in multisig 'm2'"
                    .to_string(),
            ));
        }

        let continuing_m1 = Self::continuing_member_set(m1);
        let continuing_m2 = Self::continuing_member_set(m2);
        if continuing_m1 != continuing_m2 {
            return Err(Error::InvalidConfig(
                "continuing members must be identical across 'm1' and 'm2' multisigs".to_string(),
            ));
        }

        let incoming_m2 =
            Self::member_key_set_by_role(m2, FederationRole::Incoming);
        let outgoing_m1 =
            Self::member_key_set_by_role(m1, FederationRole::Outgoing);

        if incoming_m2.iter().any(|key| Self::member_exists_with_key(m1, key)) {
            return Err(Error::InvalidConfig(
                "incoming members must not appear in multisig 'm1'".to_string(),
            ));
        }

        if outgoing_m1.iter().any(|key| Self::member_exists_with_key(m2, key)) {
            return Err(Error::InvalidConfig(
                "outgoing members must not appear in multisig 'm2'".to_string(),
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

    fn member_exists_with_key(multisig: &MultisigConfig, key: &str) -> bool {
        multisig
            .federation_member_public_key
            .iter()
            .any(|member| member.key == key)
    }
}
impl FromStr for FederationTomlConfig {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut config: Self = toml::from_str(s).map_err(Error::ParseConfig)?;
        config.upgrade_legacy_entries()?;
        config.validate()?;
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

    #[test]
    fn parses_legacy_federation_format() {
        let legacy = r#"
botanix-fee-recipient = "0x0000000000000000000000000000000000000000"
minting-contract-bytecode = ""
lst-fee-receiver = "0x0000000000000000000000000000000000000000"

[[federation-member-public-key]]
key = "029dfea7f03f62cace4b1efb5acd3c3185850a7164cc1754667fdca192e4837ad9"
socket-addr = "127.0.0.1:30303"

[[federation-member-public-key]]
key = "038df7fcb0e1cdd68741ca85184e046a42c914e0c3ffcb2464d46be3d8b4a5b140"
socket-addr = "127.0.0.1:30304"
"#;

        let config = FederationTomlConfig::from_str(legacy)
            .expect("legacy config parses");
        assert!(config.legacy_federation_member_public_key.is_empty());
        assert_eq!(config.multisig.len(), 1);
        let multisig = &config.multisig[0];
        assert_eq!(multisig.version, PRIMARY_MULTISIG_VERSION);
        assert_eq!(multisig.max_signers, Some(2));
        assert!(multisig.min_signers.is_none());
        assert_eq!(multisig.federation_member_public_key.len(), 2);
        assert!(multisig
            .federation_member_public_key
            .iter()
            .all(|member| member.role == FederationRole::Continuing));
    }

    #[test]
    fn parses_multisig_format() {
        let toml = r#"
botanix-fee-recipient = "0x08b9676Eb48F02060BB6A98c1829d58Db5Bc2413"
minting-contract-bytecode = "0x00"
lst-fee-receiver = "0x1BAdd95a3c52baBecDF7ebb8BeE264005ddAa458"

[[multisig]]
version = "m1"
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
        assert_eq!(config.multisig.len(), 1);
        let multisig = &config.multisig[0];
        assert_eq!(multisig.version, "m1");
        assert_eq!(multisig.min_signers, Some(2));
        assert_eq!(multisig.max_signers, Some(3));
        assert_eq!(multisig.federation_member_public_key.len(), 3);
    }
}
