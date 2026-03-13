/// The human readable name of the client
pub const NAME_CLIENT: &str = "BtcServer";

/// The latest version from Cargo.toml.
pub const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The full SHA of the latest commit.
pub const VERGEN_GIT_SHA_LONG: &str = env!("VERGEN_GIT_SHA");

// The rustc version.
pub const VERGEN_RUSTC_SEMVER: &str = env!("VERGEN_RUSTC_SEMVER");

/// The 8 character short SHA of the latest commit.
pub const VERGEN_GIT_SHA: &str =
    const_format::str_index!(VERGEN_GIT_SHA_LONG, ..8);

/// The build timestamp.
pub const VERGEN_BUILD_TIMESTAMP: &str = env!("VERGEN_BUILD_TIMESTAMP");

/// Cargo features enabled at build time.
pub const VERGEN_CARGO_FEATURES: &str =
    env!("VERGEN_CARGO_FEATURES");

/// The target triple the binary was compiled for.
pub const VERGEN_CARGO_TARGET_TRIPLE: &str =
    env!("VERGEN_CARGO_TARGET_TRIPLE");

/// The build profile (e.g. "release", "debug").
pub const BUILD_PROFILE_NAME: &str = env!("BUILD_PROFILE_NAME");
