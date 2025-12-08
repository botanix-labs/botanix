use botanix_btc_wallet::error::BitcoindAdapterError;
use reth_provider::ProviderError;

mod block_watcher;
mod codec;
mod db_impl;
mod foundation;
mod payload;

// Re-exports.
pub use foundation::*;

/// Errors that can occur during Foundation layer operations.
pub enum Error {
    /// An error occurred while communicating with `bitcoind`.
    BitcoindAdapter(BitcoindAdapterError),
    /// An error occurred within the Foundation state machine.
    Foundation(botanix_tem::foundation::Error<ProviderError, ProviderError>),
    /// The [`FoundationTask`] has shut down.
    Shutdown,
}

impl From<BitcoindAdapterError> for Error {
    fn from(err: BitcoindAdapterError) -> Self {
        Error::BitcoindAdapter(err)
    }
}

impl From<botanix_tem::foundation::Error<ProviderError, ProviderError>>
    for Error
{
    fn from(
        err: botanix_tem::foundation::Error<ProviderError, ProviderError>,
    ) -> Self {
        Error::Foundation(err)
    }
}

impl From<botanix_tem::foundation::ValidationError> for Error {
    fn from(err: botanix_tem::foundation::ValidationError) -> Self {
        Error::Foundation(err.into())
    }
}

impl From<ProviderError> for Error {
    fn from(err: ProviderError) -> Self {
        Error::Foundation(botanix_tem::foundation::Error::BackendError(
            botanix_tem::foundation::BackendError::Database(err.into()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::db_impl::*;
    use botanix_chainspec::BotanixChainSpec;
    use botanix_storage::{
        tables::create_botanix_tables, BotanixProviderFactory,
    };
    use botanix_tem::{
        botanix_tem::foundation::{
            bitcoin::{hashes::Hash, TxMerkleNode},
            Error, Foundation, ValidationError,
        },
        test_utils::*,
    };
    use reth_botanix::node::{storage::BotanixStorage, BotanixNode};
    use reth_db::DatabaseEnv;
    use reth_provider::providers::StaticFileProvider;
    use reth_prune::PruneModes;
    use std::sync::Arc;

    #[test]
    #[ignore]
    fn foundation_test() {
        // ## Setup database for data.
        let mut db: DatabaseEnv =
            reth_db::init_db("test_foundation_data.db", Default::default())
                .unwrap();

        create_botanix_tables(&mut db).unwrap();

        let factory = BotanixProviderFactory::<_, BotanixNode>::new(
            Arc::new(db),
            Arc::new(BotanixChainSpec::default()),
            StaticFileProvider::read_write("test_static_provider").unwrap(),
            PruneModes::none(),
            Arc::new(BotanixStorage::default()),
        );

        let factory = WBotanixProviderFactory::new(factory).unwrap();

        let block_a = gen_bitcoin_hash();
        let block_b = gen_bitcoin_hash();
        let block_c = gen_bitcoin_hash();
        let merkle_tree = TxMerkleNode::all_zeros();

        // FOUNDATION: Setup.
        let mut f = Foundation::new_genesis(factory, block_a, 200, 3).unwrap();

        let origin_root = f.commitment_root().unwrap();

        // PROPOSE: Construct an invalid state transition.
        let res_err = f
            .propose_commitments(|c| {
                // INVALID: block_hash: `B`, parent_hash: `C`
                c.insert_bitcoin_header_unchecked(
                    block_b,
                    block_c,
                    merkle_tree,
                    201,
                )?;
                Ok(())
            })
            .unwrap_err();

        let Error::ValidationError(ValidationError::BadBitcoinHeader) = res_err
        else {
            panic!("unexpected result");
        };

        // Commitment state was RESET accordingly.
        let current_root = f.commitment_root().unwrap();
        assert_eq!(current_root, origin_root);

        // FINALIZE: Finalize an invalid state transition.
        let random_root = gen_foundation_state_root();
        let res_err = f
            .finalize_commitments(random_root, |c| {
                // INVALID: block_hash: `B`, parent_hash: `C`
                c.insert_bitcoin_header_unchecked(
                    block_b,
                    block_c,
                    merkle_tree,
                    201,
                )?;
                Ok(())
            })
            .unwrap_err();

        let Error::ValidationError(ValidationError::BadBitcoinHeader) = res_err
        else {
            panic!("unexpected result");
        };

        // Commitment state was RESET accordingly.
        let current_root = f.commitment_root().unwrap();
        assert_eq!(current_root, origin_root);
    }
}
