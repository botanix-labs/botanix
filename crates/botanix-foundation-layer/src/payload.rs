//! Consensus payload types for the Foundation layer.
//!
//! This module defines [`ConsensusPayload`], the set of data types that can be
//! proposed and agreed upon by validators. The inner values are extracted and
//! passed to the Foundation layer for both validation and finalization.
//! Payloads are CBOR-encoded for transmission.
use crate::codec::Codec;
use botanix_tem::foundation::{
    bitcoin::{self, BlockHash, Txid},
    proof::FoundationStateRoot,
    ProposalEntry,
};
use serde::{Deserialize, Serialize};

/// Payload types that can be proposed and agreed upon through consensus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusPayload {
    /// A proposal to process a peg-out request.
    PegoutProposal {
        proposal: ProposalEntry,
        /// If set, this proposal replaces a previous one where its first UTXO
        /// must be reused.
        replacing: Option<Txid>,
    },
    /// A Bitcoin block header to be tracked by the Foundation layer.
    BitcoinHeader {
        header: bitcoin::block::Header,
        height: u64,
        //
    },
    /// A Bitcoin transaction with its inclusion proof.
    BitcoinTransaction {
        block_hash: BlockHash,
        tx: bitcoin::Transaction,
        #[serde(with = "partial_merkle_tree_serde")]
        proof: bitcoin::merkle_tree::PartialMerkleTree,
    },
    /// A commitment to the Foundation layer state.
    FoundationRoot {
        /// The root that must be reproducible by all participants.
        root: FoundationStateRoot,
    },
}

impl ConsensusPayload {
    /// Encodes the value to a new `Vec<u8>`.
    pub fn encode(&self) -> Vec<u8> {
        Codec::encode(self)
    }
    /// Encodes the value to a writer, returning the number of bytes written.
    pub fn encode_to<W: std::io::Write>(
        &self,
        writer: W,
    ) -> Result<usize, ciborium::ser::Error<std::io::Error>> {
        Codec::encode_to(self, writer)
    }
    /// Decodes a value from a reader, returning the value and bytes consumed.
    pub fn decode<R: std::io::Read>(
        reader: R,
    ) -> Result<(Self, usize), ciborium::de::Error<std::io::Error>> {
        Codec::decode(reader)
    }
}

mod partial_merkle_tree_serde {
    use botanix_tem::foundation::bitcoin::{
        consensus::{Decodable, Encodable},
        merkle_tree::PartialMerkleTree,
    };
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        tree: &PartialMerkleTree,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut buf = Vec::new();
        tree.consensus_encode(&mut buf).map_err(serde::ser::Error::custom)?;
        serializer.serialize_bytes(&buf)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<PartialMerkleTree, D::Error> {
        let bytes = <Vec<u8>>::deserialize(deserializer)?;
        PartialMerkleTree::consensus_decode(&mut bytes.as_slice())
            .map_err(serde::de::Error::custom)
    }
}
