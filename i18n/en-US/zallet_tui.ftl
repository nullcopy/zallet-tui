### Localization strings for zallet-tui, an interactive terminal UI for the
### Zallet Zcash wallet.

# Terms (not to be localized)
-zcash = Zcash
-zallet = zallet
-tui-rpc-url = --rpc-url

## Interactive terminal UI

# View / tab titles
tui-view-dashboard = Dashboard
tui-view-accounts = Accounts
tui-view-balances = Balances
tui-view-receive = Receive
tui-view-transactions = Transactions
tui-view-send = Send
tui-view-logs = Logs

# Sync status labels
tui-sync-synced = Synced
tui-sync-syncing = Syncing…
tui-sync-percent = Sync {$percent}%
tui-sync-heights = ({$synced} / {$node})
tui-sync-tip = (tip {$node})
tui-sync-blocks-left = · {$remaining} blocks left

# Generic units / values
tui-amount-zec = {$amount} ZEC
tui-value-unknown = (unknown)
tui-value-unnamed = (unnamed)

# Lock state (title bar)
tui-lock-unencrypted = unencrypted
tui-lock-locked = LOCKED
tui-lock-unlocked = unlocked

# Toasts: refresh and generic
tui-toast-refreshed = Refreshed

# Toasts: RPC call failures (method is a protocol identifier)
tui-err-rpc-call = {$method}: {$error}

# Toasts and prompts: lock / unlock
tui-unlock-not-encrypted-prompt = This wallet is not encrypted; there is no passphrase to enter.
tui-unlock-not-encrypted = This wallet is not encrypted; nothing to unlock.
tui-prompt-unlock-title = Unlock wallet (passphrase)
tui-prompt-new-account-title = New account name
tui-toast-unlocked = Wallet unlocked for 5 minutes
tui-err-incorrect-passphrase = Incorrect passphrase.
tui-err-unlock-failed = Unlock failed: {$error}
tui-toast-locked = Wallet locked
tui-lock-not-encrypted = This wallet is not encrypted; there is nothing to lock.

# Toasts: wallet locked, action needs unlock
tui-err-locked-press-u-lower = Wallet is locked. Press 'u' to unlock first.
tui-err-locked-press-u-upper = Wallet is locked. Press 'U' to unlock first.

# Toasts: accounts
tui-err-account-name-empty = Account name cannot be empty
tui-toast-account-created = Created account '{$name}'

# Toasts: send operations
tui-toast-send-completed = Send completed
tui-toast-send-cancelled = Send cancelled
tui-err-send-failed = Send failed: {$error}
tui-err-unknown = unknown error

# Logs view (toasts / placeholders)
tui-logs-remote-toast = Logs are written by the remote node in {-tui-rpc-url} mode.
tui-err-read-log = Could not read log file: {$error}

# Client errors (Display impls)
tui-err-build-client = failed to build RPC client: {$error}
tui-err-request = RPC request failed: {$error}
tui-err-rpc-with-code = {$message} (code {$code})

# Prompt modal
tui-prompt-hint = Enter to confirm · Esc to cancel

# Locked screen
tui-locked-title = Wallet locked
tui-locked-line1 = This wallet is encrypted and locked.
tui-locked-line2 = You must unlock it before you can view balances, addresses,
tui-locked-line3 = transactions, or send funds.
tui-locked-hint = Press 'u' or Enter to unlock · 'q' to quit

# Header / footer
tui-header-title = {-zallet} · {$lock}
tui-footer-gated = [u]nlock  [q]uit
tui-footer-unlock = [U]nlock{" "}
tui-footer-lock = [L]ock{" "}
tui-footer-nav-tabs = [h/l]tab [Enter]open
tui-footer-nav-view = [Esc]tabs [Tab]switch
tui-footer-hint = [?]help [q]uit [r]efresh {$nav} {$lock}

# Help overlay
tui-help-nav-header = Navigation
tui-help-nav-esc = Esc               Move focus to the tab row
tui-help-nav-switch-tabs = h/l or ←/→        Switch tabs (when on tab row)
tui-help-nav-enter = Enter or j        Enter the focused view
tui-help-nav-switch-view = Tab / Shift-Tab   Switch view directly
tui-help-nav-jump = 1..7              Jump to a view
tui-help-nav-select = j/k or ↑/↓        Move selection within a view
tui-help-global-header = Global
tui-help-global-refresh = r                 Refresh data
tui-help-global-unlock = U                 Unlock wallet (encrypted)
tui-help-global-lock = L                 Lock wallet (encrypted)
tui-help-global-help = ?                 Toggle this help
tui-help-global-quit = q / Ctrl-C        Quit
tui-help-accounts = Accounts: n = new   ·   Receive: ←/→ account, a = derive address, c = copy
tui-help-transactions = Transactions: [ / ] = page   ·   Send: Enter or i edits a field
tui-help-logs = Logs: j/k scroll, g/G top/bottom, R reload
tui-help-close = Press any key to close
tui-help-title = Help

# Dashboard view
tui-dash-status-title = Status
tui-dash-node-tip = Node tip
tui-dash-node-hash = Node hash
tui-dash-wallet-tip = Wallet tip
tui-dash-not-syncing = (not yet syncing)
tui-dash-fully-synced-to = Fully synced to
tui-dash-loading-status = Loading wallet status…
tui-dash-accounts = Accounts
tui-dash-sync-title = Sync
tui-dash-sync-progress = {$percent}%  ({$blocks} blocks remaining)
tui-dash-fully-synced = Fully synced
tui-dash-balance-title = Balance
tui-dash-balances-syncing = Balances are not available yet — the wallet is still syncing.
tui-dash-total = Total
tui-dash-shielded = Shielded (private)
tui-dash-transparent = Transparent
tui-dash-total-unavailable = Total balance unavailable (watch-only, or not yet synced).
tui-dash-minconf = minconf = {$minconf}

# Accounts view
tui-accounts-title = Accounts
tui-accounts-empty = No accounts yet. Press 'n' to create one (wallet must be unlocked).
tui-accounts-title-list = Accounts  ([n]ew)
tui-accounts-balance-pending = (+{$amount} pending)

# Addresses (Receive) view
tui-addr-no-account-selected = No account selected.
tui-addr-derived = Derived a new address
tui-addr-copied = Address copied to clipboard
tui-addr-copy-failed = Could not copy to clipboard: {$error}
tui-addr-kind-unified = unified
tui-addr-kind-sapling = sapling
tui-addr-kind-transparent = transparent
tui-addr-no-accounts = (no accounts)
tui-addr-account-label = {" "}Account:{" "}
tui-addr-account-hint = (←/→ account · j/k address · c copy · a new)
tui-addr-empty = No addresses for this account yet. Press 'a' to derive one.
tui-addr-title = Addresses
tui-addr-receive-title = Receive
tui-addr-select = Select an address to view it.
tui-addr-qr-enlarge = (enlarge the window to show the QR code)
tui-addr-qr-too-long = (address too long to render as QR)

# Balances view
tui-bal-header-account = Account
tui-bal-header-transparent = Transparent
tui-bal-header-sapling = Sapling
tui-bal-header-orchard = Orchard
tui-bal-header-pending = Pending
tui-bal-header-total = Total
tui-bal-title = Balances  (minconf = {$minconf}  [+/-] to change)
tui-bal-syncing = Balances are not available yet — the wallet is still syncing.
tui-bal-empty = No balances to display.

# Transactions view
tui-tx-title = Transactions  (page {$page}, [ / ] to page · experimental)
tui-tx-empty = No transactions to display.
tui-tx-unmined = unmined
tui-tx-detail-title = Detail
tui-tx-field-txid = txid
tui-tx-field-height = height
tui-tx-field-delta = delta
tui-tx-field-fee = fee
tui-tx-field-block-time = block time
tui-tx-field-account = account
tui-tx-expired = expired (unmined)
tui-tx-select = Select a transaction.

# Send view
tui-send-cancelled = Send cancelled
tui-send-err-no-accounts = No accounts available to send from
tui-send-err-no-source = Select a source account
tui-send-err-no-spendable = Selected account has no spendable address
tui-send-err-recipient-required = Recipient address is required
tui-send-err-amount-required = Amount is required
tui-send-err-amount-nan = Amount must be a number
tui-send-err-no-source-selected = No source account selected
tui-send-submitted = Submitted (op {$opid})
tui-send-from = From
tui-send-to = To
tui-send-amount = Amount (ZEC)
tui-send-memo = Memo
tui-send-privacy-policy = Privacy policy
tui-send-review = [ Review & send ]
tui-send-no-spendable-suffix = (no spendable address)
tui-send-fees-note = Fees are computed automatically (ZIP-317).
tui-send-privacy-warning = ⚠ This policy reduces privacy. Only proceed if you understand the implications.
tui-send-hint-editing = EDITING — type to enter text · Enter/Esc to finish
tui-send-hint-text = ↑↓ move · Enter to edit this field · Esc to tabs
tui-send-hint-submit = ↑↓ move · Enter to review & send · Esc to tabs
tui-send-hint-select = ↑↓ move · ←/→ change selection · Esc to tabs
tui-send-title = Send
tui-send-operation-title = Operation
tui-send-confirm = Confirm send?
tui-send-confirm-summary = {$amount} ZEC from {$from} → {$to}
tui-send-confirm-hint = [y] yes   [n] no
tui-send-queued = queued
tui-send-operation = Operation {$opid}
tui-send-status = Status: {$status}…
tui-send-succeeded = Send succeeded
tui-send-txid = txid: {$txid}
tui-send-failed = Last send did not succeed (see footer).
tui-send-placeholder = Fill in the form and press Enter to review.

# Logs view
tui-logs-file-label = {" "}Log file:{" "}
tui-logs-remote = Logs are written by the remote node when using {-tui-rpc-url}.
tui-logs-title = Logs  ({$state}  ·  j/k scroll · g/G top/bottom · R reload)
tui-logs-following = following
tui-logs-scrolled = scrolled
