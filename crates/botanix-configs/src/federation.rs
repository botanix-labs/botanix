use bitcoin::secp256k1::hashes::{sha256, Hash};
use botanix_types::MultisigId;
use displaydoc::Display as DisplayDoc;
use frost_secp256k1_tr as frost;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    net::{SocketAddr, ToSocketAddrs},
    path::Path,
    str::FromStr,
};
use thiserror::Error;

/// Error type for genesis config
#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Open config file: {0}
    OpenConfig(std::io::Error),
    /// Failed to parse config: {0}
    ParseConfig(toml::de::Error),
    /// Failed to serialize parse config: {0}
    ParseSerializeConfig(toml::ser::Error),
    /// Failed to parse config as utf-8: {0}
    ParseUtf8(std::string::FromUtf8Error),
    /// Failed to read config file: {0}
    ReadConfig(std::io::Error),
    /// Failed to read config metadata: {0}
    ReadMeta(std::io::Error),
    /// Failed to read public key: {0}
    InvalidPublicKeyFormat(#[from] secp256k1::Error),
    /// Failed to read config socket address: {0}
    InvalidSocketAddress(#[from] std::net::AddrParseError),
    /// Failed to resolve socket address via DNS lookup: {0}
    SocketAddressResolution(std::io::Error),
    /// Invalid federation configuration: {0}
    InvalidConfig(String),
    /// Missing multisig configuration entry
    MissingMultisig,
    /// Bad coordinator index
    BadCoordinatorIndex,
    /// TODO
    LocalIdentifierMissing,
}
/// Federation member public key and socket address
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct FedMemberPubKey {
    /// The pub key of the member
    pub key: String,
    /// The socket address of the member
    pub socket_addr: String,
}

impl FedMemberPubKey {
    /// Parses the socket address directly, falling back to DNS resolution.
    pub fn resolve_addr(&self) -> Result<SocketAddr, Error> {
        self.socket_addr.parse().or_else(|_| {
            self.socket_addr
                .to_socket_addrs()
                .map_err(Error::SocketAddressResolution)?
                .next()
                .ok_or_else(|| {
                    Error::SocketAddressResolution(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!(
                            "No addresses resolved for {}",
                            self.socket_addr
                        ),
                    ))
                })
        })
    }
    pub fn public_key(&self) -> Result<secp256k1::PublicKey, Error> {
        secp256k1::PublicKey::from_str(&self.key).map_err(Into::into)
    }
}

/// Multisig definition and its members
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct MultisigTomlConfig {
    /// Identifier for this multisig
    pub multisig_id: MultisigId,
    /// Threshold for this multisig
    pub min_signers: u16,
    /// The coordinator index in the `members` list.
    #[serde(default)]
    pub coordinator: u16,
    /// Members participating in this multisig
    #[serde(rename = "member")]
    pub members: Vec<FedMemberPubKey>,
}

impl MultisigTomlConfig {
    pub fn new(
        multisig_id: MultisigId,
        min_signers: u16,
        coordinator: Option<u16>,
        members: Vec<FedMemberPubKey>,
    ) -> Self {
        Self {
            multisig_id,
            min_signers,
            coordinator: coordinator.unwrap_or_default(),
            members,
        }
    }

    pub fn get_federation_pub_keys(
        &self,
    ) -> Result<Vec<secp256k1::PublicKey>, Error> {
        self.members.iter().map(|member| member.public_key()).collect()
    }

    /// Extracts federation public keys and socket addresses for a specific
    /// multisig id.
    pub fn get_federation_addrs(
        &self,
    ) -> Result<Vec<(secp256k1::PublicKey, SocketAddr)>, Error> {
        if self.members.is_empty() {
            return Err(Error::MissingMultisig);
        }

        self.members
            .iter()
            .map(|member| {
                let public_key = member.public_key()?;
                let addr = member.resolve_addr()?;
                Ok((public_key, addr))
            })
            .collect()
    }

    /// Returns the coordinator public key.
    pub fn get_coordinator_pub_key(
        &self,
    ) -> Result<secp256k1::PublicKey, Error> {
        self.members
            .get(self.coordinator as usize)
            .ok_or(Error::BadCoordinatorIndex)
            .map(|m| m.public_key())?
    }

    /// Convert the config to a string
    pub fn to_string(&self) -> Result<String, Error> {
        toml::to_string(self).map_err(Error::ParseSerializeConfig)
    }

    /// Compute the SHA-256 checksum of the serialized TOML config.
    pub fn checksum(&self) -> Result<[u8; 32], Error> {
        let s = self.to_string()?;
        let h = sha256::Hash::hash(s.as_bytes()).to_byte_array();
        Ok(h)
    }
}

/// Configuration for the genesis block (toml)
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct FederationTomlConfig {
    /// botanix fee recipient
    pub botanix_fee_recipient: String,
    /// The precompiled Minting contract bytecode
    pub minting_contract_bytecode: String,
    /// LST fee receiver
    pub lst_fee_receiver: String,
    /// List of multisig definitions
    #[serde(default, rename = "multisig")]
    pub multisigs: Vec<MultisigTomlConfig>,
}

impl FederationTomlConfig {
    pub fn new_from_path(path: impl AsRef<Path> + Send) -> Result<Self, Error> {
        read_to_string(path)?.parse()
    }

    /// Create a new genesis config
    pub fn new(
        botanix_fee_recipient: String,
        minting_contract_bytecode: String,
        lst_fee_receiver: String,
        multisigs: Vec<MultisigTomlConfig>,
    ) -> Result<Self, Error> {
        Ok(Self {
            botanix_fee_recipient,
            minting_contract_bytecode,
            lst_fee_receiver,
            multisigs,
        })
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

    pub fn get_federation_addrs(
        &self,
    ) -> Result<Vec<(secp256k1::PublicKey, SocketAddr)>, Error> {
        if self.multisigs.is_empty() {
            return Err(Error::MissingMultisig);
        }

        self.multisigs.iter().try_fold(Vec::new(), |mut acc, m| {
            acc.extend(m.get_federation_addrs()?);
            Ok(acc)
        })
    }

    /// Extracts federation public keys and socket addresses for a specific
    /// multisig id.
    pub fn get_federation_addr_by_multisig(
        &self,
        multisig_id: &MultisigId,
    ) -> Result<Vec<(secp256k1::PublicKey, SocketAddr)>, Error> {
        if self.multisigs.is_empty() {
            return Err(Error::MissingMultisig);
        }

        self.multisigs
            .iter()
            .find(|m| &m.multisig_id == multisig_id)
            .ok_or(Error::MissingMultisig)?
            .get_federation_addrs()
    }

    pub fn get_config_by_multisig_id(
        &self,
        multisig_id: MultisigId,
    ) -> Result<&MultisigTomlConfig, Error> {
        self.multisigs
            .iter()
            .find(|m| m.multisig_id == multisig_id)
            .ok_or(Error::MissingMultisig)
    }
}

impl FromStr for FederationTomlConfig {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        toml::from_str(s).map_err(Error::ParseConfig).map_err(Into::into)
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
pub fn load_federation_config_toml<P: AsRef<Path>>(
    path: P,
) -> eyre::Result<FederationTomlConfig> {
    let raw = fs::read_to_string(path)?;
    let genesis_toml_config = FederationTomlConfig::from_str(&raw)?;
    Ok(genesis_toml_config)
}

/// Configuration for a single federation multisig, representing one epoch in
/// the dynafed lifecycle.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MultisigConfig {
    /// Identifier for this multisig.
    pub multisig_id: MultisigId,
    /// Minimum number of signers required to produce a valid signature.
    pub min_signers: u16,
    /// Total number of signers for this multisig (defaults to member count).
    pub max_signers: u16,
    /// The coordinator Id.
    pub coordinator: frost::Identifier,
    /// The local identifier in the authority list, if present.
    pub local_identifier: Option<frost::Identifier>,
    /// The Frost identifier and their corresponding public keys of all participants in this multisig.
    pub authorities: BTreeMap<frost::Identifier, secp256k1::PublicKey>,
}

impl MultisigConfig {
    pub fn is_local_identifier_present(&self) -> bool {
        let Some(local_id) = self.local_identifier else {
            return false;
        };

        self.authorities.contains_key(&local_id)
    }
    pub fn from_toml_config(
        m: &MultisigTomlConfig,
        local_pk: Option<&secp256k1::PublicKey>,
    ) -> Result<Self, Error> {
        let multisig_id = m.multisig_id;
        let min_signers = m.min_signers;
        let max_signers = m.members.len() as u16;

        // Deserialize the public keys of the authorities and compute their
        // corresponding Frost Ids.
        let authorities: Vec<(frost::Identifier, secp256k1::PublicKey)> = m
            .members
            .iter()
            .map(|m| {
                let pubkey = secp256k1::PublicKey::from_str(&m.key)?;
                let frost_id = frost::Identifier::derive(&pubkey.serialize())
                    .expect("frost id must be valid");

                Ok((frost_id, pubkey))
            })
            .collect::<Result<_, Error>>()?;

        // Retrieve the coordinator Frost Id.
        let coordinator = authorities
            .get(m.coordinator as usize)
            .ok_or(Error::BadCoordinatorIndex)?
            .0;

        // Compute the local Frost Id, if set.
        let local_identifier = local_pk.map(|pk| {
            frost::Identifier::derive(&pk.serialize())
                .expect("frost id must be valid")
        });

        let authorities: BTreeMap<_, _> = authorities.into_iter().collect();

        Ok(MultisigConfig {
            multisig_id,
            min_signers,
            max_signers,
            coordinator,
            local_identifier,
            authorities,
        })
    }
}

/// Multisig configuration for a node that is a participant in the multisig.
/// Guarantees that `local_identifier` is present, meaning the local node's
/// public key was found among the authorities.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthorityMultisigConfig {
    /// Identifier for this multisig.
    pub multisig_id: MultisigId,
    /// Minimum number of signers required to produce a valid signature.
    pub min_signers: u16,
    /// Total number of signers for this multisig (defaults to member count).
    pub max_signers: u16,
    /// The coordinator Id.
    pub coordinator: frost::Identifier,
    /// The local identifier in the authority list.
    pub local_identifier: frost::Identifier,
    /// The Frost identifier and their corresponding public keys of all participants in this multisig.
    pub authorities: BTreeMap<frost::Identifier, secp256k1::PublicKey>,
}

impl AuthorityMultisigConfig {
    pub fn from_toml_config(
        m: &MultisigTomlConfig,
        local_pk: Option<&secp256k1::PublicKey>,
    ) -> Result<Self, Error> {
        let config = MultisigConfig::from_toml_config(m, local_pk)?;
        Self::try_from(config)
    }
    /// Compute the SHA-256 checksum of the shared multisig config.
    ///
    /// Only covers fields common to all participants (excludes
    /// `local_identifier`) so every node produces the same digest.
    pub fn checksum(&self) -> [u8; 32] {
        // TODO (lamafab): there's probably a more elegant way to do this(?)
        let shared = MultisigConfig {
            multisig_id: self.multisig_id,
            min_signers: self.min_signers,
            max_signers: self.max_signers,
            coordinator: self.coordinator,
            local_identifier: None,
            authorities: self.authorities.clone(),
        };

        let s =
            toml::to_string(&shared).expect("toml serialization must be valid");

        sha256::Hash::hash(s.as_bytes()).to_byte_array()
    }
}

impl TryFrom<MultisigConfig> for AuthorityMultisigConfig {
    type Error = Error;

    fn try_from(m: MultisigConfig) -> Result<Self, Self::Error> {
        let local_identifier =
            m.local_identifier.ok_or_else(|| Error::LocalIdentifierMissing)?;

        Ok(AuthorityMultisigConfig {
            multisig_id: m.multisig_id,
            min_signers: m.min_signers,
            max_signers: m.max_signers,
            coordinator: m.coordinator,
            local_identifier,
            authorities: m.authorities,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multisig_format() {
        let toml = r#"
botanix-fee-recipient = "0x08b9676Eb48F02060BB6A98c1829d58Db5Bc2413"
minting-contract-bytecode = "0x00"
lst-fee-receiver = "0x1BAdd95a3c52baBecDF7ebb8BeE264005ddAa458"

[[multisig]]
multisig-id = 0
min-signers = 2
coordinator = 1

[[multisig.member]]
key = "0268c9ee781a5f06434eb96ae54569b6354428d3e4f88822333d48f2b0bfd69ba4"
socket-addr = "172.22.1.1:30303"

[[multisig.member]]
key = "0222202b198245e4e30019d2677b05f1b13ed673f961679aa219a379f1cec67913"
socket-addr = "172.22.2.1:30303"

[[multisig.member]]
key = "03649234cdffe9a115d37a31a07c5f09e014539c90001ce8764a5815fd676d404a"
socket-addr = "172.22.3.1:30303"

[[multisig]]
multisig-id = 1
min-signers = 2

[[multisig.member]]
key = "0268c9ee781a5f06434eb96ae54569b6354428d3e4f88822333d48f2b0bfd69ba4"
socket-addr = "172.22.1.1:30303"

[[multisig.member]]
key = "0222202b198245e4e30019d2677b05f1b13ed673f961679aa219a379f1cec67913"
socket-addr = "172.22.2.1:30303"

[[multisig.member]]
key = "03649234cdffe9a115d37a31a07c5f09e014539c90001ce8764a5815fd676d404a"
socket-addr = "172.22.3.1:30303"
"#;

        let config = FederationTomlConfig::from_str(toml)
            .expect("multisig config parses");

        assert_eq!(config.multisigs.len(), 2);
        let m1 = &config.multisigs[0];
        let m2 = &config.multisigs[1];

        assert_eq!(m1.multisig_id, MultisigId::new(0));
        assert_eq!(m1.min_signers, 2);
        assert_eq!(m1.coordinator, 1);
        assert_eq!(m1.members.len(), 3);
        assert_eq!(m1.members[0].key, "0268c9ee781a5f06434eb96ae54569b6354428d3e4f88822333d48f2b0bfd69ba4");
        assert_eq!(m1.members[0].socket_addr, "172.22.1.1:30303");
        assert_eq!(m1.members[1].key, "0222202b198245e4e30019d2677b05f1b13ed673f961679aa219a379f1cec67913");
        assert_eq!(m1.members[1].socket_addr, "172.22.2.1:30303");
        assert_eq!(m1.members[2].key, "03649234cdffe9a115d37a31a07c5f09e014539c90001ce8764a5815fd676d404a");
        assert_eq!(m1.members[2].socket_addr, "172.22.3.1:30303");

        assert_eq!(m2.multisig_id, MultisigId::new(1));
        assert_eq!(m2.min_signers, 2);
        assert_eq!(m2.coordinator, 0);
        assert_eq!(m2.members.len(), 3);
        assert_eq!(m2.members[0].key, "0268c9ee781a5f06434eb96ae54569b6354428d3e4f88822333d48f2b0bfd69ba4");
        assert_eq!(m2.members[0].socket_addr, "172.22.1.1:30303");
        assert_eq!(m2.members[1].key, "0222202b198245e4e30019d2677b05f1b13ed673f961679aa219a379f1cec67913");
        assert_eq!(m2.members[1].socket_addr, "172.22.2.1:30303");
        assert_eq!(m2.members[2].key, "03649234cdffe9a115d37a31a07c5f09e014539c90001ce8764a5815fd676d404a");
        assert_eq!(m2.members[2].socket_addr, "172.22.3.1:30303");
    }
}
