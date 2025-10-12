use std::ops::{Deref, DerefMut};
use crate::evm::transaction::BotanixTxEnv;
use super::precompiles::BotanixPrecompiles;
use botanix_chainspec::BotanixHardfork;
use reth_evm::{precompiles::PrecompilesMap, Database, EvmEnv};
use revm::{
    context::{BlockEnv, CfgEnv, Evm as EvmCtx, FrameStack, JournalTr},
    handler::{
        evm::{ContextDbError, FrameInitResult},
        instructions::EthInstructions,
        EthFrame, EvmTr, FrameInitOrResult, FrameResult,
    },
    inspector::InspectorEvmTr,
    interpreter::{interpreter::EthInterpreter, interpreter_action::FrameInit},
    Context, Inspector, Journal,
};

mod exec;

/// Type alias for the default context type of the BotanixEvm.
pub type BotanixContext<DB> = Context<BlockEnv, BotanixTxEnv, CfgEnv<BotanixHardfork>, DB>;

/// Botanix EVM implementation.
///
/// This is a wrapper type around the `revm` evm with optional [`Inspector`] (tracing)
/// support. [`Inspector`] support is configurable at runtime because it's part of the underlying
#[allow(missing_debug_implementations)]
pub struct BotanixEvm<DB: revm::database::Database, I> {
    pub inner: EvmCtx<
        BotanixContext<DB>,
        I,
        EthInstructions<EthInterpreter, BotanixContext<DB>>,
        PrecompilesMap,
        EthFrame,
    >,
    pub inspect: bool,
}

impl<DB: Database, I> BotanixEvm<DB, I> {
    /// Creates a new [`BotanixEvm`].
    pub fn new(env: EvmEnv<BotanixHardfork>, db: DB, inspector: I, inspect: bool) -> Self {
        let precompiles =
            PrecompilesMap::from_static(BotanixPrecompiles::new(env.cfg_env.spec).precompiles());

        Self {
            inner: EvmCtx {
                ctx: Context {
                    block: env.block_env,
                    cfg: env.cfg_env,
                    journaled_state: Journal::new(db),
                    tx: Default::default(),
                    chain: Default::default(),
                    local: Default::default(),
                    error: Ok(()),
                },
                inspector,
                instruction: EthInstructions::new_mainnet(),
                precompiles,
                frame_stack: Default::default(),
            },
            inspect,
        }
    }
}

impl<DB: Database, I> BotanixEvm<DB, I> {
    /// Provides a reference to the EVM context.
    pub const fn ctx(&self) -> &BotanixContext<DB> {
        &self.inner.ctx
    }

    /// Provides a mutable reference to the EVM context.
    pub fn ctx_mut(&mut self) -> &mut BotanixContext<DB> {
        &mut self.inner.ctx
    }
}

impl<DB: Database, I> Deref for BotanixEvm<DB, I> {
    type Target = BotanixContext<DB>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.ctx()
    }
}

impl<DB: Database, I> DerefMut for BotanixEvm<DB, I> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ctx_mut()
    }
}

impl<DB, INSP> EvmTr for BotanixEvm<DB, INSP>
where
    DB: Database,
{
    type Context = BotanixContext<DB>;
    type Instructions = EthInstructions<EthInterpreter, BotanixContext<DB>>;
    type Precompiles = PrecompilesMap;
    type Frame = EthFrame;

    fn ctx(&mut self) -> &mut Self::Context {
        self.inner.ctx_mut()
    }

    fn ctx_ref(&self) -> &Self::Context {
        self.inner.ctx_ref()
    }

    fn ctx_instructions(&mut self) -> (&mut Self::Context, &mut Self::Instructions) {
        self.inner.ctx_instructions()
    }

    fn ctx_precompiles(&mut self) -> (&mut Self::Context, &mut Self::Precompiles) {
        self.inner.ctx_precompiles()
    }

    /// Returns a mutable reference to the frame stack.
    fn frame_stack(&mut self) -> &mut FrameStack<Self::Frame> {
        self.inner.frame_stack()
    }

    fn frame_init(
        &mut self,
        frame_input: FrameInit,
    ) -> Result<FrameInitResult<'_, Self::Frame>, ContextDbError<Self::Context>> {
        self.inner.frame_init(frame_input)
    }

    fn frame_run(
        &mut self,
    ) -> Result<FrameInitOrResult<Self::Frame>, ContextDbError<Self::Context>> {
        self.inner.frame_run()
    }

    fn frame_return_result(
        &mut self,
        result: FrameResult,
    ) -> Result<Option<FrameResult>, ContextDbError<Self::Context>> {
        self.inner.frame_return_result(result)
    }
}

impl<DB, INSP> InspectorEvmTr for BotanixEvm<DB, INSP>
where
    DB: Database,
    INSP: Inspector<BotanixContext<DB>>,
{
    type Inspector = INSP;

    fn inspector(&mut self) -> &mut Self::Inspector {
        self.inner.inspector()
    }

    fn ctx_inspector(&mut self) -> (&mut Self::Context, &mut Self::Inspector) {
        self.inner.ctx_inspector()
    }

    fn ctx_inspector_frame(
        &mut self,
    ) -> (&mut Self::Context, &mut Self::Inspector, &mut Self::Frame) {
        self.inner.ctx_inspector_frame()
    }

    fn ctx_inspector_frame_instructions(
        &mut self,
    ) -> (&mut Self::Context, &mut Self::Inspector, &mut Self::Frame, &mut Self::Instructions) {
        self.inner.ctx_inspector_frame_instructions()
    }
}
