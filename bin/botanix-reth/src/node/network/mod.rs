#![allow(clippy::owned_cow)]
use crate::{
    consensus::ParliaConsensus,
    node::{
        engine_api::payload::BotanixPayloadTypes,
        network::block_import::{handle::ImportHandle, service::ImportService, BotanixBlockImport},
        primitives::{BotanixBlobTransactionSidecar, BotanixPrimitives},
        BotanixNode,
    },
    BotanixBlock,
};
use alloy_rlp::{Decodable, Encodable};
use handshake::BotanixHandshake;
use reth::{
    api::{FullNodeTypes, TxTy},
    builder::{components::NetworkBuilder, BuilderContext},
    transaction_pool::{PoolTransaction, TransactionPool},
};
use reth_chainspec::EthChainSpec;
use reth_discv4::Discv4Config;
use reth_engine_primitives::BeaconConsensusEngineHandle;
use reth_eth_wire::{BasicNetworkPrimitives, NewBlock, NewBlockPayload};
use reth_ethereum_primitives::PooledTransactionVariant;
use reth_network::{NetworkConfig, NetworkHandle, NetworkManager};
use reth_network_api::PeersInfo;
use std::{sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::info;

pub mod block_import;
pub mod bootnodes;
pub mod handshake;
pub(crate) mod upgrade_status;

/// Botanix `NewBlock` message value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotanixNewBlock(pub NewBlock<BotanixBlock>);

mod rlp {
    use super::*;
    use crate::BotanixBlockBody;
    use alloy_consensus::{BlockBody, Header};
    use alloy_primitives::U128;
    use alloy_rlp::{RlpDecodable, RlpEncodable};
    use alloy_rpc_types::Withdrawals;
    use reth_primitives::TransactionSigned;
    use std::borrow::Cow;

    #[derive(RlpEncodable, RlpDecodable)]
    #[rlp(trailing)]
    struct BlockHelper<'a> {
        header: Cow<'a, Header>,
        transactions: Cow<'a, Vec<TransactionSigned>>,
        ommers: Cow<'a, Vec<Header>>,
        withdrawals: Option<Cow<'a, Withdrawals>>,
    }

    #[derive(RlpEncodable, RlpDecodable)]
    #[rlp(trailing)]
    struct BotanixNewBlockHelper<'a> {
        block: BlockHelper<'a>,
        td: U128,
        sidecars: Option<Cow<'a, Vec<BotanixBlobTransactionSidecar>>>,
    }

    impl<'a> From<&'a BotanixNewBlock> for BotanixNewBlockHelper<'a> {
        fn from(value: &'a BotanixNewBlock) -> Self {
            let BotanixNewBlock(NewBlock {
                block:
                    BotanixBlock {
                        header,
                        body:
                            BotanixBlockBody {
                                inner: BlockBody { transactions, ommers, withdrawals },
                                sidecars,
                            },
                    },
                td,
            }) = value;

            Self {
                block: BlockHelper {
                    header: Cow::Borrowed(header),
                    transactions: Cow::Borrowed(transactions),
                    ommers: Cow::Borrowed(ommers),
                    withdrawals: withdrawals.as_ref().map(Cow::Borrowed),
                },
                td: *td,
                sidecars: sidecars.as_ref().map(Cow::Borrowed),
            }
        }
    }

    impl Encodable for BotanixNewBlock {
        fn encode(&self, out: &mut dyn bytes::BufMut) {
            BotanixNewBlockHelper::from(self).encode(out);
        }

        fn length(&self) -> usize {
            BotanixNewBlockHelper::from(self).length()
        }
    }

    impl Decodable for BotanixNewBlock {
        fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
            let BotanixNewBlockHelper {
                block: BlockHelper { header, transactions, ommers, withdrawals },
                td,
                sidecars,
            } = BotanixNewBlockHelper::decode(buf)?;

            Ok(BotanixNewBlock(NewBlock {
                block: BotanixBlock {
                    header: header.into_owned(),
                    body: BotanixBlockBody {
                        inner: BlockBody {
                            transactions: transactions.into_owned(),
                            ommers: ommers.into_owned(),
                            withdrawals: withdrawals.map(|w| w.into_owned()),
                        },
                        sidecars: sidecars.map(|s| s.into_owned()),
                    },
                },
                td,
            }))
        }
    }
}

impl NewBlockPayload for BotanixNewBlock {
    type Block = BotanixBlock;

    fn block(&self) -> &Self::Block {
        &self.0.block
    }
}

/// Network primitives for Botanix.
pub type BotanixNetworkPrimitives =
    BasicNetworkPrimitives<BotanixPrimitives, PooledTransactionVariant, BotanixNewBlock>;

/// A basic Botanix network builder.
#[derive(Debug)]
pub struct BotanixNetworkBuilder {
    pub(crate) engine_handle_rx:
        Arc<Mutex<Option<oneshot::Receiver<BeaconConsensusEngineHandle<BotanixPayloadTypes>>>>>,
}

impl BotanixNetworkBuilder {
    /// Returns the [`NetworkConfig`] that contains the settings to launch the p2p network.
    ///
    /// This applies the configured [`BotanixNetworkBuilder`] settings.
    pub fn network_config<Node>(
        self,
        ctx: &BuilderContext<Node>,
    ) -> eyre::Result<NetworkConfig<Node::Provider, BotanixNetworkPrimitives>>
    where
        Node: FullNodeTypes<Types = BotanixNode>,
    {
        let Self { engine_handle_rx } = self;

        let network_builder = ctx.network_config_builder()?;
        let mut discv4 = Discv4Config::builder();

        if let Some(boot_nodes) = ctx.chain_spec().bootnodes() {
            discv4.add_boot_nodes(boot_nodes);
        }
        discv4.lookup_interval(Duration::from_millis(500));

        let (to_import, from_network) = mpsc::unbounded_channel();
        let (to_network, import_outcome) = mpsc::unbounded_channel();

        let handle = ImportHandle::new(to_import, import_outcome);
        let consensus = Arc::new(ParliaConsensus { provider: ctx.provider().clone() });

        ctx.task_executor().spawn_critical("block import", async move {
            let handle = engine_handle_rx
                .lock()
                .await
                .take()
                .expect("node should only be launched once")
                .await
                .unwrap();

            ImportService::new(consensus, handle, from_network, to_network).await.unwrap();
        });

        let network_builder = network_builder
            .boot_nodes(ctx.chain_spec().bootnodes().unwrap_or_default())
            .set_head(ctx.chain_spec().head())
            .with_pow()
            .block_import(Box::new(BotanixBlockImport::new(handle)))
            .discovery(discv4)
            .eth_rlpx_handshake(Arc::new(BotanixHandshake::default()));

        let network_config = ctx.build_network_config(network_builder);

        Ok(network_config)
    }
}

impl<Node, Pool> NetworkBuilder<Node, Pool> for BotanixNetworkBuilder
where
    Node: FullNodeTypes<Types = BotanixNode>,
    Pool: TransactionPool<
            Transaction: PoolTransaction<
                Consensus = TxTy<Node::Types>,
                Pooled = PooledTransactionVariant,
            >,
        > + Unpin
        + 'static,
{
    type Network = NetworkHandle<BotanixNetworkPrimitives>;

    async fn build_network(
        self,
        ctx: &BuilderContext<Node>,
        pool: Pool,
    ) -> eyre::Result<Self::Network> {
        let network_config = self.network_config(ctx)?;
        let network = NetworkManager::builder(network_config).await?;
        let handle = ctx.start_network(network, pool);
        info!(target: "reth::cli", enode=%handle.local_node_record(), "P2P networking initialized");

        Ok(handle)
    }
}
