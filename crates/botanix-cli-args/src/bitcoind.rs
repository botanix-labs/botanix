use std::path::PathBuf;

use botanix_btc_server_client::jwt::{JwtError, JwtSecret};
use botanix_btc_wallet::bitcoind::BitcoindConfig;
use botanix_cli_parsers::parsers::{parse_grpc_address, parse_url};
use clap::Args;
use serde::{Deserialize, Serialize};
use url::Url;

/// Default bitcoind url
pub const DEFAULT_BITCOIND_URL: &str = "localhost:18443";

/// Default bitcoind username
pub const DEFAULT_BITCOIND_USERNAME: &str = "foo";

/// Default bitcoind password
pub const DEFAULT_BITCOIND_PASSWORD: &str = "bar";

/// Default bitcoin network
pub const DEFAULT_BITCOIN_NETWORK: bitcoin::Network = bitcoin::Network::Regtest;

/// Default btc server url
pub const DEFAULT_BTC_SERVER: &str = "127.0.0.1:8080";

/// Parameters to configure Bitcoind.
#[derive(Debug, Clone, Args, PartialEq, Eq, Serialize, Deserialize)]
#[clap(next_help_heading = "Bitcoind")]
pub struct BitcoindArgs {
    /// bitcoind RPC url
    ///
    /// The url of the bitcoind server.
    #[arg(default_value_t=Url::parse(DEFAULT_BITCOIND_URL).expect("valid url"), value_parser = parse_url, long = "bitcoind.url", name = "bitcoind.url", value_name = "BITCOIND_URL", env = "RETH_BITCOIND_URL"
    )]
    pub url: Url,

    /// Btcd username
    ///
    /// The username of the bitcoind server.
    #[arg(
        default_value_t = DEFAULT_BITCOIND_USERNAME.into(),
        long = "bitcoind.username",
        name = "bitcoind.username",
        value_name = "BITCOIND_USERNAME",
        env = "RETH_BITCOIND_USERNAME"
    )]
    pub username: String,

    /// Btcd password
    ///
    /// The password of the bitcoind server.
    #[arg(
        default_value_t = DEFAULT_BITCOIND_PASSWORD.into(),
        long = "bitcoind.password",
        name = "bitcoind.password",
        value_name = "BITCOIND_PASSWORD",
        env = "RETH_BITCOIND_PASSWORD"
    )]
    pub password: String,

    /// ZMQ address for bitcoind
    #[arg(
        long = "bitcoind.zmq.hash-block-address",
        name = "bitcoind.zmq.hash-block-address",
        value_name = "BITCOIND_ZMQ_HASH_BLOCK_ADDRESS",
        env = "BITCOIND_ZMQ_HASH_BLOCK_ADDRESS"
    )]
    pub zmq_hash_block_address: Option<Url>,

    #[arg(long,
        default_value_t = DEFAULT_BITCOIN_NETWORK,
        value_name = "BITCOIN_NETWORK",
        help_heading = "Btc_network",
        env = "RETH_BTC_NETWORK"
    )]
    pub btc_network: bitcoin::Network,

    /// Btc signing service
    ///
    /// The metrics will be served at the given interface and port.
    #[arg(long, value_name = "BTC_SERVER", value_parser = parse_grpc_address, help_heading = "Btc_server", env = "RETH_BTC_SERVER"
    )]
    pub btc_server: Option<String>,

    /// Btc server JWT secret path
    ///
    /// JWT secret path for jwt-encrypted communication between the client and the btc server
    #[arg(
        long,
        value_name = "BTC_SIGNING_SERVER_JWT_SECRET_PETH",
        help_heading = "btc_signing_server_jwt_secret_path",
        env = "RETH_BTC_SIGNING_SERVER_JWT_SECRET_PATH"
    )]
    pub btc_signing_server_jwt_secret_path: Option<PathBuf>,
}

impl Default for BitcoindArgs {
    fn default() -> Self {
        Self {
            url: Url::parse(DEFAULT_BITCOIND_URL).expect("valid url"),
            username: DEFAULT_BITCOIND_USERNAME.into(),
            password: DEFAULT_BITCOIND_PASSWORD.into(),
            zmq_hash_block_address: None,
            btc_network: DEFAULT_BITCOIN_NETWORK,
            btc_server: Some(DEFAULT_BTC_SERVER.to_owned()),
            btc_signing_server_jwt_secret_path: None,
        }
    }
}

impl From<BitcoindArgs> for BitcoindConfig {
    fn from(args: BitcoindArgs) -> Self {
        Self::new(args.url.clone(), args.username.clone(), args.password)
    }
}

impl BitcoindArgs {
    pub fn btc_signing_server_jwt_secret(
        &self,
    ) -> Result<Option<JwtSecret>, JwtError> {
        self.btc_signing_server_jwt_secret_path
            .as_ref()
            .map(|jwt| JwtSecret::from_file(jwt))
            .transpose()
    }
}
