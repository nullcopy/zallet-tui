//! TUI application state and event loop.

use std::time::Duration;

use anyhow::{Context as _, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::client::{
    Account, Balances, LockState, OperationStatus, TotalBalance, WalletClient, WalletStatus,
    WalletTx,
};
use crate::event::{Event, EventSource};
use crate::terminal::Tui;
use crate::views::accounts::AccountsState;
use crate::views::addresses::AddressesState;
use crate::views::transactions::TransactionsState;
use crate::{ui, views};

/// How often the UI refreshes wallet data and re-renders.
const TICK_RATE: Duration = Duration::from_secs(3);

/// The number of transactions fetched per page in the transactions view.
pub(crate) const TX_PAGE_SIZE: u32 = 50;

/// The primary views of the TUI.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum View {
    Dashboard,
    Accounts,
    Balances,
    Addresses,
    Transactions,
    Send,
    Logs,
}

impl View {
    /// All views, in tab order.
    pub(crate) const ALL: [View; 7] = [
        View::Dashboard,
        View::Accounts,
        View::Balances,
        View::Addresses,
        View::Transactions,
        View::Send,
        View::Logs,
    ];

    pub(crate) fn title(self) -> String {
        match self {
            View::Dashboard => crate::fl!("tui-view-dashboard"),
            View::Accounts => crate::fl!("tui-view-accounts"),
            View::Balances => crate::fl!("tui-view-balances"),
            View::Addresses => crate::fl!("tui-view-receive"),
            View::Transactions => crate::fl!("tui-view-transactions"),
            View::Send => crate::fl!("tui-view-send"),
            View::Logs => crate::fl!("tui-view-logs"),
        }
    }

    fn index(self) -> usize {
        View::ALL.iter().position(|&v| v == self).unwrap_or(0)
    }
}

/// A compact summary of wallet sync progress, derived from `getwalletstatus`.
pub(crate) struct SyncSummary {
    /// Fraction in `[0.0, 1.0]`, or `None` if indeterminate.
    pub(crate) fraction: Option<f64>,
    /// Whether the wallet is fully synced.
    pub(crate) synced: bool,
    /// The height the wallet is fully synced to, if known.
    pub(crate) synced_height: Option<u32>,
    /// The node's chain tip height.
    pub(crate) node_height: Option<u32>,
    /// Blocks still to scan, if known.
    pub(crate) unscanned_blocks: Option<u32>,
}

impl SyncSummary {
    /// A short one-line label, e.g. `Sync 87%` or `Synced`.
    pub(crate) fn short_label(&self) -> String {
        if self.synced {
            crate::fl!("tui-sync-synced")
        } else if let Some(f) = self.fraction {
            crate::fl!("tui-sync-percent", percent = format!("{:.0}", f * 100.0))
        } else {
            crate::fl!("tui-sync-syncing")
        }
    }
}

/// Where keyboard focus currently sits.
///
/// `Tabs` focus means navigation keys move between views (the header row is highlighted);
/// `View` focus means keys are handled by the active view's body. `Esc` moves focus from
/// the body up to the tabs; selecting/entering a view moves focus back down into it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    Tabs,
    View,
}

/// A transient status message shown in the footer.
#[derive(Clone)]
pub(crate) struct Toast {
    pub(crate) text: String,
    pub(crate) is_error: bool,
}

/// Cached wallet data, refreshed on each tick and after actions.
#[derive(Default)]
pub(crate) struct WalletData {
    pub(crate) status: Option<WalletStatus>,
    pub(crate) total_balance: Option<TotalBalance>,
    pub(crate) balances: Option<Balances>,
    pub(crate) accounts: Vec<Account>,
    pub(crate) transactions: Vec<WalletTx>,
    /// The minimum number of confirmations used when querying balances.
    pub(crate) minconf: u32,
    /// Whether the wallet summary is ready yet (balances unavailable while syncing).
    pub(crate) balances_syncing: bool,
}

/// The interactive send form.
#[derive(Default)]
pub(crate) struct SendForm {
    /// The source account, as an index into [`WalletData::accounts`].
    pub(crate) from_account: usize,
    pub(crate) to: String,
    pub(crate) amount: String,
    pub(crate) memo: String,
    /// Index into [`PRIVACY_POLICIES`].
    pub(crate) privacy_policy: usize,
    /// Which field currently has focus.
    pub(crate) field: SendField,
    /// Whether the focused text field is currently being edited.
    ///
    /// Text fields only capture keystrokes while editing; otherwise navigation keys
    /// (`j`/`k`) move between fields. Editing is entered with `Enter` and left with `Esc`.
    pub(crate) editing: bool,
    /// A submitted operation that is being polled, if any.
    pub(crate) pending_opid: Option<String>,
    /// The latest known status of the pending operation.
    pub(crate) pending_status: Option<OperationStatus>,
    /// Whether the confirmation prompt is showing.
    pub(crate) confirming: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SendField {
    #[default]
    From,
    To,
    Amount,
    Memo,
    PrivacyPolicy,
    /// The "Review & send" action row.
    Submit,
}

impl SendField {
    /// Whether this field accepts free-text input (vs. a selector or action).
    pub(crate) fn is_text(self) -> bool {
        matches!(self, SendField::To | SendField::Amount | SendField::Memo)
    }
}

/// The privacy policies offered in the send form, weakest-last.
pub(crate) const PRIVACY_POLICIES: [&str; 7] = [
    "FullPrivacy",
    "AllowRevealedAmounts",
    "AllowRevealedRecipients",
    "AllowRevealedSenders",
    "AllowLinkingAccountAddresses",
    "AllowFullyTransparent",
    "NoPrivacy",
];

/// State for the Logs view.
#[derive(Default)]
pub(crate) struct LogsState {
    /// The path to the client log file, if known.
    pub(crate) path: Option<std::path::PathBuf>,
    /// The most recently loaded tail of the log file.
    pub(crate) lines: Vec<String>,
    /// Scroll offset from the bottom, in lines (0 = following the tail).
    pub(crate) scroll_from_bottom: usize,
    /// An error encountered while reading the log file, if any.
    pub(crate) read_error: Option<String>,
}

/// A modal prompt for textual input (e.g. unlock passphrase, new account name).
pub(crate) struct Prompt {
    pub(crate) title: String,
    pub(crate) value: String,
    pub(crate) masked: bool,
    pub(crate) kind: PromptKind,
}

#[derive(Clone, Copy)]
pub(crate) enum PromptKind {
    Unlock,
    NewAccount,
}

/// The top-level application state.
pub(crate) struct App {
    client: WalletClient,
    pub(crate) view: View,
    pub(crate) focus: Focus,
    pub(crate) data: WalletData,
    pub(crate) accounts: AccountsState,
    pub(crate) addresses: AddressesState,
    pub(crate) transactions: TransactionsState,
    pub(crate) send: SendForm,
    pub(crate) logs: LogsState,
    pub(crate) toast: Option<Toast>,
    pub(crate) prompt: Option<Prompt>,
    pub(crate) show_help: bool,
    /// The wallet's encryption/lock state, as last observed.
    pub(crate) lock_state: LockState,
    should_quit: bool,
}

impl App {
    pub(crate) fn new(client: WalletClient, log_path: Option<std::path::PathBuf>) -> Self {
        Self {
            client,
            view: View::Dashboard,
            focus: Focus::View,
            data: WalletData {
                minconf: 1,
                ..Default::default()
            },
            accounts: AccountsState::default(),
            addresses: AddressesState::default(),
            transactions: TransactionsState::default(),
            send: SendForm::default(),
            logs: LogsState {
                path: log_path,
                ..Default::default()
            },
            toast: None,
            prompt: None,
            show_help: false,
            // Assume locked until we learn otherwise, so the UI never appears usable
            // before we have confirmed the wallet is accessible.
            lock_state: LockState::Locked,
            should_quit: false,
        }
    }

    /// Whether the wallet is currently inaccessible and must be unlocked before use.
    pub(crate) fn is_gated(&self) -> bool {
        self.lock_state == LockState::Locked
    }

    /// Whether a text field is currently capturing keystrokes, in which case global
    /// keyboard shortcuts must be suppressed so the characters can be typed.
    fn is_text_input_active(&self) -> bool {
        self.view == View::Send && self.send.editing
    }

    /// Computes a summary of wallet sync progress from the latest `getwalletstatus`.
    ///
    /// Progress is wallet-wide (the backend does not expose per-account progress).
    pub(crate) fn sync_summary(&self) -> SyncSummary {
        let Some(status) = &self.data.status else {
            return SyncSummary {
                fraction: None,
                synced: false,
                synced_height: None,
                node_height: None,
                unscanned_blocks: None,
            };
        };

        match &status.sync_work_remaining {
            // No work remaining means fully synced.
            None => SyncSummary {
                fraction: Some(1.0),
                synced: true,
                synced_height: status.fully_synced_height,
                node_height: Some(status.node_tip.height),
                unscanned_blocks: Some(0),
            },
            Some(work) => {
                let p = &work.progress;
                // The denominator can be zero when a range contains no shielded notes.
                let fraction = (p.denominator != 0)
                    .then(|| (p.numerator as f64 / p.denominator as f64).clamp(0.0, 1.0));
                SyncSummary {
                    fraction,
                    synced: false,
                    synced_height: status.fully_synced_height,
                    node_height: Some(status.node_tip.height),
                    unscanned_blocks: Some(work.unscanned_blocks),
                }
            }
        }
    }

    /// Runs the event loop until the user quits.
    pub(crate) async fn run(&mut self, terminal: &mut Tui) -> Result<()> {
        let mut events = EventSource::new(TICK_RATE);

        // Determine the wallet's lock state before doing anything else, so the UI never
        // appears usable when the wallet is locked.
        self.refresh_lock_state().await;
        if !self.is_gated() {
            self.refresh().await;
        }

        loop {
            terminal
                .draw(|frame| ui::render(self, frame))
                .context("failed to draw the terminal frame")?;

            match events.next().await {
                Event::Key(key) => self.on_key(key).await,
                Event::Tick => self.on_tick().await,
                Event::Resize => {}
            }

            if self.should_quit {
                break;
            }
        }

        // Lock the wallet on exit so spend authority does not outlive the session (the
        // unlock we performed has a timeout, but quitting earlier should re-lock promptly).
        self.lock_on_exit().await;

        Ok(())
    }

    /// Best-effort re-lock of the wallet when the session ends.
    ///
    /// Attempts to lock unless the wallet is known to be unencrypted (in which case there
    /// is nothing to lock). Locking an already-locked wallet is harmless, so we do not rely
    /// on `lock_state` being perfectly up to date. Any error is ignored: the process is
    /// exiting, and a stale unlock will still expire on its own timeout.
    async fn lock_on_exit(&mut self) {
        if self.lock_state != LockState::Unencrypted {
            let _ = self.client.lock().await;
        }
    }

    /// Refreshes the wallet's encryption/lock state from `getwalletinfo`.
    pub(crate) async fn refresh_lock_state(&mut self) {
        match self.client.get_wallet_info().await {
            Ok(Ok(info)) => self.lock_state = info.lock_state(),
            // If we can't determine the state, remain conservative: stay locked.
            Ok(Err(e)) => self.error(crate::fl!(
                "tui-err-rpc-call",
                method = "getwalletinfo",
                error = e.to_string()
            )),
            Err(e) => self.error(e.to_string()),
        }
    }

    /// Returns a reference to the wallet client.
    pub(crate) fn client(&self) -> &WalletClient {
        &self.client
    }

    /// Shows a transient informational message (also recorded to the log).
    pub(crate) fn info(&mut self, text: impl Into<String>) {
        let text = text.into();
        tracing::info!("{text}");
        self.toast = Some(Toast {
            text,
            is_error: false,
        });
    }

    /// Shows a transient error message (also recorded to the log).
    ///
    /// The footer can only show one line, so the full message — which may be truncated
    /// on screen — is always written to the log for later inspection.
    pub(crate) fn error(&mut self, text: impl Into<String>) {
        let text = text.into();
        tracing::error!("{text}");
        self.toast = Some(Toast {
            text,
            is_error: true,
        });
    }

    async fn on_tick(&mut self) {
        // The Logs view is available regardless of lock state, and follows the log tail.
        if self.view == View::Logs && self.logs.scroll_from_bottom == 0 {
            self.load_logs();
        }

        // Keep the lock state current (it can change when the unlock timeout elapses).
        self.refresh_lock_state().await;
        if self.is_gated() {
            // While locked, do not fetch or display any wallet data.
            return;
        }
        self.refresh().await;
        // Poll a pending send operation if there is one.
        if self.send.pending_opid.is_some() {
            self.poll_send().await;
        }
    }

    /// Refreshes cached wallet data from the backend.
    pub(crate) async fn refresh(&mut self) {
        let minconf = self.data.minconf;

        match self.client.get_wallet_status().await {
            Ok(Ok(status)) => self.data.status = Some(status),
            Ok(Err(e)) => self.error(crate::fl!(
                "tui-err-rpc-call",
                method = "getwalletstatus",
                error = e.to_string()
            )),
            Err(e) => self.error(e.to_string()),
        }

        match self.client.get_total_balance(minconf).await {
            Ok(Ok(tb)) => self.data.total_balance = Some(tb),
            Ok(Err(_)) => {} // tolerated; total balance has stricter requirements
            Err(e) => self.error(e.to_string()),
        }

        match self.client.get_balances(minconf).await {
            Ok(Ok(b)) => {
                self.data.balances = Some(b);
                self.data.balances_syncing = false;
            }
            // `-28` (InWarmup) means the wallet summary isn't ready yet because the wallet
            // is still syncing/scanning. This is expected, not an error: surface it as a
            // "syncing" state rather than a scary toast.
            Ok(Err(e)) if e.code == -28 => {
                self.data.balances = None;
                self.data.balances_syncing = true;
            }
            Ok(Err(e)) => self.error(crate::fl!(
                "tui-err-rpc-call",
                method = "z_getbalances",
                error = e.to_string()
            )),
            Err(e) => self.error(e.to_string()),
        }

        match self.client.list_accounts().await {
            Ok(Ok(accounts)) => {
                self.data.accounts = accounts;
                self.clamp_selection();
            }
            Ok(Err(e)) => self.error(crate::fl!(
                "tui-err-rpc-call",
                method = "z_listaccounts",
                error = e.to_string()
            )),
            Err(e) => self.error(e.to_string()),
        }

        self.refresh_transactions().await;
    }

    pub(crate) async fn refresh_transactions(&mut self) {
        match self
            .client
            .list_transactions(self.transactions.offset, TX_PAGE_SIZE)
            .await
        {
            Ok(Ok(txs)) => {
                self.data.transactions = txs;
                if self.transactions.selected >= self.data.transactions.len() {
                    self.transactions.selected = self.data.transactions.len().saturating_sub(1);
                }
            }
            // z_listtransactions is experimental; don't spam errors on empty wallets.
            Ok(Err(_)) => self.data.transactions.clear(),
            Err(e) => self.error(e.to_string()),
        }
    }

    /// Keeps every account-indexed selection within the bounds of the current accounts
    /// list, which can shrink between refreshes. Each index is read with `.get()`, so an
    /// out-of-bounds value would not panic, but it would silently select the wrong account.
    fn clamp_selection(&mut self) {
        let last = self.data.accounts.len().saturating_sub(1);
        for index in [
            &mut self.accounts.selected,
            &mut self.addresses.account,
            &mut self.send.from_account,
        ] {
            *index = (*index).min(last);
        }
    }

    async fn on_key(&mut self, key: KeyEvent) {
        // Global: Ctrl-C always quits.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        // A modal prompt takes precedence over everything else.
        if self.prompt.is_some() {
            self.on_key_prompt(key).await;
            return;
        }

        // The help overlay swallows input; any key dismisses it.
        if self.show_help {
            self.show_help = false;
            return;
        }

        // While the wallet is locked, the only permitted actions are unlocking, quitting,
        // viewing help, and viewing the (non-sensitive) logs. The wallet's data must not
        // appear usable.
        if self.is_gated() {
            match key.code {
                KeyCode::Char('q') => self.should_quit = true,
                KeyCode::Char('?') => self.show_help = true,
                KeyCode::Char('u') => self.open_unlock_prompt(),
                // Allow toggling to/from the Logs view while locked.
                KeyCode::Char('7') => self.set_view(View::Logs),
                KeyCode::Char('1') => self.set_view(View::Dashboard),
                // Enter unlocks unless we're on the Logs view (where it's not an action).
                KeyCode::Enter if self.view != View::Logs => self.open_unlock_prompt(),
                // Let the Logs view handle scrolling keys while locked.
                _ if self.view == View::Logs => views::logs::on_key(self, key),
                _ => {}
            }
            return;
        }

        // When a text field is actively being edited, all keystrokes must go to the field
        // (so e.g. '?', 'q', and digits are typed rather than triggering shortcuts).
        if self.is_text_input_active() {
            self.on_key_view(key).await;
            return;
        }

        // Keys handled regardless of focus.
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                return;
            }
            KeyCode::Char('?') => {
                self.show_help = true;
                return;
            }
            KeyCode::Char('r') => {
                self.refresh().await;
                self.info(crate::fl!("tui-toast-refreshed"));
                return;
            }
            // Direct view jumps work from anywhere and focus the view body.
            KeyCode::Char(c @ '1'..='7') => {
                let idx = (c as u8 - b'1') as usize;
                if let Some(&v) = View::ALL.get(idx) {
                    self.set_view(v);
                    self.focus = Focus::View;
                }
                return;
            }
            // Lock/unlock shortcuts (only meaningful for encrypted wallets).
            KeyCode::Char('L') => {
                self.lock_wallet().await;
                return;
            }
            KeyCode::Char('U') => {
                self.open_unlock_prompt();
                return;
            }
            _ => {}
        }

        match self.focus {
            Focus::Tabs => self.on_key_tabs(key),
            Focus::View => self.on_key_view(key).await,
        }
    }

    /// Key handling when focus is on the header/tab row.
    fn on_key_tabs(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left | KeyCode::Char('h') | KeyCode::BackTab => self.prev_view(),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => self.next_view(),
            // Descend into the focused view's body.
            KeyCode::Enter | KeyCode::Down | KeyCode::Char('j') => self.focus = Focus::View,
            _ => {}
        }
    }

    /// Switches to the given view, performing any per-view entry work.
    fn set_view(&mut self, view: View) {
        self.view = view;
        // Load the log tail immediately on entering the Logs view.
        if view == View::Logs {
            self.load_logs();
        }
    }

    fn next_view(&mut self) {
        let idx = (self.view.index() + 1) % View::ALL.len();
        self.set_view(View::ALL[idx]);
    }

    fn prev_view(&mut self) {
        let idx = (self.view.index() + View::ALL.len() - 1) % View::ALL.len();
        self.set_view(View::ALL[idx]);
    }

    /// Dispatches keys to the active view when focus is on the body.
    ///
    /// `Esc` normally moves focus back up to the tab row so the user can switch views;
    /// `Tab`/`BackTab` move between views directly. The exception is when the Send view is
    /// actively editing a text field: in that case all keys (including `Esc`/`Tab`) are
    /// routed to the Send handler so they don't navigate away mid-edit.
    async fn on_key_view(&mut self, key: KeyEvent) {
        let send_editing = self.is_text_input_active();

        if !send_editing {
            match key.code {
                KeyCode::Esc => {
                    self.focus = Focus::Tabs;
                    return;
                }
                KeyCode::Tab => {
                    self.next_view();
                    return;
                }
                KeyCode::BackTab => {
                    self.prev_view();
                    return;
                }
                _ => {}
            }
        }

        match self.view {
            View::Accounts => views::accounts::on_key(self, key).await,
            View::Addresses => views::addresses::on_key(self, key).await,
            View::Transactions => views::transactions::on_key(self, key).await,
            View::Balances => views::balances::on_key(self, key),
            View::Send => views::send::on_key(self, key).await,
            View::Logs => views::logs::on_key(self, key),
            View::Dashboard => {}
        }
    }

    /// Reads the tail of the log file into [`LogsState`].
    ///
    /// At most the last `MAX_LOG_LINES` lines are kept, to bound memory and rendering cost
    /// for a large log file. See [`tail_lines`] for why only the file's tail is read.
    pub(crate) fn load_logs(&mut self) {
        const MAX_LOG_LINES: usize = 2000;

        let Some(path) = self.logs.path.clone() else {
            self.logs.read_error = Some(crate::fl!("tui-logs-remote-toast"));
            return;
        };

        match tail_lines(&path, MAX_LOG_LINES) {
            Ok(lines) => {
                self.logs.lines = lines;
                self.logs.read_error = None;
            }
            Err(e) => {
                self.logs.lines.clear();
                self.logs.read_error = Some(crate::fl!("tui-err-read-log", error = e.to_string()));
            }
        }
    }

    // --- Prompts ----------------------------------------------------------------------

    fn open_unlock_prompt(&mut self) {
        if self.lock_state == LockState::Unencrypted {
            self.info(crate::fl!("tui-unlock-not-encrypted-prompt"));
            return;
        }
        self.prompt = Some(Prompt {
            title: crate::fl!("tui-prompt-unlock-title"),
            value: String::new(),
            masked: true,
            kind: PromptKind::Unlock,
        });
    }

    pub(crate) fn open_new_account_prompt(&mut self) {
        self.prompt = Some(Prompt {
            title: crate::fl!("tui-prompt-new-account-title"),
            value: String::new(),
            masked: false,
            kind: PromptKind::NewAccount,
        });
    }

    async fn on_key_prompt(&mut self, key: KeyEvent) {
        let Some(prompt) = self.prompt.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.prompt = None,
            KeyCode::Enter => {
                let prompt = self.prompt.take().expect("prompt is present");
                self.submit_prompt(prompt).await;
            }
            KeyCode::Backspace => {
                prompt.value.pop();
            }
            KeyCode::Char(c) => prompt.value.push(c),
            _ => {}
        }
    }

    async fn submit_prompt(&mut self, prompt: Prompt) {
        match prompt.kind {
            PromptKind::Unlock => {
                // Unlock for 5 minutes.
                match self.client.unlock(&prompt.value, 300).await {
                    Ok(Ok(_)) => {
                        self.info(crate::fl!("tui-toast-unlocked"));
                        // Re-check state and load data now that we have access.
                        self.refresh_lock_state().await;
                        if !self.is_gated() {
                            self.focus = Focus::View;
                            self.refresh().await;
                        }
                    }
                    // Wrong passphrase.
                    Ok(Err(e)) if e.code == -14 => {
                        self.error(crate::fl!("tui-err-incorrect-passphrase"));
                    }
                    // Wallet is not encrypted (should not happen: we guard the prompt).
                    Ok(Err(e)) if e.code == -15 => {
                        self.error(crate::fl!("tui-unlock-not-encrypted"));
                    }
                    Ok(Err(e)) => {
                        self.error(crate::fl!("tui-err-unlock-failed", error = e.to_string()))
                    }
                    Err(e) => self.error(e.to_string()),
                }
            }
            PromptKind::NewAccount => {
                if prompt.value.trim().is_empty() {
                    self.error(crate::fl!("tui-err-account-name-empty"));
                    return;
                }
                match self.client.new_account(prompt.value.trim()).await {
                    Ok(Ok(_)) => {
                        self.info(crate::fl!(
                            "tui-toast-account-created",
                            name = prompt.value.trim()
                        ));
                        self.refresh().await;
                    }
                    Ok(Err(e)) if e.is_unlock_needed() => {
                        self.error(crate::fl!("tui-err-locked-press-u-lower"));
                    }
                    Ok(Err(e)) => self.error(crate::fl!(
                        "tui-err-rpc-call",
                        method = "z_getnewaccount",
                        error = e.to_string()
                    )),
                    Err(e) => self.error(e.to_string()),
                }
            }
        }
    }

    // --- Lock/unlock ------------------------------------------------------------------

    async fn lock_wallet(&mut self) {
        if self.lock_state == LockState::Unencrypted {
            self.info(crate::fl!("tui-lock-not-encrypted"));
            return;
        }
        match self.client.lock().await {
            Ok(Ok(_)) => {
                self.info(crate::fl!("tui-toast-locked"));
                self.refresh_lock_state().await;
            }
            Ok(Err(e)) => self.error(crate::fl!(
                "tui-err-rpc-call",
                method = "walletlock",
                error = e.to_string()
            )),
            Err(e) => self.error(e.to_string()),
        }
    }

    // --- Send polling -----------------------------------------------------------------

    /// Polls the pending send operation and updates its status.
    pub(crate) async fn poll_send(&mut self) {
        let Some(opid) = self.send.pending_opid.clone() else {
            return;
        };
        match self.client.operation_status(&opid).await {
            Ok(Ok(mut statuses)) => {
                if let Some(status) = statuses.pop() {
                    let finished =
                        matches!(status.status.as_str(), "success" | "failed" | "cancelled");
                    if finished {
                        self.send.pending_opid = None;
                        match status.status.as_str() {
                            "success" => self.info(crate::fl!("tui-toast-send-completed")),
                            "failed" => {
                                let msg = status
                                    .error
                                    .as_ref()
                                    .and_then(|e| e.message.clone())
                                    .unwrap_or_else(|| crate::fl!("tui-err-unknown"));
                                self.error(crate::fl!("tui-err-send-failed", error = msg));
                            }
                            _ => self.info(crate::fl!("tui-toast-send-cancelled")),
                        }
                    }
                    self.send.pending_status = Some(status);
                }
            }
            Ok(Err(e)) => self.error(crate::fl!(
                "tui-err-rpc-call",
                method = "z_getoperationstatus",
                error = e.to_string()
            )),
            Err(e) => self.error(e.to_string()),
        }
    }
}

/// Reads up to the last `max_lines` lines of the file at `path`.
///
/// Only the final `MAX_TAIL_BYTES` of the file are read, so the work is bounded no matter
/// how large the log grows. The Logs view re-reads on every tick while following the tail;
/// slurping a multi-megabyte file each time would be wasteful. The read is synchronous but
/// small and fixed, so it does not meaningfully stall the event loop.
fn tail_lines(path: &std::path::Path, max_lines: usize) -> std::io::Result<Vec<String>> {
    use std::io::{Read, Seek, SeekFrom};

    /// The maximum number of bytes read from the end of the log file; comfortably holds
    /// `MAX_LOG_LINES` lines of typical log output.
    const MAX_TAIL_BYTES: u64 = 512 * 1024;

    let mut file = std::fs::File::open(path)?;
    let len = file.seek(SeekFrom::End(0))?;
    let read_len = len.min(MAX_TAIL_BYTES);
    file.seek(SeekFrom::Start(len - read_len))?;

    let mut buf = Vec::with_capacity(read_len as usize);
    file.take(read_len).read_to_end(&mut buf)?;

    // If we began partway through the file, the first line is probably truncated mid-line.
    let dropped_partial_head = read_len < len;
    Ok(lines_from_tail(&buf, dropped_partial_head, max_lines))
}

/// Splits the tail bytes of a log file into at most `max_lines` trailing lines.
///
/// Invalid UTF-8 (such as a multi-byte character split by the read boundary) is replaced
/// lossily. When `drop_partial_head` is set, the first line is dropped as likely-truncated.
fn lines_from_tail(buf: &[u8], drop_partial_head: bool, max_lines: usize) -> Vec<String> {
    let text = String::from_utf8_lossy(buf);
    let skip = usize::from(drop_partial_head);
    let mut lines: Vec<String> = text.lines().skip(skip).map(str::to_owned).collect();
    if lines.len() > max_lines {
        lines.drain(0..lines.len() - max_lines);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::lines_from_tail;

    #[test]
    fn drops_truncated_head_line_when_reading_from_the_middle() {
        let buf = b"rtial line\nbeta\ngamma\ndelta\n";
        assert_eq!(lines_from_tail(buf, true, 10), ["beta", "gamma", "delta"]);
    }

    #[test]
    fn keeps_only_the_last_max_lines() {
        let buf = b"one\ntwo\nthree\nfour\n";
        assert_eq!(lines_from_tail(buf, false, 2), ["three", "four"]);
    }

    #[test]
    fn keeps_head_when_the_whole_file_was_read() {
        let buf = b"alpha\nbeta\n";
        assert_eq!(lines_from_tail(buf, false, 10), ["alpha", "beta"]);
    }

    #[test]
    fn handles_empty_input() {
        assert!(lines_from_tail(b"", false, 10).is_empty());
        assert!(lines_from_tail(b"", true, 10).is_empty());
    }

    use proptest::prelude::*;

    proptest! {
        /// The tail is bounded by `max_lines` and never panics, even on arbitrary bytes
        /// (which may be invalid UTF-8 or split a multi-byte character at the boundary).
        #[test]
        fn tail_is_bounded_and_panic_free(
            bytes in proptest::collection::vec(any::<u8>(), 0..1024),
            drop_head in any::<bool>(),
            max in 0usize..32,
        ) {
            prop_assert!(lines_from_tail(&bytes, drop_head, max).len() <= max);
        }

        /// Without dropping a partial head line, the result is exactly the last `max` lines.
        #[test]
        fn tail_returns_the_last_max_lines(
            parts in proptest::collection::vec("[a-z]{0,8}", 0..32),
            max in 0usize..40,
        ) {
            let text = parts.join("\n");
            let all: Vec<String> = text.lines().map(str::to_owned).collect();
            let start = all.len().saturating_sub(max);
            prop_assert_eq!(
                lines_from_tail(text.as_bytes(), false, max),
                all[start..].to_vec()
            );
        }
    }
}
