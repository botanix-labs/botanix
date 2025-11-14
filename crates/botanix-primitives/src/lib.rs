#![allow(clippy::owned_cow)]
//! Primitive types for Botanix blockchain.

use alloy_consensus::{BlobTransactionSidecar, Header};
use alloy_primitives::B256;
use alloy_rlp::{Encodable, RlpDecodable, RlpEncodable};
use reth_ethereum_primitives::{BlockBody, Receipt};
use reth_primitives::{NodePrimitives, TransactionSigned};
use reth_primitives_traits::{
    Block, BlockBody as BlockBodyTrait, InMemorySize,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Primitive types for Botanix.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct BotanixPrimitives;

impl NodePrimitives for BotanixPrimitives {
    type Block = BotanixBlock;
    type BlockHeader = Header;
    type BlockBody = BotanixBlockBody;
    type SignedTx = TransactionSigned;
    type Receipt = Receipt;
}

/// Botanix representation of a EIP-4844 sidecar.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    RlpEncodable,
    RlpDecodable,
    Serialize,
    Deserialize,
)]
pub struct BotanixBlobTransactionSidecar {
    pub inner: BlobTransactionSidecar,
    pub block_number: u64,
    pub block_hash: B256,
    pub tx_index: u64,
    pub tx_hash: B256,
}

/// Block body for Botanix. It is equivalent to Ethereum [`BlockBody`] but additionally stores sidecars
/// for blob transactions.
#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_more::Deref,
    derive_more::DerefMut,
)]
pub struct BotanixBlockBody {
    #[serde(flatten)]
    #[deref]
    #[deref_mut]
    pub inner: BlockBody,
    pub sidecars: Option<Vec<BotanixBlobTransactionSidecar>>,
}

impl InMemorySize for BotanixBlockBody {
    fn size(&self) -> usize {
        self.inner.size()
            + self.sidecars.as_ref().map_or(0, |s| {
                s.capacity()
                    * core::mem::size_of::<BotanixBlobTransactionSidecar>()
            })
    }
}

impl BlockBodyTrait for BotanixBlockBody {
    type Transaction = TransactionSigned;
    type OmmerHeader = Header;

    fn transactions(&self) -> &[Self::Transaction] {
        BlockBodyTrait::transactions(&self.inner)
    }

    fn into_ethereum_body(self) -> BlockBody {
        self.inner
    }

    fn into_transactions(self) -> Vec<Self::Transaction> {
        self.inner.into_transactions()
    }

    fn withdrawals(&self) -> Option<&alloy_rpc_types::Withdrawals> {
        self.inner.withdrawals()
    }

    fn ommers(&self) -> Option<&[Self::OmmerHeader]> {
        self.inner.ommers()
    }
}

/// Block for Botanix
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotanixBlock {
    /// The block header.
    pub header: Header,
    /// The block body.
    pub body: BotanixBlockBody,
}

impl InMemorySize for BotanixBlock {
    fn size(&self) -> usize {
        self.header.size() + self.body.size()
    }
}

impl Block for BotanixBlock {
    type Header = Header;
    type Body = BotanixBlockBody;

    fn new(header: Self::Header, body: Self::Body) -> Self {
        Self { header, body }
    }

    fn header(&self) -> &Self::Header {
        &self.header
    }

    fn body(&self) -> &Self::Body {
        &self.body
    }

    fn split(self) -> (Self::Header, Self::Body) {
        (self.header, self.body)
    }

    fn rlp_length(header: &Self::Header, body: &Self::Body) -> usize {
        rlp::BlockHelper {
            header: Cow::Borrowed(header),
            transactions: Cow::Borrowed(&body.inner.transactions),
            ommers: Cow::Borrowed(&body.inner.ommers),
            withdrawals: body.inner.withdrawals.as_ref().map(Cow::Borrowed),
            sidecars: body.sidecars.as_ref().map(Cow::Borrowed),
        }
        .length()
    }
}

mod rlp {
    use super::*;
    use alloy_eips::eip4895::Withdrawals;
    use alloy_rlp::Decodable;

    #[derive(RlpEncodable, RlpDecodable)]
    #[rlp(trailing)]
    struct BlockBodyHelper<'a> {
        transactions: Cow<'a, Vec<TransactionSigned>>,
        ommers: Cow<'a, Vec<Header>>,
        withdrawals: Option<Cow<'a, Withdrawals>>,
        sidecars: Option<Cow<'a, Vec<BotanixBlobTransactionSidecar>>>,
    }

    #[derive(RlpEncodable, RlpDecodable)]
    #[rlp(trailing)]
    pub(crate) struct BlockHelper<'a> {
        pub(crate) header: Cow<'a, Header>,
        pub(crate) transactions: Cow<'a, Vec<TransactionSigned>>,
        pub(crate) ommers: Cow<'a, Vec<Header>>,
        pub(crate) withdrawals: Option<Cow<'a, Withdrawals>>,
        pub(crate) sidecars:
            Option<Cow<'a, Vec<BotanixBlobTransactionSidecar>>>,
    }

    impl<'a> From<&'a BotanixBlockBody> for BlockBodyHelper<'a> {
        fn from(value: &'a BotanixBlockBody) -> Self {
            let BotanixBlockBody {
                inner:
                    BlockBody {
                        transactions,
                        ommers,
                        withdrawals,
                    },
                sidecars,
            } = value;

            Self {
                transactions: Cow::Borrowed(transactions),
                ommers: Cow::Borrowed(ommers),
                withdrawals: withdrawals.as_ref().map(Cow::Borrowed),
                sidecars: sidecars.as_ref().map(Cow::Borrowed),
            }
        }
    }

    impl<'a> From<&'a BotanixBlock> for BlockHelper<'a> {
        fn from(value: &'a BotanixBlock) -> Self {
            let BotanixBlock {
                header,
                body:
                    BotanixBlockBody {
                        inner:
                            BlockBody {
                                transactions,
                                ommers,
                                withdrawals,
                            },
                        sidecars,
                    },
            } = value;

            Self {
                header: Cow::Borrowed(header),
                transactions: Cow::Borrowed(transactions),
                ommers: Cow::Borrowed(ommers),
                withdrawals: withdrawals.as_ref().map(Cow::Borrowed),
                sidecars: sidecars.as_ref().map(Cow::Borrowed),
            }
        }
    }

    impl Encodable for BotanixBlockBody {
        fn encode(&self, out: &mut dyn bytes::BufMut) {
            BlockBodyHelper::from(self).encode(out);
        }

        fn length(&self) -> usize {
            BlockBodyHelper::from(self).length()
        }
    }

    impl Decodable for BotanixBlockBody {
        fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
            let BlockBodyHelper {
                transactions,
                ommers,
                withdrawals,
                sidecars,
            } = BlockBodyHelper::decode(buf)?;
            Ok(Self {
                inner: BlockBody {
                    transactions: transactions.into_owned(),
                    ommers: ommers.into_owned(),
                    withdrawals: withdrawals.map(|w| w.into_owned()),
                },
                sidecars: sidecars.map(|s| s.into_owned()),
            })
        }
    }

    impl Encodable for BotanixBlock {
        fn encode(&self, out: &mut dyn bytes::BufMut) {
            BlockHelper::from(self).encode(out);
        }

        fn length(&self) -> usize {
            BlockHelper::from(self).length()
        }
    }

    impl Decodable for BotanixBlock {
        fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
            let BlockHelper {
                header,
                transactions,
                ommers,
                withdrawals,
                sidecars,
            } = BlockHelper::decode(buf)?;
            Ok(Self {
                header: header.into_owned(),
                body: BotanixBlockBody {
                    inner: BlockBody {
                        transactions: transactions.into_owned(),
                        ommers: ommers.into_owned(),
                        withdrawals: withdrawals.map(|w| w.into_owned()),
                    },
                    sidecars: sidecars.map(|s| s.into_owned()),
                },
            })
        }
    }
}

pub mod serde_bincode_compat {
    use super::*;
    use reth_primitives_traits::serde_bincode_compat::{
        BincodeReprFor, SerdeBincodeCompat,
    };

    #[derive(Debug, Serialize, Deserialize)]
    pub struct BotanixBlockBodyBincode<'a> {
        inner: BincodeReprFor<'a, BlockBody>,
        sidecars: Option<Cow<'a, Vec<BotanixBlobTransactionSidecar>>>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct BotanixBlockBincode<'a> {
        header: BincodeReprFor<'a, Header>,
        body: BincodeReprFor<'a, BotanixBlockBody>,
    }

    impl SerdeBincodeCompat for BotanixBlockBody {
        type BincodeRepr<'a> = BotanixBlockBodyBincode<'a>;

        fn as_repr(&self) -> Self::BincodeRepr<'_> {
            BotanixBlockBodyBincode {
                inner: self.inner.as_repr(),
                sidecars: self.sidecars.as_ref().map(Cow::Borrowed),
            }
        }

        fn from_repr(repr: Self::BincodeRepr<'_>) -> Self {
            let BotanixBlockBodyBincode { inner, sidecars } = repr;
            Self {
                inner: BlockBody::from_repr(inner),
                sidecars: sidecars.map(|s| s.into_owned()),
            }
        }
    }

    impl SerdeBincodeCompat for BotanixBlock {
        type BincodeRepr<'a> = BotanixBlockBincode<'a>;

        fn as_repr(&self) -> Self::BincodeRepr<'_> {
            BotanixBlockBincode {
                header: self.header.as_repr(),
                body: self.body.as_repr(),
            }
        }

        fn from_repr(repr: Self::BincodeRepr<'_>) -> Self {
            let BotanixBlockBincode { header, body } = repr;
            Self {
                header: Header::from_repr(header),
                body: BotanixBlockBody::from_repr(body),
            }
        }
    }
}
