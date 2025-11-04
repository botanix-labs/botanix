use super::peg_contract::{PeginData, PegoutWithId};
use reth_primitives::RecoveredBlock;
use reth_primitives_traits::Block;

/// Sealed block with pegin and pegout data
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedBlockWithPeg<B: Block = reth_primitives::Block> {
    /// Sealed block with senders
    block: RecoveredBlock<B>,
    /// Pegins
    pegins: Vec<PeginData>,
    /// Pegouts
    pegouts: Vec<PegoutWithId>,
}

impl<B: Block> SealedBlockWithPeg<B> {
    /// Create a new `SealedBlockWithPeg`
    pub const fn new(
        block: RecoveredBlock<B>,
        pegins: Vec<PeginData>,
        pegouts: Vec<PegoutWithId>,
    ) -> Self {
        Self {
            block,
            pegins,
            pegouts,
        }
    }

    /// Returns the block
    pub const fn block(&self) -> &RecoveredBlock<B> {
        &self.block
    }

    /// Pegins
    pub fn pegins(&self) -> &[PeginData] {
        self.pegins.as_slice()
    }

    /// Pegouts
    pub fn pegouts(&self) -> &[PegoutWithId] {
        self.pegouts.as_slice()
    }
}
