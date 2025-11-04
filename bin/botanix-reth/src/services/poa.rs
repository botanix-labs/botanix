use crate::consensus::AuthorityConsensusBuilder;

// pub async fn construct_poa() -> eyre::Result<()> {
//     let (abci_started_tx, abci_started_rx) = tokio::sync::oneshot::channel::<()>();
//     let (frost_task, abci_client_builder, snapshot_manager, wallet_sync) =
//         match AuthorityConsensusBuilder::try_new(
//             Arc::clone(&chain_arc.clone()),
//             blockchain_db.clone(),
//             activation_manager,
//             btc_server_factory.clone(),
//             bitcoin_checkpoints.clone(),
//             secret_key,
//             network_handle.clone(),
//             frost_handle,
//             executor.clone(),
//             frost_config,
//             node_config.rpc.btc_network,
//             genesis_authorities.clone(),
//             authorities_socket_addresses,
//             executor_factory.clone(),
//             bitcoind_factory.clone(),
//             evm_config,
//             cometbft_rpc_factory,
//             RandomSourceProvider::new(),
//             driver_tx,
//             node_config.clone().state_sync,
//             reth_provider_factory.clone(),
//             botanix_database_provider_factory,
//             *block_fee_recipient_address,
//             bitcoind_client,
//         ) {
//             Ok(consensus) => consensus.build::<BtcServerExtendedClient>().await,
//             Err(e) => {
//                 return Err(eyre::eyre!("AuthorityConsensusBuilderError : {:?}", e));
//             }
//         };
// }
