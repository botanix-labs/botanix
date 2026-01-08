use crate::{provider::BotanixProviderFactory, tables::create_botanix_tables};
use reth_chainspec::{ChainSpec, ChainSpecBuilder};
use reth_db::{
    mdbx::{init_db, DatabaseArguments, MaxReadTransactionDuration},
    models::ClientVersion,
    test_utils::{tempdir_path, TempDatabase},
    DatabaseEnv,
};
use reth_ethereum_engine_primitives::EthPayloadTypes;
use reth_node_types::AnyNodeTypes;
use reth_primitives::EthPrimitives;
use reth_provider::providers::StaticFileProvider;
use reth_prune::PruneModes;
use reth_storage_api::EthStorage;
use reth_trie_db::MerklePatriciaTrie;
use std::sync::Arc;

type TestNodeTypes = AnyNodeTypes<
    EthPrimitives,
    ChainSpec,
    MerklePatriciaTrie,
    EthStorage,
    EthPayloadTypes,
>;

/// Creates test database and provider factory.
///
/// This function creates a temporary in-memory database with a provider factory
/// for testing purposes. The database is automatically cleaned up when the
/// returned factory is dropped, making it ideal for unit tests.
///
/// # Returns
///
/// A `BotanixProviderFactory` instance backed by a temporary database that:
/// - Supports both read and write operations
/// - Is isolated from other tests
/// - Is automatically cleaned up after use
/// - Uses the same database structure as production
///
/// # Usage
///
/// ```rust,ignore
/// use botanix_storage::test_utils::create_test_provider_factory;
///
/// #[test]
/// fn test_snapshot_creation() {
///     let factory = create_test_provider_factory();
///     let provider = factory.provider_rw().unwrap();
///
///     let snapshot_id = provider.create_new_snapshot(100, block_hash).unwrap();
///     provider.commit().unwrap();
///
///     assert!(snapshot_id > 0);
/// }
/// ```
///
/// # Thread Safety
///
/// Each call to this function creates a separate database instance, so
/// multiple tests can run concurrently without interfering with each other.
pub fn create_test_provider_factory(
) -> BotanixProviderFactory<Arc<TempDatabase<DatabaseEnv>>, TestNodeTypes> {
    let db_path = tempdir_path();
    let mut db = init_db(
        &db_path,
        DatabaseArguments::new(ClientVersion::default())
            .with_max_read_transaction_duration(Some(
                MaxReadTransactionDuration::Unbounded,
            )),
    )
    .expect("test db");
    create_botanix_tables(&mut db).expect("botanix tables");
    let db = Arc::new(TempDatabase::new(db, db_path));
    let chain_spec = Arc::new(ChainSpecBuilder::mainnet().build());
    let static_files_dir = db.path().join("static_files");
    std::fs::create_dir_all(&static_files_dir)
        .expect("static files dir for tests");
    let static_file_provider =
        StaticFileProvider::read_write(&static_files_dir)
            .expect("static files provider for tests");
    let prune_modes = PruneModes::none();
    let storage = Arc::new(EthStorage::default());

    BotanixProviderFactory::new(
        db,
        chain_spec,
        static_file_provider,
        prune_modes,
        storage,
    )
}
