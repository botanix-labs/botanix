#![allow(unused)]

use botanix_chainspec::BotanixHardfork;
use cfg_if::cfg_if;
use once_cell::race::OnceBox;
use revm::{
    context::Cfg,
    handler::EthPrecompiles,
    precompile::{
        bls12_381, kzg_point_evaluation, modexp, secp256r1, Precompiles,
    },
};
use std::boxed::Box;

mod bls;
mod cometbft;
mod double_sign;
mod error;
mod iavl;
mod tendermint;
mod tm_secp256k1;

// Botanix precompile provider
#[derive(Debug, Clone)]
pub struct BotanixPrecompiles {
    /// Inner precompile provider is same as Ethereums.
    inner: EthPrecompiles,
}

impl BotanixPrecompiles {
    /// Create a new precompile provider with the given Botanix spec.
    #[inline]
    pub fn new(spec: BotanixHardfork) -> Self {
        // Add more precompiles as needed
        Self {
            inner: EthPrecompiles {
                precompiles: Precompiles::prague(),
                spec: spec.into(),
            },
        }
    }

    #[inline]
    pub fn precompiles(&self) -> &'static Precompiles {
        self.inner.precompiles
    }
}

/// Returns precompiles for the Prague spec.
pub fn genesis() -> &'static Precompiles {
    static INSTANCE: OnceBox<Precompiles> = OnceBox::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = Precompiles::prague().clone(); // NOTE: Currently Botanix is same as Prague
        precompiles.extend([
            tendermint::TENDERMINT_HEADER_VALIDATION,
            iavl::IAVL_PROOF_VALIDATION,
        ]);
        Box::new(precompiles)
    })
}

impl Default for BotanixPrecompiles {
    fn default() -> Self {
        Self::new(BotanixHardfork::default())
    }
}
