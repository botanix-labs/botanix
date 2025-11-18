use crate::{
    node::{
        consensus::BotanixConsensusBuilder,
        engine_api::{
            builder::BotanixEngineApiBuilder,
            payload::BotanixPayloadTypes,
            validator::{
                BotanixEngineValidatorBuilder, BotanixPayloadValidatorBuilder,
            },
        },
        primitives::BotanixPrimitives,
        storage::BotanixStorage,
    },
    BotanixBlock, BotanixBlockBody,
};
use botanix_chainspec::BotanixChainSpec;
use engine::BotanixPayloadServiceBuilder;
use evm::BotanixExecutorBuilder;
use network::BotanixNetworkBuilder;
use reth::{
    api::{FullNodeComponents, FullNodeTypes, NodeTypes},
    builder::{
        components::ComponentsBuilder, rpc::RpcAddOns, DebugNode, Node,
        NodeAdapter,
    },
};
use reth_engine_local::LocalPayloadAttributesBuilder;
use reth_node_ethereum::{node::EthereumPoolBuilder, EthereumEthApiBuilder};
use reth_payload_primitives::{PayloadAttributesBuilder, PayloadTypes};
use reth_primitives::BlockBody;
use reth_trie_db::MerklePatriciaTrie;
use std::sync::Arc;

/// Consensus module containing logic related to Botanix consensus.
pub mod consensus;
/// Engine module containing logic related to the Botanix engine.
pub mod engine;
/// EVM module containing logic related to the Botanix EVM.
pub mod engine_api;
/// Chainspec module containing logic related to Botanix chain specifications.
pub mod evm;
/// Node module containing logic related to Botanix nodes.
pub mod network;
/// Payload module containing logic related to Botanix payloads.
pub mod primitives;
/// Storage module containing logic related to Botanix storage.
pub mod storage;

/// Botanix addons configuring RPC types
pub type BotanixNodeAddOns<N> = RpcAddOns<
    N,
    EthereumEthApiBuilder,
    BotanixPayloadValidatorBuilder,
    BotanixEngineApiBuilder,
    BotanixEngineValidatorBuilder,
>;

/// Type configuration for a regular Botanix node.
#[derive(Debug, Default, Clone)]
pub struct BotanixNode {}

impl BotanixNode {
    /// Builds and returns the components builder for the BotanixNode.
    pub fn components<Node>(
        &self,
    ) -> ComponentsBuilder<
        Node,
        EthereumPoolBuilder,
        BotanixPayloadServiceBuilder,
        BotanixNetworkBuilder,
        BotanixExecutorBuilder,
        BotanixConsensusBuilder,
    >
    where
        Node: FullNodeTypes<Types = Self>,
    {
        ComponentsBuilder::default()
            .node_types::<Node>()
            .pool(EthereumPoolBuilder::default())
            .executor(BotanixExecutorBuilder::default())
            .payload(BotanixPayloadServiceBuilder::default())
            .network(BotanixNetworkBuilder::default())
            .consensus(BotanixConsensusBuilder::default())
    }
}

impl NodeTypes for BotanixNode {
    type Primitives = BotanixPrimitives;
    type ChainSpec = BotanixChainSpec;
    type StateCommitment = MerklePatriciaTrie;
    type Storage = BotanixStorage;
    type Payload = BotanixPayloadTypes;
}

impl<N> Node<N> for BotanixNode
where
    N: FullNodeTypes<Types = Self>,
{
    type ComponentsBuilder = ComponentsBuilder<
        N,
        EthereumPoolBuilder,
        BotanixPayloadServiceBuilder,
        BotanixNetworkBuilder,
        BotanixExecutorBuilder,
        BotanixConsensusBuilder,
    >;

    type AddOns = BotanixNodeAddOns<NodeAdapter<N>>;

    fn components_builder(&self) -> Self::ComponentsBuilder {
        Self::components(self)
    }

    fn add_ons(&self) -> Self::AddOns {
        BotanixNodeAddOns::default()
    }
}

impl<N> DebugNode<N> for BotanixNode
where
    N: FullNodeComponents<Types = Self>,
{
    type RpcBlock = alloy_rpc_types::Block;

    fn rpc_to_primitive_block(rpc_block: Self::RpcBlock) -> BotanixBlock {
        let alloy_rpc_types::Block {
            header, transactions, withdrawals, ..
        } = rpc_block;
        BotanixBlock {
            header: header.inner,
            body: BotanixBlockBody {
                inner: BlockBody {
                    transactions: transactions
                        .into_transactions()
                        .map(|tx| tx.inner.into_inner().into())
                        .collect(),
                    ommers: Default::default(),
                    withdrawals,
                },
                sidecars: None,
            },
        }
    }

    fn local_payload_attributes_builder(
        chain_spec: &Self::ChainSpec,
    ) -> impl PayloadAttributesBuilder<
        <Self::Payload as PayloadTypes>::PayloadAttributes,
    > {
        LocalPayloadAttributesBuilder::new(Arc::new(chain_spec.clone()))
    }
}
