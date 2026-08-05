# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Initial release: an interactive terminal UI for the Zallet Zcash wallet, implemented as a
standalone JSON-RPC client.

### Added

- Connection to a running `zallet start` instance over its JSON-RPC interface, with
  `[[rpc.auth]]` user/password authentication via `--rpc-user`/`--rpc-password` or a
  `zallet-tui.toml` config file.
- Dashboard, accounts, balances, receive (with QR codes), transactions, send, and logs
  views, with a persistent wallet-wide sync indicator.
- Automatic re-locking of an encrypted wallet when the TUI exits.
- Copying the selected receive address to the clipboard with `c` (via OSC 52).
- `generate-config` subcommand that writes a default, fully-commented `zallet-tui.toml`.
- File-based logging so output does not corrupt the terminal display, written to the
  platform log directory by default (overridable with `--log-file`).
- Fluent-based localization.
- Ironwood pool support (Zallet v0.1.0-beta.2 / NU6.3): an Ironwood column in the
  Balances view, and a notice flagging at-risk funds remaining in the sealed Orchard
  pool.
- Sending via `z_sendfromaccount`: spends from the account UUID with an explicit fund
  source selector (Orchard + Ironwood / Sapling / Transparent) and reports the broadcast
  txids directly. `z_sendmany` remains only for ZIP 320 (TEX) recipients.
- A hint appended to privacy-policy send rejections, explaining that spending sealed
  Orchard funds crosses into Ironwood and needs `AllowRevealedAmounts` or weaker.
- Cookie-file authentication (`--cookie-file` / `cookie_file`), using the `.cookie`
  credential Zallet writes into its data directory on startup.
- `check` subcommand that queries the server's `rpc.discover` schema and reports any RPC
  method the TUI needs that the server does not expose.
- Block datetimes (RFC 3339) in the transaction detail pane, falling back to the raw
  Unix timestamp for older servers.

### Changed

- The TUI now targets Zallet v0.1.0-beta.2: the `z_getbalances` response models the
  regular/coinbase transparent split (previously any account with transparent funds
  failed to deserialize and broke the Balances view) and the `ironwood` pool.
- Zallet's global sync lock errors (`-10` catching up, `-2` reorg recovery) are treated
  as the existing "still syncing" state instead of surfacing as error toasts, and the
  new `getwalletstatus.locked` field is checked before submitting a send.
- `z_listtransactions` failures are shown in the Transactions view instead of being
  silently swallowed as an empty list.
- Connection flags are global, so they may be given after a subcommand.

### Fixed

- Send operations are collected with `z_getoperationresult` once finished, so they no
  longer accumulate in (and eventually overflow) Zallet's bounded operation queue; an
  opid the wallet no longer tracks stops the polling loop instead of spinning forever.
- The README's example RPC port and username now match the built-in defaults.
