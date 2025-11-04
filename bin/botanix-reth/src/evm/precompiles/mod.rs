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
        let precompiles = if spec >= BotanixHardfork::Jalapeno {
            jalapeno()
        } else if spec >= BotanixHardfork::Pectra {
            pectra()
        } else {
            jalapeno()
        };

        Self {
            inner: EthPrecompiles {
                precompiles,
                spec: spec.into(),
            },
        }
    }

    #[inline]
    pub fn precompiles(&self) -> &'static Precompiles {
        self.inner.precompiles
    }
}

/// Returns precompiles for Jalapeno spec.
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

/// Returns precompiles for Jalapeno sepc.
pub fn jalapeno() -> &'static Precompiles {
    static INSTANCE: OnceBox<Precompiles> = OnceBox::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = genesis().clone();
        precompiles.extend([
            double_sign::DOUBLE_SIGN_EVIDENCE_VALIDATION,
            tm_secp256k1::TM_SECP256K1_SIGNATURE_RECOVER,
        ]);
        precompiles.extend([
            tendermint::TENDERMINT_HEADER_VALIDATION,
            iavl::IAVL_PROOF_VALIDATION,
        ]);
        precompiles.extend([
            cometbft::COMETBFT_LIGHT_BLOCK_VALIDATION,
            modexp::BERLIN,
        ]);
        precompiles.extend([iavl::IAVL_PROOF_VALIDATION_PLATO]);
        precompiles.extend([
            bls::BLS_SIGNATURE_VALIDATION,
            cometbft::COMETBFT_LIGHT_BLOCK_VALIDATION_BEFORE_HERTZ,
        ]);
        precompiles.extend([
            tendermint::TENDERMINT_HEADER_VALIDATION_NANO,
            iavl::IAVL_PROOF_VALIDATION_NANO,
        ]);
        Box::new(precompiles)
    })
}

/// Returns precompiles for Pectra spec.
pub fn pectra() -> &'static Precompiles {
    static INSTANCE: OnceBox<Precompiles> = OnceBox::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = jalapeno().clone();
        precompiles.extend([kzg_point_evaluation::POINT_EVALUATION]);
        Box::new(precompiles)
    })
}

impl Default for BotanixPrecompiles {
    fn default() -> Self {
        Self::new(BotanixHardfork::default())
    }
}
