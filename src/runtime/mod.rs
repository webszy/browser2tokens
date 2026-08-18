//! Application runtime and HTTP service lifecycle.

use std::net::SocketAddr;

use axum::{Json, Router, routing::get};
use serde::Serialize;

use crate::config::Config;
use crate::error::{Error, Result};

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

/// Start the runtime and block on the HTTP server.
pub async fn start(config: Config) -> Result<()> {
    let addr = parse_listen_addr(&config)?;
    let app = router();
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|source| Error::Bind {
            addr: addr.to_string(),
            source,
        })?;

    println!("Browser2Tokens v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("[core] runtime started");
    println!("[http] listening on {addr}");

    tracing::info!("runtime started");
    tracing::info!("http server listening on {addr}");

    axum::serve(listener, app).await.map_err(Error::Server)?;

    Ok(())
}

fn parse_listen_addr(config: &Config) -> Result<SocketAddr> {
    let addr = config.listen_addr();
    addr.parse()
        .map_err(|source| Error::InvalidListenAddr { addr, source })
}

fn router() -> Router {
    Router::new().route("/health", get(health))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

#[cfg(test)]
mod tests {
    use super::{HealthResponse, parse_listen_addr};
    use crate::config::Config;

    #[test]
    fn health_json_is_ok() {
        let body = serde_json::to_string(&HealthResponse { status: "ok" })
            .expect("health response should serialize");
        assert_eq!(body, r#"{"status":"ok"}"#);
    }

    #[test]
    fn default_config_parses_listen_addr() {
        let addr = parse_listen_addr(&Config::default()).expect("default listen addr");
        assert_eq!(addr.to_string(), "127.0.0.1:8787");
    }
}
