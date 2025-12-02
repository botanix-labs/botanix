use crate::{
    node::evm::config::{BotanixBlockExecutorFactory, BotanixEvmConfig},
    BotanixBlock, BotanixBlockBody,
};
use alloy_consensus::{Block, Header};
use reth_evm::{
    block::BlockExecutionError,
    execute::{BlockAssembler, BlockAssemblerInput},
};

impl BlockAssembler<BotanixBlockExecutorFactory> for BotanixEvmConfig {
    type Block = BotanixBlock;

    fn assemble_block(
        &self,
        input: BlockAssemblerInput<'_, '_, BotanixBlockExecutorFactory, Header>,
    ) -> Result<Self::Block, BlockExecutionError> {
        let Block { header, body: inner } =
            self.block_assembler.assemble_block(input)?;
        Ok(BotanixBlock {
            header,
            body: BotanixBlockBody {
                inner,
                // HACK: we're setting sidecars to `None` here but ideally we should somehow get
                // them from the payload builder.
                //
                // Payload building is out of scope of botanix-reth for now, so this is not critical
                sidecars: None,
            },
        })
    }
}
