//! Runtime configuration.
//!
//! Defaults are in-code for now. Later this can load `~/.b2t/config.toml`.

use serde::{Deserialize, Serialize};

/// Process configuration for the local B2T runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub host: String,
    pub port: u16,
}

impl Config {
    pub const DEFAULT_HOST: &'static str = "127.0.0.1";
    pub const DEFAULT_PORT: u16 = 8787;

    /// Load configuration. File-based loading is not implemented yet.
    pub fn load() -> Self {
        Self::default()
    }

    pub fn listen_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: Self::DEFAULT_HOST.to_owned(),
            port: Self::DEFAULT_PORT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn default_listen_addr() {
        let config = Config::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8787);
        assert_eq!(config.listen_addr(), "127.0.0.1:8787");
    }
}
