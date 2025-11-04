use clap::Args;

pub mod bitcoind;
pub mod chain;
pub mod frost_args;
pub mod poa_node;
pub mod state_sync;

#[derive(Clone, Debug, Args)]
#[command(next_help_heading = "Botanix")]
pub struct BotanixArgs {
    #[command(flatten)]
    pub bitcoind: bitcoind::BitcoindArgs,

    #[command(flatten)]
    pub frost: frost_args::FrostArgs,

    #[command(flatten)]
    pub poa: poa_node::PoaNodeArgs,

    #[command(flatten)]
    pub state_sync: state_sync::StateSyncArgs,
}
