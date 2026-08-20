mod config;
mod error;
mod kernel;
mod protocol;
mod provider;
mod runtime;
mod session;
#[path = "testCDP.rs"]
mod test_cdp;
#[path = "testChatgpt.rs"]
mod test_chatgpt;
mod transport;

use anyhow::Context;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "b2t")]
#[command(version)]
#[command(about = "Browser2Tokens — Browser AI in. Local tokens out.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the B2T runtime and HTTP server
    Start,
    /// Isolated CDP spike: launch managed Chrome and evaluate JS on ChatGPT
    TestCdp,
    /// Isolated ChatGPT network observation spike
    TestChatgpt,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let cli = Cli::parse();
    match cli.command {
        Command::Start => {
            let config = config::Config::load();
            runtime::start(config)
                .await
                .context("failed to start Browser2Tokens runtime")?;
        }
        Command::TestCdp => test_cdp::run_cdp_test().await.context("CDP spike failed")?,
        Command::TestChatgpt => test_chatgpt::run_chatgpt_test()
            .await
            .context("ChatGPT network observation spike failed")?,
    }

    Ok(())
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Cli;

    #[test]
    fn cli_debug_assert() {
        Cli::command().debug_assert();
    }
}
