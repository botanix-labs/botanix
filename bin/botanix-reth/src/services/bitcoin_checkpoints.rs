use std::{sync::Arc, time::Duration};
use botanix_bitcoin_checkpoint::{BitcoinCheckpointsChain, BitcoinCheckpointsChainSynchronizer, BitcoinHashBlockStream, DummyHashBlockStream};
use botanix_btc_wallet::bitcoind::BitcoindClient;
use botanix_chainspec::BotanixChainSpec;
use botanix_cli_args::bitcoind::BitcoindArgs;
use eyre::Ok;
use bitcoincore_zmq::subscribe_async_wait_handshake;
use tokio::time::timeout;


/// Sets up the Bitcoin checkpoints synchronizer and block hash stream using the provided Bitcoind client,
/// configuration, and chain specification. Waits for the Bitcoind client to sync and connects to the ZMQ socket
/// if available, otherwise falls back to a dummy stream.
///
/// # Arguments
///
/// * `bitcoind_client` - The Bitcoind client instance.
/// * `bitcoind_cfg` - The Bitcoind configuration arguments.
/// * `chain` - The Botanix chain specification.
///
/// # Returns
///
/// Returns a tuple containing the Bitcoin checkpoints chain synchronizer and the block hash stream.
pub async fn setup_bitcoin_checkpoints(
    bitcoind_client: BitcoindClient,
    bitcoind_cfg: &BitcoindArgs,
    chain: &BotanixChainSpec,
) -> eyre::Result<(BitcoinCheckpointsChainSynchronizer, BitcoinHashBlockStream, Arc<BitcoinCheckpointsChain>)> {
    tracing::info!(target: "reth::cli", "Waiting for bitcoind client to sync...");

    match tokio::time::timeout(Duration::from_secs(60), bitcoind_client.get_rpc_client_dyn().wait_until_synced()).await {
        std::result::Result::Ok(_) => {
            tracing::info!(target: "reth::cli", "Bitcoind client synced");
        }
        Err(_) => {
            tracing::error!(target: "reth::cli", "Bitcoind client could not achieve synced status within 60 secs. Exiting...");
            return Err(eyre::eyre!(
                "Bitcoind client could not achieve synced status within 60secs. Exiting..."
            ));
        }
    }

    let bitcoin_checkpoints = Arc::new(BitcoinCheckpointsChain::try_new(
        chain.bitcoin_checkpoint_confirmation_depth as usize,
        chain.historical_bitcoin_checkpoints_count,
        chain.weak_bitcoin_checkpoints_count,
    )?);

    let checkpoints_synchronizer = BitcoinCheckpointsChainSynchronizer::new(
        Arc::clone(&bitcoin_checkpoints),
        bitcoind_client.into_rpc_arc(),
    );

    // Connect to Bitcoin ZMQ socket to receive new block notifications
    // to synchronize the bitcoin checkpoints chain with the bitcoind node

    let bitcoin_zmq_block_hash_stream: BitcoinHashBlockStream = if let Some(
        zmq_hash_block_address,
    ) =
        &bitcoind_cfg.zmq_hash_block_address
    {
        // Connect to the ZMQ socket for block hash notifications
        // if the zmq hash block address is provided

        // Timeout if we cannot connect to the ZMQ socket after 5 seconds
        let connection_timeout = Duration::from_secs(5);

        match timeout(
            connection_timeout,
            subscribe_async_wait_handshake(&[zmq_hash_block_address.as_str()]),
        )
        .await
        {
            std::result::Result::Ok(std::result::Result::Ok(stream)) => {
                tracing::info!(target: "reth::cli", "Connected to bitcoind ZMQ hashblock socket {}", zmq_hash_block_address);

                Box::new(stream)
            }
            std::result::Result::Ok(Err(err)) => {
                // Ok from `timeout` but an error from the subscribe function.
                return Err(eyre::eyre!(
                    "Failed to subscribe to bitcoind ZMQ hashblock socket {}: {}",
                    zmq_hash_block_address,
                    err
                ));
            }
            Err(_) => {
                // Timeout error
                return Err(eyre::eyre!(
                    "Timeout to subscribe to bitcoind ZMQ hashblock socket {} after {} secs",
                    zmq_hash_block_address,
                    connection_timeout.as_secs_f64(),
                ));
            }
        }
    } else {
        // ZMQ socket for block hash notifications
        // is not provided. Fall back to an interval update logic

        // TODO: Remove this fallback and make zmq socket mandatory when we release
        //  version 2

        let update_interval = Duration::from_secs(5);

        tracing::warn!(target: "reth::cli", "No ZMQ hash block address provided. Using dummy block hash stream with checkpoints update interval of {} seconds", update_interval.as_secs_f64());

        let stream = DummyHashBlockStream::new(update_interval);

        Box::new(stream)
    };

    Ok((checkpoints_synchronizer, bitcoin_zmq_block_hash_stream, bitcoin_checkpoints))
}