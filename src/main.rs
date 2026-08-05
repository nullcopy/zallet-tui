//! `zallet-tui`: an interactive terminal UI for the Zallet Zcash wallet.
//!
//! The TUI is fundamentally a JSON-RPC client. It connects to an already-running `zallet
//! start` instance over HTTP and drives the wallet exclusively through the JSON-RPC
//! interface.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use clap::Parser;
use unic_langid::LanguageIdentifier;

mod app;
mod cli;
mod client;
mod config;
mod event;
mod format;
mod i18n;
mod qr;
mod terminal;
mod ui;
mod views;

use cli::{Cli, Command, GenerateConfigCmd, TuiCmd};
use config::{Config, Connection};

/// A macro to obtain localized messages, checking the `message_id` and arguments at
/// compile time.
///
/// See [`i18n_embed_fl::fl`] for full documentation.
#[macro_export]
macro_rules! fl {
    ($message_id:literal) => {{
        i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id)
    }};

    ($message_id:literal, $($args:expr),* $(,)?) => {{
        i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id, $($args), *)
    }};
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Command::GenerateConfig(cmd)) => generate_config(cmd),
        Some(Command::Check) => check(cli.tui),
        None => run(cli.tui),
    };

    if let Err(e) = result {
        // The terminal has been restored by this point (see `run_ui`), so it is safe to
        // print the error to standard error. `{e:#}` renders the full anyhow context chain
        // (e.g. "failed to read config file X: <io error>").
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

/// Writes a default configuration file and reports where it was written.
fn generate_config(cmd: GenerateConfigCmd) -> Result<()> {
    let path = config::generate_default(cmd.output.as_deref(), cmd.force)?;
    println!("Wrote default configuration to {}", path.display());
    Ok(())
}

/// Checks that the server exposes every RPC method the TUI uses, via `rpc.discover`.
fn check(cmd: TuiCmd) -> Result<()> {
    // Error `Display` impls go through the localization loader, so it must be initialized
    // even though this path prints plain output.
    i18n::load_languages(&requested_languages());

    let config = Config::load(cmd.config.as_deref())?;
    let connection = Connection::resolve(&cmd, &config);
    let client = connection.connect()?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to build the Tokio runtime")?;
    let schema = match runtime.block_on(client.discover())? {
        Ok(schema) => schema,
        Err(e) => anyhow::bail!("rpc.discover failed: {e}"),
    };

    let missing = missing_methods(&schema);
    if missing.is_empty() {
        println!(
            "OK: the server exposes all {} RPC methods zallet-tui uses (of {} methods served).",
            client::REQUIRED_METHODS.len(),
            schema.methods.len()
        );
        Ok(())
    } else {
        for method in &missing {
            println!("MISSING: {method}");
        }
        anyhow::bail!(
            "the server is missing {} of the {} RPC methods zallet-tui uses",
            missing.len(),
            client::REQUIRED_METHODS.len()
        );
    }
}

/// The [`client::REQUIRED_METHODS`] that `schema` does not serve, in the order required.
fn missing_methods(schema: &client::OpenRpcSchema) -> Vec<&'static str> {
    let available: std::collections::BTreeSet<&str> =
        schema.methods.iter().map(|m| m.name.as_str()).collect();
    client::REQUIRED_METHODS
        .iter()
        .copied()
        .filter(|method| !available.contains(method))
        .collect()
}

fn run(cmd: TuiCmd) -> Result<()> {
    // Negotiate the UI language from the environment.
    let requested = requested_languages();
    i18n::load_languages(&requested);

    // The TUI takes over the terminal; divert log output to a file so it does not corrupt
    // the display. Defaults to a platform-specific log directory.
    let log_path = cmd
        .log_file
        .clone()
        .unwrap_or_else(config::default_log_path);
    init_logging(&log_path)?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "zallet-tui starting");

    // Resolve and build the client connection.
    let config = Config::load(cmd.config.as_deref())?;
    let connection = Connection::resolve(&cmd, &config);
    let client = connection.connect()?;

    // Run the async UI on a Tokio runtime.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build the Tokio runtime")?;

    runtime.block_on(run_ui(client, Some(log_path)))
}

/// Runs the terminal UI event loop against the given wallet client.
///
/// The terminal is placed into raw mode and the alternate screen on entry, and restored on
/// exit (including on panic, via [`terminal::TerminalGuard`]).
async fn run_ui(client: client::WalletClient, log_path: Option<PathBuf>) -> Result<()> {
    let mut guard =
        terminal::TerminalGuard::enter().context("failed to initialize the terminal")?;

    let mut app = app::App::new(client, log_path);
    let result = app.run(guard.terminal_mut()).await;

    // Restore the terminal before returning, so any error is printed cleanly.
    guard.restore();

    result
}

/// Initializes file-based tracing. The TUI cannot log to standard error without corrupting
/// its display, so all log output goes to `log_path`.
fn init_logging(log_path: &Path) -> Result<()> {
    use tracing_subscriber::EnvFilter;

    // Ensure the log directory exists (the platform default may not have been created yet).
    if let Some(parent) = log_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create log directory {}", parent.display()))?;
    }

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .with_context(|| format!("failed to open log file {}", log_path.display()))?;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(file)
        .with_ansi(false)
        .init();

    Ok(())
}

/// Reads the user's preferred languages from the environment.
fn requested_languages() -> Vec<LanguageIdentifier> {
    // Honour the standard locale environment variables, falling back to en-US.
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(var)
            && !value.is_empty()
        {
            // Trim any encoding/modifier suffix, e.g. `en_US.UTF-8` -> `en-US`.
            let tag = value
                .split(['.', '@'])
                .next()
                .unwrap_or(&value)
                .replace('_', "-");
            if let Ok(lang) = tag.parse::<LanguageIdentifier>() {
                return vec![lang];
            }
        }
    }
    vec!["en-US".parse().expect("en-US is a valid language tag")]
}

#[cfg(test)]
mod tests {
    use super::missing_methods;
    use crate::client::{OpenRpcMethod, OpenRpcSchema, REQUIRED_METHODS};

    fn schema_of(names: &[&str]) -> OpenRpcSchema {
        OpenRpcSchema {
            methods: names
                .iter()
                .map(|n| OpenRpcMethod {
                    name: n.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn missing_methods_reports_exactly_the_unserved_methods() {
        // A server exposing everything (plus extras) has nothing missing.
        let mut served: Vec<&str> = REQUIRED_METHODS.to_vec();
        served.push("getinfo");
        assert!(missing_methods(&schema_of(&served)).is_empty());

        // Dropping methods reports precisely those, regardless of extras.
        served.retain(|m| *m != "z_sendfromaccount" && *m != "z_getbalances");
        assert_eq!(
            missing_methods(&schema_of(&served)),
            ["z_getbalances", "z_sendfromaccount"]
        );

        // An empty schema is missing everything.
        assert_eq!(missing_methods(&schema_of(&[])), REQUIRED_METHODS);
    }
}
