use alloy_primitives::hex;
use botanix_btc_server_client::{
    BtcServerExtendedApi, BtcServerExtendedClient, RecoverMissingUtxosRequest,
};
use botanix_cli_args::poa_node::PoaNodeArgs;
use btcserverlib::utxo_recovery::read_utxos_from_file;
use eyre::Ok;

/// Recovers missing UTXOs using the provided PoA node configuration and Bitcoin server client.
pub async fn recover_missing_utxos(
    poa_cfg: &PoaNodeArgs,
    btc_server_client: &mut BtcServerExtendedClient,
) -> eyre::Result<()> {
    // if utxo_recovery_file is provided, use it to recover UTXOs
    if let Some(utxo_recovery_file) = &poa_cfg.utxo_recovery_file {
        tracing::info!(target: "reth::cli", "Recovering missing UTXOs from file: {}", utxo_recovery_file.display());
        let utxos =
            read_utxos_from_file(std::path::Path::new(&utxo_recovery_file));

        // Print all outpoints with their txids in hex format
        for (i, utxo) in utxos.iter().enumerate() {
            if let Some(outpoint) = &utxo.outpoint {
                let txid = hex::encode(&outpoint.txid);
                tracing::info!(
                    "reth::cli::recover_missing_utxos: UTXO recovery request number {}: txid: {}, vout: {}, eth_address: {}",
                    i, txid, outpoint.vout, utxo.eth_address
                );
            }
        }
        let recover_request = RecoverMissingUtxosRequest { utxos };
        // Only proceed if we have UTXOs to recover
        if !recover_request.utxos.is_empty() {
            match btc_server_client
                .recover_missing_utxos(recover_request)
                .await
            {
                std::result::Result::Ok(response) => {
                    tracing::info!(target: "reth::cli",
                        "reth::cli::recover_missing_utxos: UTXO recovery completed. Requested: {}, Recovered: {}",
                        response.total_requested, response.total_recovered
                    );
                }
                Err(err) => {
                    tracing::error!(target: "reth::cli", "reth::cli::recover_missing_utxos: UTXO recovery failed: {}", err);
                }
            }
        } else {
            tracing::error!(target: "reth::cli", "reth::cli::recover_missing_utxos: UTXO_RECOVERY_FILE is provided but no UTXOs to recover");
        }
    } else {
        tracing::info!(target: "reth::cli", "reth::cli::recover_missing_utxos:No UTXOs recovery file provided");
    };
    Ok(())
}
