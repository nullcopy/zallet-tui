//! Configuration and connection resolution for `zallet-tui`.
//!
//! `zallet-tui` is a pure JSON-RPC client: it connects to an already-running `zallet
//! start` instance over HTTP.
//!
//! The endpoint is an explicit URL (`--rpc-url` or config `rpc_url`), defaulting to the
//! conventional local RPC endpoint. Authentication uses a configured `[[rpc.auth]]`
//! user/password, supplied via `--rpc-user`/`--rpc-password` or `[[auth]]` entries in the
//! `zallet-tui.toml` config file; CLI flags take precedence over the config file.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use secrecy::SecretString;
use serde::Deserialize;

use crate::cli::TuiCmd;
use crate::client::WalletClient;

/// The default client request timeout, in seconds.
const DEFAULT_HTTP_CLIENT_TIMEOUT: u64 = 900;

/// The conventional local Zallet JSON-RPC endpoint, used when no URL is configured.
const DEFAULT_RPC_URL: &str = "http://127.0.0.1:8232";

/// A single RPC authentication credential, mirroring Zallet's `[[rpc.auth]]` entries
/// closely enough for the client's needs.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RpcAuth {
    /// The username.
    pub(crate) user: String,
    /// The password, if any. Stored as a secret so it is not accidentally logged.
    #[serde(default)]
    pub(crate) password: Option<SecretString>,
}

/// The on-disk `zallet-tui.toml` configuration.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    /// The URL of the remote `zallet start` JSON-RPC server (e.g. `http://127.0.0.1:8232`).
    #[serde(default)]
    pub(crate) rpc_url: Option<String>,

    /// Authentication credentials for the RPC server.
    #[serde(default)]
    pub(crate) auth: Vec<RpcAuth>,

    /// Path to a Zallet RPC cookie file (`__cookie__:<password>`) to authenticate with.
    ///
    /// Takes precedence over the `[[auth]]` entries; re-read on every connection attempt
    /// because Zallet rewrites the cookie on each startup.
    #[serde(default)]
    pub(crate) cookie_file: Option<PathBuf>,

    /// The client request timeout, in seconds (`0` means no timeout).
    #[serde(default)]
    pub(crate) timeout: Option<u64>,
}

impl Config {
    /// Loads configuration from the given path, or from the platform default location if
    /// `path` is `None`. A missing default file is not an error (returns the default
    /// config); a missing explicitly-requested file is.
    pub(crate) fn load(path: Option<&Path>) -> Result<Self> {
        match path {
            Some(path) => Self::read(path),
            None => match default_config_path() {
                Some(path) if path.exists() => Self::read(&path),
                _ => Ok(Config::default()),
            },
        }
    }

    /// Reads and parses a config file that is expected to exist.
    fn read(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        toml::from_str(&contents)
            .with_context(|| format!("failed to parse config file {}", path.display()))
    }
}

/// A fully-resolved connection specification, ready to build a [`WalletClient`].
pub(crate) struct Connection {
    url: String,
    auth: Vec<RpcAuth>,
    /// A cookie file to read credentials from, taking precedence over `auth`.
    cookie_file: Option<PathBuf>,
    timeout: Duration,
}

impl Connection {
    /// Resolves the connection to use from CLI flags and the loaded config file.
    ///
    /// The endpoint is an explicit URL (`--rpc-url` or config `rpc_url`), defaulting to the
    /// conventional local RPC endpoint. Authentication uses a configured `[[rpc.auth]]`
    /// user/password: `--rpc-user`/`--rpc-password` take precedence over the `[[auth]]`
    /// entries in the config file.
    pub(crate) fn resolve(cmd: &TuiCmd, config: &Config) -> Self {
        let timeout = Duration::from_secs(match cmd.timeout.or(config.timeout) {
            Some(0) => u64::MAX,
            Some(t) => t,
            None => DEFAULT_HTTP_CLIENT_TIMEOUT,
        });

        let url = cmd
            .rpc_url
            .clone()
            .or_else(|| config.rpc_url.clone())
            .unwrap_or_else(|| DEFAULT_RPC_URL.to_string());

        // A `--rpc-user`/`--rpc-password` pair on the command line overrides the configured
        // `[[auth]]` credentials entirely — including any cookie file. Without one, a
        // cookie file (CLI over config) is preferred over `[[auth]]` entries.
        let (auth, cookie_file) = match &cmd.rpc_user {
            Some(user) => (
                vec![RpcAuth {
                    user: user.clone(),
                    password: cmd.rpc_password.clone().map(SecretString::new),
                }],
                None,
            ),
            None => (
                config.auth.clone(),
                cmd.cookie_file
                    .clone()
                    .or_else(|| config.cookie_file.clone()),
            ),
        };

        Connection {
            url,
            auth,
            cookie_file,
            timeout,
        }
    }

    /// Builds a connected [`WalletClient`] for this connection.
    ///
    /// The cookie file (when configured) is read here, at connection time, because Zallet
    /// rewrites it with a fresh credential on every startup.
    pub(crate) fn connect(&self) -> Result<WalletClient> {
        let cookie_auth = match &self.cookie_file {
            Some(path) => Some(read_cookie(path)?),
            None => None,
        };
        let auth = match &cookie_auth {
            Some(auth) => std::slice::from_ref(auth),
            None => &self.auth,
        };
        Ok(WalletClient::connect_remote(&self.url, auth, self.timeout)?)
    }
}

/// Reads an RPC credential from a Zallet cookie file.
fn read_cookie(path: &Path) -> Result<RpcAuth> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read cookie file {}", path.display()))?;
    parse_cookie(&contents)
        .with_context(|| format!("cookie file {} is not `user:password`", path.display()))
}

/// Parses the `user:password` line of an RPC cookie file.
///
/// The password may itself contain `:`; only the first separator delimits the user. Empty
/// users or passwords are rejected — Zallet itself refuses empty RPC passwords.
fn parse_cookie(contents: &str) -> Option<RpcAuth> {
    let line = contents.lines().next()?.trim();
    let (user, password) = line.split_once(':')?;
    if user.is_empty() || password.is_empty() {
        return None;
    }
    Some(RpcAuth {
        user: user.to_string(),
        password: Some(SecretString::new(password.to_string())),
    })
}

/// The platform-default location for `zallet-tui.toml`.
pub(crate) fn default_config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("zallet-tui").join("zallet-tui.toml"))
}

/// A commented default `zallet-tui.toml`, with every setting present but disabled so the
/// out-of-the-box behaviour (connecting to the local RPC endpoint without credentials) is
/// unchanged.
const DEFAULT_CONFIG_TEMPLATE: &str = "\
# Configuration for zallet-tui.
#
# zallet-tui connects to an already-running `zallet start` instance. With every option
# below left commented out, it connects to the default local RPC endpoint without
# credentials. Uncomment and edit the options you need.

# The URL of the Zallet JSON-RPC server. Defaults to http://127.0.0.1:8232.
# rpc_url = \"http://127.0.0.1:8232\"

# Client request timeout, in seconds. Use 0 for no timeout. Defaults to 900.
# timeout = 900

# Path to a Zallet RPC cookie file. Zallet writes a fresh `.cookie` (containing
# `__cookie__:<password>`) into its data directory on every startup; pointing at it here
# avoids configuring a `[[rpc.auth]]` user by hand. Takes precedence over `[[auth]]`.
# cookie_file = \"/path/to/zallet/datadir/.cookie\"

# RPC authentication credentials. These must match a `[[rpc.auth]]` user configured in the
# Zallet server's own config. Repeat the `[[auth]]` block for multiple credentials.
# [[auth]]
# user = \"zallet\"
# password = \"your-rpc-password\"
";

/// Writes a default `zallet-tui.toml` to `output` (or the platform default location), and
/// returns the path written.
///
/// Refuses to overwrite an existing file unless `force` is set.
pub(crate) fn generate_default(output: Option<&Path>, force: bool) -> Result<PathBuf> {
    let path = match output {
        Some(path) => path.to_path_buf(),
        None => default_config_path().context(
            "could not determine a default configuration path; pass --output to choose one",
        )?,
    };

    if path.exists() && !force {
        bail!(
            "{} already exists; pass --force to overwrite it",
            path.display()
        );
    }

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory {}", parent.display()))?;
    }

    std::fs::write(&path, DEFAULT_CONFIG_TEMPLATE)
        .with_context(|| format!("failed to write config file {}", path.display()))?;

    Ok(path)
}

/// The platform configuration directory.
fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        home::home_dir().map(|h| h.join("Library").join("Application Support"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home::home_dir().map(|h| h.join(".config")))
    }
}

/// The platform directory for application log files.
fn log_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
            .map(|d| d.join("zallet-tui").join("logs"))
    }
    #[cfg(target_os = "macos")]
    {
        home::home_dir().map(|h| h.join("Library").join("Logs").join("zallet-tui"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        // Follows the XDG Base Directory spec: logs are state data.
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| home::home_dir().map(|h| h.join(".local").join("state")))
            .map(|d| d.join("zallet-tui"))
    }
}

/// The platform-default path for the client log file.
///
/// Falls back to `zallet-tui.log` in the current directory if no suitable platform
/// directory can be determined.
pub(crate) fn default_log_path() -> PathBuf {
    match log_dir() {
        Some(dir) => dir.join("zallet-tui.log"),
        None => PathBuf::from("zallet-tui.log"),
    }
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret as _;

    use super::parse_cookie;

    #[test]
    fn cookie_parses_user_and_password() {
        let auth = parse_cookie("__cookie__:sekrit\n").expect("valid cookie");
        assert_eq!(auth.user, "__cookie__");
        assert_eq!(auth.password.unwrap().expose_secret(), "sekrit");
    }

    #[test]
    fn cookie_password_may_contain_separators() {
        let auth = parse_cookie("__cookie__:se:kr:it").expect("valid cookie");
        assert_eq!(auth.password.unwrap().expose_secret(), "se:kr:it");
    }

    #[test]
    fn malformed_or_empty_cookies_are_rejected() {
        for contents in ["", "\n", "nocolon", ":password", "user:", ":"] {
            assert!(parse_cookie(contents).is_none(), "{contents:?}");
        }
    }
}
