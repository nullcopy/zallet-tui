//! Typed JSON-RPC client for the TUI.
//!
//! This wraps a [`jsonrpsee`] HTTP client with one method per wallet RPC the TUI uses,
//! deserializing responses into purpose-built structs.

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64ct::{Base64, Encoding};
use hyper::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use jsonrpsee::core::{client::ClientT, params::ArrayParams};
use jsonrpsee::rpc_params;
use jsonrpsee_http_client::{HttpClient, HttpClientBuilder};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde::de::IgnoredAny;
use serde_json::{Value, json};

use crate::config::RpcAuth;

/// Errors that can arise when building or using the TUI's RPC client.
#[derive(Debug)]
pub(crate) enum ClientError {
    Build(String),
    Request(String),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientError::Build(e) => {
                write!(f, "{}", crate::fl!("tui-err-build-client", error = e))
            }
            ClientError::Request(e) => {
                write!(f, "{}", crate::fl!("tui-err-request", error = e))
            }
        }
    }
}

impl std::error::Error for ClientError {}

/// A JSON-RPC error returned by the wallet, including its numeric code.
///
/// The code matches the `zcashd`-compatible `LegacyCode` values (e.g. `-13` for
/// "wallet needs unlocking").
#[derive(Clone, Debug)]
pub(crate) struct RpcError {
    pub(crate) code: i32,
    pub(crate) message: String,
}

impl RpcError {
    /// Whether this error indicates the wallet must be unlocked before the operation can
    /// proceed (`LegacyCode::WalletUnlockNeeded`).
    pub(crate) fn is_unlock_needed(&self) -> bool {
        self.code == -13
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            crate::fl!(
                "tui-err-rpc-with-code",
                message = self.message.clone(),
                code = self.code
            )
        )
    }
}

/// The result of a wallet RPC call: either a typed value, or a structured wallet error.
pub(crate) type CallResult<T> = Result<Result<T, RpcError>, ClientError>;

/// A typed JSON-RPC client for the wallet.
#[derive(Clone)]
pub(crate) struct WalletClient {
    inner: HttpClient,
}

impl WalletClient {
    /// Connects to a remote `zallet start` server at the given RPC URL.
    ///
    /// The URL may include a scheme (`http://` or `https://`); if it doesn't, `http://` is
    /// assumed (so a bare `host:port` works). Credentials are taken from the first
    /// configured `[[rpc.auth]]` entry with a password, or from a `user:pass@` userinfo
    /// component embedded in the URL.
    ///
    /// Authentication is sent via an HTTP `Authorization: Basic` header. Zallet's RPC
    /// server reads credentials from this header and ignores any userinfo in the URL, so
    /// the credential must not be left embedded in the URL (doing so results in a `401`).
    pub(crate) fn connect_remote(
        rpc_url: &str,
        auth: &[RpcAuth],
        timeout: Duration,
    ) -> Result<Self, ClientError> {
        // Split off an explicit scheme, defaulting to http.
        let (scheme, rest) = match rpc_url.split_once("://") {
            Some((scheme, rest)) => (scheme, rest),
            None => ("http", rpc_url),
        };

        // Extract credentials embedded in the URL (`user:pass@host`), if any, and strip
        // them from the host portion. jsonrpsee does not turn URL userinfo into an
        // `Authorization` header, so we must do that ourselves below.
        //
        // Split on the *last* `@`: a host cannot contain `@`, so anything before the final
        // one is userinfo. This keeps passwords that themselves contain `@` intact.
        let (embedded_credential, host) = match rest.rsplit_once('@') {
            Some((credential, host)) => (Some(SecretString::new(credential.to_string())), host),
            None => (None, rest),
        };

        // Prefer an explicit credential from the URL; otherwise use the first configured
        // auth entry that has a password.
        let credential = embedded_credential.or_else(|| {
            auth.iter().find_map(|a| {
                a.password
                    .as_ref()
                    .map(|pw| SecretString::new(format!("{}:{}", a.user, pw.expose_secret())))
            })
        });

        let url = format!("{scheme}://{host}");
        Self::build(&url, credential.as_ref(), timeout)
    }

    /// Builds the underlying HTTP client for `url`, optionally attaching an
    /// `Authorization: Basic` header carrying `credential` (a `user:password` string).
    fn build(
        url: &str,
        credential: Option<&SecretString>,
        timeout: Duration,
    ) -> Result<Self, ClientError> {
        let mut builder = HttpClientBuilder::default().request_timeout(timeout);

        if let Some(credential) = credential {
            let encoded = Base64::encode_string(credential.expose_secret().as_bytes());
            let mut value = HeaderValue::from_str(&format!("Basic {encoded}"))
                .map_err(|e| ClientError::Build(e.to_string()))?;
            value.set_sensitive(true);
            let mut headers = HeaderMap::new();
            headers.insert(AUTHORIZATION, value);
            builder = builder.set_headers(headers);
        }

        let inner = builder
            .build(url)
            .map_err(|e| ClientError::Build(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Performs a raw request, mapping a jsonrpsee error into either a structured
    /// [`RpcError`] (for wallet-level call errors) or a transport [`ClientError`].
    ///
    /// Failures are logged with the method name, error code, and message so they can be
    /// diagnosed from the log file. The parameters are deliberately **not** logged: they
    /// may carry secrets such as the wallet passphrase or a transaction memo.
    async fn request<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: ArrayParams,
    ) -> CallResult<T> {
        match self.inner.request::<T, _>(method, params).await {
            Ok(value) => Ok(Ok(value)),
            Err(jsonrpsee::core::client::Error::Call(err)) => {
                tracing::warn!(
                    method,
                    code = err.code(),
                    message = err.message(),
                    "wallet RPC returned an error"
                );
                Ok(Err(RpcError {
                    code: err.code(),
                    message: err.message().to_string(),
                }))
            }
            Err(other) => {
                tracing::error!(method, error = %other, "RPC transport error");
                Err(ClientError::Request(other.to_string()))
            }
        }
    }

    // --- Status & balances ------------------------------------------------------------

    pub(crate) async fn get_wallet_status(&self) -> CallResult<WalletStatus> {
        self.request("getwalletstatus", ArrayParams::new()).await
    }

    pub(crate) async fn get_wallet_info(&self) -> CallResult<WalletInfo> {
        self.request("getwalletinfo", ArrayParams::new()).await
    }

    pub(crate) async fn get_balances(&self, minconf: u32) -> CallResult<Balances> {
        self.request("z_getbalances", rpc_params![minconf]).await
    }

    pub(crate) async fn get_total_balance(&self, minconf: u32) -> CallResult<TotalBalance> {
        // `z_gettotalbalance` currently requires `include_watchonly = true`.
        self.request("z_gettotalbalance", rpc_params![minconf, true])
            .await
    }

    // --- Accounts ---------------------------------------------------------------------

    pub(crate) async fn list_accounts(&self) -> CallResult<Vec<Account>> {
        self.request("z_listaccounts", rpc_params![true]).await
    }

    pub(crate) async fn new_account(&self, name: &str) -> CallResult<IgnoredAny> {
        self.request("z_getnewaccount", rpc_params![name]).await
    }

    pub(crate) async fn new_address_for_account(
        &self,
        account_uuid: &str,
    ) -> CallResult<IgnoredAny> {
        self.request("z_getaddressforaccount", rpc_params![account_uuid])
            .await
    }

    // --- Transactions -----------------------------------------------------------------

    pub(crate) async fn list_transactions(
        &self,
        offset: u32,
        limit: u32,
    ) -> CallResult<Vec<WalletTx>> {
        // The leading three nulls are the account_uuid, start_height, and end_height
        // filters; null selects all transactions.
        self.request(
            "z_listtransactions",
            rpc_params![Value::Null, Value::Null, Value::Null, offset, limit],
        )
        .await
    }

    // --- Wallet lock ------------------------------------------------------------------

    pub(crate) async fn unlock(&self, passphrase: &str, timeout: u64) -> CallResult<IgnoredAny> {
        // `walletpassphrase` returns null on success.
        self.request("walletpassphrase", rpc_params![passphrase, timeout])
            .await
    }

    pub(crate) async fn lock(&self) -> CallResult<IgnoredAny> {
        self.request("walletlock", ArrayParams::new()).await
    }

    // --- Sending ----------------------------------------------------------------------

    /// Submits a `z_sendmany`, returning the async operation id.
    pub(crate) async fn send_many(
        &self,
        from: &str,
        to: &str,
        amount: &str,
        memo: Option<&str>,
        privacy_policy: &str,
    ) -> CallResult<String> {
        let mut recipient = serde_json::Map::new();
        recipient.insert("address".into(), json!(to));
        recipient.insert("amount".into(), json!(amount));
        if let Some(memo) = memo {
            // The wallet expects the memo as hex-encoded bytes, but the form captures it as
            // human-readable text, so encode the UTF-8 bytes here.
            recipient.insert(
                "memo".into(),
                json!(crate::format::hex_encode(memo.as_bytes())),
            );
        }
        let recipients = Value::Array(vec![Value::Object(recipient)]);

        // from, [recipient], minconf = 1, fee = null (ZIP-317 automatic), privacy_policy.
        self.request(
            "z_sendmany",
            rpc_params![from, recipients, 1, Value::Null, privacy_policy],
        )
        .await
    }

    /// Polls the status of one async operation.
    pub(crate) async fn operation_status(&self, opid: &str) -> CallResult<Vec<OperationStatus>> {
        self.request("z_getoperationstatus", rpc_params![[opid]])
            .await
    }
}

// --- Response types -------------------------------------------------------------------
//
// These mirror the JSON shapes produced by the wallet RPC. They are intentionally
// tolerant: unknown fields are ignored, and optional fields are modelled as `Option`.

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct WalletStatus {
    pub(crate) node_tip: ChainTip,
    #[serde(default)]
    pub(crate) wallet_tip: Option<ChainTip>,
    #[serde(default)]
    pub(crate) fully_synced_height: Option<u32>,
    #[serde(default)]
    pub(crate) sync_work_remaining: Option<SyncWorkRemaining>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ChainTip {
    pub(crate) blockhash: String,
    pub(crate) height: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct SyncWorkRemaining {
    pub(crate) unscanned_blocks: u32,
    pub(crate) progress: Progress,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Progress {
    pub(crate) numerator: u64,
    pub(crate) denominator: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct TotalBalance {
    pub(crate) transparent: String,
    pub(crate) private: String,
    pub(crate) total: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Balances {
    #[serde(default)]
    pub(crate) accounts: Vec<AccountBalance>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct AccountBalance {
    pub(crate) account_uuid: String,
    #[serde(default)]
    pub(crate) transparent: Option<PoolBalance>,
    #[serde(default)]
    pub(crate) sapling: Option<PoolBalance>,
    #[serde(default)]
    pub(crate) orchard: Option<PoolBalance>,
    #[serde(default)]
    pub(crate) total: Option<PoolBalance>,
}

impl AccountBalance {
    /// Spendable balance in the transparent pool, in zatoshi.
    pub(crate) fn transparent_zat(&self) -> i64 {
        spendable(&self.transparent)
    }
    /// Spendable balance in the Sapling pool, in zatoshi.
    pub(crate) fn sapling_zat(&self) -> i64 {
        spendable(&self.sapling)
    }
    /// Spendable balance in the Orchard pool, in zatoshi.
    pub(crate) fn orchard_zat(&self) -> i64 {
        spendable(&self.orchard)
    }

    /// The account's not-yet-spendable (pending) balance across all pools, in zatoshi.
    pub(crate) fn pending_total_zat(&self) -> i64 {
        self.total.as_ref().map_or(0, PoolBalance::pending_zat)
    }
    /// The account's grand total balance across all pools and categories, in zatoshi.
    pub(crate) fn total_zat(&self) -> i64 {
        self.total.as_ref().map_or(0, PoolBalance::total_zat)
    }
}

/// The spendable zatoshi in a pool, or `0` if the pool is absent.
fn spendable(pool: &Option<PoolBalance>) -> i64 {
    pool.as_ref().map_or(0, PoolBalance::spendable_zat)
}

/// A per-pool (or whole-account `total`) balance, keyed by spendability category.
///
/// `z_getbalances` reports each pool as a map of category to amount, e.g.
/// `{"spendable": {"valueZat": 100}, "pending": {"valueZat": 40}}`, omitting empty pools.
/// Capturing the categories in a map (rather than naming them) means new categories are
/// handled automatically: anything that is not `spendable` counts as pending.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct PoolBalance(std::collections::BTreeMap<String, ZatAmount>);

impl PoolBalance {
    /// The name of the one spendable category; every other category is pending.
    const SPENDABLE: &'static str = "spendable";

    /// The spendable balance in zatoshi (`0` if not reported).
    pub(crate) fn spendable_zat(&self) -> i64 {
        self.0.get(Self::SPENDABLE).map_or(0, |a| a.value_zat)
    }

    /// The sum of all not-yet-spendable categories (pending, immature, ...), in zatoshi.
    pub(crate) fn pending_zat(&self) -> i64 {
        self.0
            .iter()
            .filter(|(category, _)| category.as_str() != Self::SPENDABLE)
            .map(|(_, amount)| amount.value_zat)
            .sum()
    }

    /// The sum of every category (spendable and otherwise), in zatoshi.
    pub(crate) fn total_zat(&self) -> i64 {
        self.0.values().map(|amount| amount.value_zat).sum()
    }
}

/// An amount denominated in zatoshi, as reported under a balance category.
#[derive(Clone, Copy, Debug, Deserialize)]
pub(crate) struct ZatAmount {
    #[serde(rename = "valueZat")]
    pub(crate) value_zat: i64,
}

/// The wallet's encryption and lock state, derived from `getwalletinfo`.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct WalletInfo {
    /// The timestamp (seconds since epoch) until which the wallet is unlocked, or `0` if
    /// the wallet is encrypted but currently locked.
    ///
    /// This field is absent entirely when the wallet is unencrypted.
    #[serde(default)]
    pub(crate) unlocked_until: Option<u64>,
}

impl WalletInfo {
    /// The wallet's encryption/lock state.
    pub(crate) fn lock_state(&self) -> LockState {
        match self.unlocked_until {
            None => LockState::Unencrypted,
            Some(0) => LockState::Locked,
            // `unlocked_until` is a Unix timestamp. Treat an unlock that has already
            // elapsed as locked, rather than trusting a stale future-looking value the
            // backend has not yet reset to `0`.
            Some(until) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    // If the clock is unreadable, stay conservative and assume locked.
                    .unwrap_or(u64::MAX);
                if until > now {
                    LockState::Unlocked
                } else {
                    LockState::Locked
                }
            }
        }
    }
}

/// The encryption/lock state of the wallet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LockState {
    /// The wallet is not encrypted; there is no passphrase and nothing to unlock.
    Unencrypted,
    /// The wallet is encrypted and currently locked. Spending requires unlocking.
    Locked,
    /// The wallet is encrypted and currently unlocked.
    Unlocked,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Account {
    pub(crate) account_uuid: String,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) addresses: Vec<AccountAddress>,
}

impl Account {
    /// A human-readable label for this account: its name, or a short UUID prefix.
    pub(crate) fn label(&self) -> String {
        match &self.name {
            Some(name) if !name.is_empty() => name.clone(),
            _ => format!("({})", crate::format::short_uuid(&self.account_uuid)),
        }
    }

    /// Returns a unified address owned by this account, suitable for use as a `z_sendmany`
    /// source (`fromaddress`). `z_sendmany` does not accept account UUIDs, so the selected
    /// account must be resolved to one of its addresses.
    pub(crate) fn spend_source_address(&self) -> Option<&str> {
        self.addresses
            .iter()
            .find_map(|a| a.ua.as_deref())
            .or_else(|| self.addresses.iter().find_map(|a| a.sapling.as_deref()))
            .or_else(|| self.addresses.iter().find_map(|a| a.transparent.as_deref()))
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct AccountAddress {
    #[serde(default)]
    pub(crate) ua: Option<String>,
    #[serde(default)]
    pub(crate) sapling: Option<String>,
    #[serde(default)]
    pub(crate) transparent: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct WalletTx {
    #[serde(default)]
    pub(crate) account_uuid: Option<String>,
    #[serde(default)]
    pub(crate) mined_height: Option<u32>,
    pub(crate) txid: String,
    pub(crate) account_balance_delta: i64,
    #[serde(default)]
    pub(crate) fee_paid: Option<i64>,
    #[serde(default)]
    pub(crate) block_time: Option<i64>,
    #[serde(default)]
    pub(crate) expired_unmined: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct OperationStatus {
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) error: Option<OperationError>,
    #[serde(default)]
    pub(crate) result: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct OperationError {
    #[serde(default)]
    pub(crate) message: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    use base64ct::{Base64, Encoding};
    use secrecy::SecretString;

    use super::*;
    use crate::config::RpcAuth;

    /// Spawns a one-shot HTTP server that captures the first request's `Authorization`
    /// header, replies with a minimal JSON-RPC result, and returns the captured header.
    fn capture_one_request(listener: TcpListener) -> std::thread::JoinHandle<Option<String>> {
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).expect("read");
            let req = String::from_utf8_lossy(&buf[..n]).to_string();

            let body = br#"{"jsonrpc":"2.0","id":0,"result":{"unlocked_until":null}}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(resp.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
            stream.flush().unwrap();

            req.lines()
                .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
                .map(|l| l["authorization:".len()..].trim().to_string())
        })
    }

    #[tokio::test]
    async fn configured_auth_is_sent_as_basic_header() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = capture_one_request(listener);

        let auth = vec![RpcAuth {
            user: "zallet".to_string(),
            password: Some(SecretString::new("hunter2".to_string())),
        }];
        let client =
            WalletClient::connect_remote(&format!("http://{addr}"), &auth, Duration::from_secs(5))
                .expect("build client");
        let _ = client.get_wallet_info().await;

        let header = handle
            .join()
            .unwrap()
            .expect("Authorization header present");
        let expected = format!("Basic {}", Base64::encode_string(b"zallet:hunter2"));
        assert_eq!(header, expected);
    }

    #[tokio::test]
    async fn url_embedded_credential_is_sent_as_basic_header() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = capture_one_request(listener);

        // Credentials embedded in the URL must be moved into the Authorization header,
        // not left in the URL (which Zallet ignores, causing a 401).
        let url = format!("http://zallet:hunter2@{addr}");
        let client =
            WalletClient::connect_remote(&url, &[], Duration::from_secs(5)).expect("build client");
        let _ = client.get_wallet_info().await;

        let header = handle
            .join()
            .unwrap()
            .expect("Authorization header present");
        let expected = format!("Basic {}", Base64::encode_string(b"zallet:hunter2"));
        assert_eq!(header, expected);
    }

    #[tokio::test]
    async fn no_auth_sends_no_header() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = capture_one_request(listener);

        let client =
            WalletClient::connect_remote(&format!("http://{addr}"), &[], Duration::from_secs(5))
                .expect("build client");
        let _ = client.get_wallet_info().await;

        assert_eq!(handle.join().unwrap(), None);
    }

    /// Regression test against a real `z_getbalances` response: pools are nested objects
    /// (`{"spendable": {"valueZat": N}}`) and absent when empty.
    #[test]
    fn balances_deserialize_from_real_response() {
        let json = r#"{"accounts":[
            {"account_uuid":"4e7543e5-1c82-4b5b-8c7e-1113383b763e","total":{"spendable":{"valueZat":0}}},
            {"account_uuid":"5f24cb93-dcbc-4915-b815-c6ddfa018b86","orchard":{"spendable":{"valueZat":10000000000}},"total":{"spendable":{"valueZat":10000000000}}}
        ]}"#;

        let balances: Balances = serde_json::from_str(json).expect("parse z_getbalances response");
        assert_eq!(balances.accounts.len(), 2);

        let empty = &balances.accounts[0];
        assert_eq!(empty.transparent_zat(), 0);
        assert_eq!(empty.orchard_zat(), 0);
        assert_eq!(empty.total_zat(), 0);

        let funded = &balances.accounts[1];
        assert_eq!(funded.orchard_zat(), 10_000_000_000);
        assert_eq!(funded.total_zat(), 10_000_000_000);
        assert_eq!(funded.transparent_zat(), 0);
        assert_eq!(funded.sapling_zat(), 0);
    }

    /// Non-`spendable` categories are summed into the pending total, and the grand total
    /// includes them. The exact category name does not matter — anything but `spendable`
    /// counts as pending.
    #[test]
    fn balances_account_separates_spendable_and_pending() {
        let json = r#"{"accounts":[{
            "account_uuid":"a",
            "orchard":{"spendable":{"valueZat":100},"pending":{"valueZat":40}},
            "total":{"spendable":{"valueZat":100},"pending":{"valueZat":40}}
        }]}"#;

        let balances: Balances = serde_json::from_str(json).expect("parse balances with pending");
        let account = &balances.accounts[0];

        assert_eq!(account.orchard_zat(), 100); // pool columns show spendable only
        assert_eq!(account.pending_total_zat(), 40);
        assert_eq!(account.total_zat(), 140); // spendable + pending
    }
}
