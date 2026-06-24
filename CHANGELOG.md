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
