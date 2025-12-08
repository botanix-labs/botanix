/// TODO (lamafab): This code was copied 1-to-1 from
/// `crates/node/core/src/args/federation_args.rs`. We should maybe consider
/// unifying this in one place.
use bitcoin::secp256k1;
use displaydoc::Display as DisplayDoc;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{Read, Write},
    mem,
    net::{SocketAddr, ToSocketAddrs},
    path::Path,
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
    /// Invalid number of multisig entries: expected 1 or 2, found {0}
    InvalidMultisigCount(usize),
    /// Duplicate multisig id: {0}
    DuplicateMultisigId(u32),
    /// Missing multisig id: {0}
    MissingMultisigId(u32),
}

const LEGACY_MULTISIG_ID: u32 = 0;

/// Role of a federation member when transitioning multisigs.
#[derive(
    Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "kebab-case")]
pub enum FederationRole {
    Incoming,
    Continuing,
    Outgoing,
}

impl Default for FederationRole {
    fn default() -> Self {
        Self::Continuing
    }
}

/// Federation member public key and socket address
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct FedMemberPubKey {
    /// The pub key of the member
    pub key: String,
    /// The socket address of the member
    pub socket_addr: String,
    /// The role of the federation member
    #[serde(default)]
    pub role: FederationRole,
}

/// A multisig definition together with its participants.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct MultisigConfig {
    /// Identifier of this multisig (0 = pre-dynafed, 1 = next, ...)
    pub multisig_id: u32,
    /// Threshold for this multisig
    #[serde(default)]
    pub min_signers: Option<u16>,
    /// Total number of signers for this multisig
    #[serde(default)]
    pub max_signers: Option<u16>,
    /// Federation members taking part in this multisig
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
    /// All multisig definitions. Order is ignored, sorted by multisig_id internally
    #[serde(default)]
    pub multisig: Vec<MultisigConfig>,
    /// Legacy federation entries (pre-multisig format)
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
    pub fn new(
        multisig: Vec<MultisigConfig>,
        botanix_fee_recipient: String,
        minting_contract_bytecode: String,
        lst_fee_receiver: String,
    ) -> Result<Self, Error> {
        let mut config = Self {
            multisig,
            legacy_federation_member_public_key: Vec::new(),
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
        self.upgrade_legacy_entries()?;
        self.validate()?;
        Ok(())
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
                multisig_id: LEGACY_MULTISIG_ID,
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

    /// Extracts federation public keys and socket addresses for a given
    /// multisig id.
    pub fn get_federation_pks_for_id(
        &self,
        multisig_id: u32,
    ) -> Result<Vec<(secp256k1::PublicKey, SocketAddr)>, Error> {
        self.get_federation_pks_internal(Some(multisig_id))
    }

    fn get_federation_pks_internal(
        &self,
        multisig_id: Option<u32>,
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
        multisig_id: Option<u32>,
    ) -> Result<&MultisigConfig, Error> {
        if self.multisig.is_empty() {
            return Err(Error::MissingMultisigs);
        }

        let selected = if let Some(multisig_id) = multisig_id {
            self.multisig
                .iter()
                .find(|m| m.multisig_id == multisig_id)
                .ok_or(Error::MissingMultisigId(multisig_id))?
        } else {
            self.multisig
                .iter()
                .find(|m| m.multisig_id == LEGACY_MULTISIG_ID)
                .or_else(|| self.multisig.first())
                .ok_or(Error::MissingMultisigs)?
        };

        Ok(selected)
    }

    pub fn get_multisig_by_id(
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
            return Err(Error::InvalidMultisigCount(self.multisig.len()));
        }

        let mut seen_ids = BTreeSet::new();
        for multisig in &self.multisig {
            if !seen_ids.insert(multisig.multisig_id) {
                return Err(Error::DuplicateMultisigId(multisig.multisig_id));
            }

            if let Some(max_signers) = multisig.max_signers {
                if multisig.federation_member_public_key.len() !=
                    max_signers as usize
                {
                    return Err(Error::InvalidConfig(format!(
                        "multisig {} max-signers ({}) must equal listed members ({})",
                        multisig.multisig_id,
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
                        "multisig {} min-signers ({}) must be within 1..=max-signers ({})",
                        multisig.multisig_id, min_signers, max_signers
                    )));
                }
            }
        }

        if self.multisig.len() == 2 {
            // Always compare the two multisig IDs present, current versus incoming.
            let mut sorted_multisigs =
                self.multisig.iter().collect::<Vec<&MultisigConfig>>();
            sorted_multisigs.sort_by_key(|m| m.multisig_id);
            let current = sorted_multisigs[0];
            let next = sorted_multisigs[1];
            Self::validate_dynafed_roles(current, next)?;
        }

        Ok(())
    }

    fn validate_dynafed_roles(
        current: &MultisigConfig,
        next: &MultisigConfig,
    ) -> Result<(), Error> {
        if current
            .federation_member_public_key
            .iter()
            .any(|member| member.role == FederationRole::Incoming)
        {
            return Err(Error::InvalidConfig(
                format!(
                    "incoming members must not be defined in multisig {}",
                    current.multisig_id
                ),
            ));
        }

        if next
            .federation_member_public_key
            .iter()
            .any(|member| member.role == FederationRole::Outgoing)
        {
            return Err(Error::InvalidConfig(
                format!(
                    "outgoing members must not be defined in multisig {}",
                    next.multisig_id
                ),
            ));
        }

        let continuing_current = Self::continuing_member_set(current);
        let continuing_next = Self::continuing_member_set(next);
        if continuing_current != continuing_next {
            return Err(Error::InvalidConfig(
                format!(
                    "continuing members must be identical across multisigs {} and {}",
                    current.multisig_id, next.multisig_id
                ),
            ));
        }

        let incoming_next =
            Self::member_key_set_by_role(next, FederationRole::Incoming);
        let outgoing_current =
            Self::member_key_set_by_role(current, FederationRole::Outgoing);

        if incoming_next
            .iter()
            .any(|key| Self::member_exists_with_key(current, key))
        {
            return Err(Error::InvalidConfig(
                format!(
                    "incoming members must not appear in multisig {}",
                    current.multisig_id
                ),
            ));
        }

        if outgoing_current
            .iter()
            .any(|key| Self::member_exists_with_key(next, key))
        {
            return Err(Error::InvalidConfig(
                format!(
                    "outgoing members must not appear in multisig {}",
                    next.multisig_id
                ),
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
