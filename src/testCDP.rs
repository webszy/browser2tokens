//! Isolated CDP spike.
//!
//! This file is intentionally not part of the production Protocol / Provider /
//! Transport architecture. It only proves:
//!
//! Rust → managed Chrome → CDP → ChatGPT tab → Runtime.evaluate

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow, bail};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::page::Page;
use futures::StreamExt;

const CHATGPT_URL: &str = "https://chatgpt.com";
const MACOS_CHROME: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const TARGET_WAIT: Duration = Duration::from_secs(30);
const TARGET_POLL: Duration = Duration::from_millis(500);

/// Launch managed Chrome, find the ChatGPT page, and evaluate two JS expressions.
pub async fn run_cdp_test() -> anyhow::Result<()> {
    tracing::info!("Browser2Tokens CDP Spike");

    let chrome = chrome_executable()?;
    let profile = profile_dir()?;
    tokio::fs::create_dir_all(&profile).await.with_context(|| {
        format!(
            "failed to create Chrome profile directory {}",
            profile.display()
        )
    })?;

    tracing::info!("[chrome] launching managed Chrome");
    tracing::info!(path = %chrome.display(), "[chrome] executable");
    tracing::info!(path = %profile.display(), "[chrome] profile");

    let config = BrowserConfig::builder()
        .chrome_executable(&chrome)
        .user_data_dir(&profile)
        .with_head()
        .viewport(None)
        .respect_https_errors()
        .build()
        .map_err(|error| anyhow!("failed to build Chrome launch config: {error}"))?;

    let (mut browser, mut handler) = Browser::launch(config)
        .await
        .context("failed to launch managed Chrome")?;

    let mut handler_task = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(error) = event {
                tracing::warn!(%error, "CDP handler terminated unexpectedly");
                break;
            }
        }
    });

    tracing::info!("[chrome] launched");
    tracing::info!(
        websocket = %browser.websocket_address(),
        "[cdp] connected"
    );

    browser
        .new_page(CHATGPT_URL)
        .await
        .with_context(|| format!("failed to open {CHATGPT_URL}"))?;

    let page = tokio::select! {
        result = wait_for_chatgpt_page(&browser) => result?,
        join = &mut handler_task => {
            match join {
                Ok(()) => bail!("CDP handler terminated unexpectedly"),
                Err(error) => bail!("CDP handler terminated unexpectedly: {error}"),
            }
        }
    };

    tracing::info!("[target] ChatGPT page connected");

    let title = evaluate_string(&page, "document.title")
        .await
        .context("failed to evaluate document.title")?;
    tracing::info!(r#"[eval] document.title = "{title}""#);

    let href = evaluate_string(&page, "location.href")
        .await
        .context("failed to evaluate location.href")?;
    tracing::info!(r#"[eval] location.href = "{href}""#);

    tracing::info!("[cdp] test passed");
    tracing::info!("[cdp] Chrome is open for manual login if needed. Press Ctrl+C to exit.");

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("[cdp] shutting down");
        }
        join = &mut handler_task => {
            match join {
                Ok(()) => bail!("CDP handler terminated unexpectedly"),
                Err(error) => bail!("CDP handler terminated unexpectedly: {error}"),
            }
        }
    }

    if let Err(error) = browser.close().await {
        tracing::warn!(%error, "failed to close managed Chrome cleanly");
    }
    handler_task.abort();

    Ok(())
}

fn chrome_executable() -> anyhow::Result<PathBuf> {
    let path = PathBuf::from(MACOS_CHROME);
    if path.is_file() {
        return Ok(path);
    }
    bail!("Google Chrome executable not found at {MACOS_CHROME}");
}

fn profile_dir() -> anyhow::Result<PathBuf> {
    let home = home_dir()?;
    Ok(home.join(".b2t").join("chrome-profile"))
}

fn home_dir() -> anyhow::Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .context("failed to determine user home directory")
}

async fn wait_for_chatgpt_page(browser: &Browser) -> anyhow::Result<Page> {
    let deadline = Instant::now() + TARGET_WAIT;

    loop {
        tracing::info!("[target] waiting for ChatGPT...");

        if let Some(page) = find_chatgpt_page(browser).await? {
            if let Some(url) = page
                .url()
                .await
                .context("failed to read ChatGPT page URL")?
            {
                tracing::info!(%url, "[target] ChatGPT found");
            } else {
                tracing::info!("[target] ChatGPT found");
            }
            return Ok(page);
        }

        if Instant::now() >= deadline {
            bail!("timed out waiting for ChatGPT page");
        }

        tokio::time::sleep(TARGET_POLL).await;
    }
}

async fn find_chatgpt_page(browser: &Browser) -> anyhow::Result<Option<Page>> {
    let pages = browser
        .pages()
        .await
        .context("failed to list browser pages")?;

    for page in pages {
        match page.url().await {
            Ok(Some(url)) if is_chatgpt_url(&url) => return Ok(Some(page)),
            Ok(_) => {}
            Err(error) => {
                tracing::debug!(%error, "skipped page while discovering ChatGPT target");
            }
        }
    }

    Ok(None)
}

fn is_chatgpt_url(url: &str) -> bool {
    let rest = url
        .strip_prefix("https://chatgpt.com")
        .or_else(|| url.strip_prefix("http://chatgpt.com"));
    matches!(rest, Some(path) if path.is_empty() || path.starts_with('/'))
}

async fn evaluate_string(page: &Page, expression: &str) -> anyhow::Result<String> {
    page.evaluate(expression)
        .await
        .with_context(|| format!("failed to evaluate {expression}"))?
        .into_value()
        .with_context(|| format!("failed to decode {expression} as string"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{is_chatgpt_url, profile_dir};

    #[test]
    fn chatgpt_urls_match_expected_hosts() {
        assert!(is_chatgpt_url("https://chatgpt.com"));
        assert!(is_chatgpt_url("https://chatgpt.com/"));
        assert!(is_chatgpt_url("https://chatgpt.com/auth/login"));
        assert!(!is_chatgpt_url("https://example.com"));
        assert!(!is_chatgpt_url("https://chatgpt.com.evil.example/"));
    }

    #[test]
    fn profile_is_under_home_b2t() {
        let profile = profile_dir().expect("home directory should exist in tests");
        assert!(profile.ends_with(Path::new(".b2t").join("chrome-profile")));
    }
}
