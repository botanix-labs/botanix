use std::{fs, sync::Arc,};
use botanix_chainspec::{BotanixChainSpec};
use botanix_storage_migrate::{is_migration_needed, migrate_botanix_tables};
use eyre::{Context, Ok};
use reth::{args::{DatabaseArgs, DatadirArgs}};
use reth_db::DatabaseEnv;

const BOTANIX_DB_PATH: &str = "botanix_db";

/// Migrates Botanix tables from the Reth database to a separate Botanix database if needed.
///
/// # Arguments
///
/// * `datadir` - The data directory arguments.
/// * `chain_arc` - The Botanix chain specification.
/// * `db` - The database arguments.
///
/// # Returns
///
/// * `eyre::Result<()>` - Returns Ok if migration succeeds, otherwise an error.
pub fn init_and_migrate_db(
    datadir: &DatadirArgs,
    chain_arc: Arc<BotanixChainSpec>,
    db: &DatabaseArgs
) -> eyre::Result<(Arc<DatabaseEnv>, Arc<DatabaseEnv>)> {
    let data_dir = datadir.datadir.unwrap_or_chain_default(chain_arc.chain(), datadir.clone());
    let reth_db_path = data_dir.db();
    let botanix_db_path = data_dir.data_dir().join(BOTANIX_DB_PATH);

    // If botanix database path does not exist, it means we didn't migrate botanix tables yet.
    let is_migration_needed = is_migration_needed(&reth_db_path, &botanix_db_path)?;

    tracing::info!(target: "reth::cli", path = ?reth_db_path, "Opening reth database");
    let reth_database = Arc::new(reth_db::init_db(&reth_db_path, db.database_args())?.with_metrics());

    tracing::info!(target: "reth::cli", path = ?botanix_db_path, "Opening botanix database");
    let botanix_database = reth_db::init_db(&botanix_db_path, db.database_args())?;
    let botanix_database = Arc::new(botanix_database);

    // Move botanix tables from reth to botanix database
    if is_migration_needed {
        migrate_botanix_tables(&reth_database, &botanix_database).or_else(|e| {
            // If migration fails, we remove the botanix database directory to start from
            // scratch on the next run.
            fs::remove_dir_all(&botanix_db_path).wrap_err(format!(
                "Failed to remove botanix database directory {}",
                botanix_db_path.display()
            ))?;
            Err(e)
        })?;
    }
    Ok((reth_database, botanix_database))
}