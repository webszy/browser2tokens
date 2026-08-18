//! Shared domain errors.
//!
//! Application boundaries (`main`) use `anyhow`. Internal domain errors use
//! this type.

use thiserror::Error;

/// Recoverable runtime failure inside B2T.
#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid listen address '{addr}': {source}")]
    InvalidListenAddr {
        addr: String,
        #[source]
        source: std::net::AddrParseError,
    },
    #[error("failed to bind HTTP server on {addr}: {source}")]
    Bind {
        addr: String,
        #[source]
        source: std::io::Error,
    },
    #[error("HTTP server error: {0}")]
    Server(#[source] std::io::Error),
}

/// Domain result alias.
pub type Result<T> = std::result::Result<T, Error>;
