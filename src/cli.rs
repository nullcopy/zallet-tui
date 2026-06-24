//! Command-line interface for `zallet-tui`.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// An interactive terminal UI for the Zallet Zcash wallet.
///
/// `zallet-tui` is a JSON-RPC client: it connects to an already-running `zallet start`
/// instance. By default it connects to the local RPC endpoint; use `--rpc-url` to choose a
/// different endpoint, and `--rpc-user`/`--rpc-password` (or a `zallet-tui.toml` config
/// file) to authenticate against a server with configured `[[rpc.auth]]` credentials.
#[derive(Debug, Parser)]
#[command(name = "zallet-tui", version, about, long_about = None)]
pub(crate) struct Cli {
    /// An optional subcommand. When omitted, the interactive terminal UI is launched using
    /// the connection options below.
    #[command(subcommand)]
    pub(crate) command: Option<Command>,

    /// Connection and runtime options for the terminal UI.
    #[command(flatten)]
    pub(crate) tui: TuiCmd,
}

/// The available subcommands.
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Write a default `zallet-tui.toml` configuration file.
    GenerateConfig(GenerateConfigCmd),
}

/// Options for the interactive terminal UI (the default command).
#[derive(Debug, Parser)]
pub(crate) struct TuiCmd {
    /// The Zallet JSON-RPC server URL (e.g. `http://127.0.0.1:8232`).
    ///
    /// Defaults to `http://127.0.0.1:8232`.
    #[arg(long, value_name = "URL")]
    pub(crate) rpc_url: Option<String>,

    /// Username for JSON-RPC authentication (a configured `[[rpc.auth]]` user).
    #[arg(long, value_name = "USER")]
    pub(crate) rpc_user: Option<String>,

    /// Password for JSON-RPC authentication.
    #[arg(long, value_name = "PASSWORD")]
    pub(crate) rpc_password: Option<String>,

    /// Path to a `zallet-tui.toml` configuration file.
    #[arg(long, value_name = "PATH")]
    pub(crate) config: Option<PathBuf>,

    /// Client timeout in seconds for HTTP requests, or `0` for no timeout.
    ///
    /// Defaults to 900 seconds.
    #[arg(long)]
    pub(crate) timeout: Option<u64>,

    /// Path to write the client log file to.
    ///
    /// The TUI takes over the terminal, so log output is diverted to a file instead of
    /// standard error. Defaults to a `zallet-tui.log` in the platform log directory
    /// (e.g. `~/.local/state/zallet-tui/` on Linux, `~/Library/Logs/zallet-tui/` on macOS,
    /// `%LOCALAPPDATA%\zallet-tui\logs\` on Windows).
    #[arg(long, value_name = "PATH")]
    pub(crate) log_file: Option<PathBuf>,
}

/// Options for the `generate-config` subcommand.
#[derive(Debug, Parser)]
pub(crate) struct GenerateConfigCmd {
    /// Path to write the configuration file to.
    ///
    /// Defaults to the platform configuration directory
    /// (e.g. `~/.config/zallet-tui/zallet-tui.toml`).
    #[arg(long, value_name = "PATH")]
    pub(crate) output: Option<PathBuf>,

    /// Overwrite the file if it already exists.
    #[arg(long)]
    pub(crate) force: bool,
}
