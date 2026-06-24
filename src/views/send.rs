//! Send view: a form for `z_sendmany` with confirmation and inline operation polling.
//!
//! The "from" field is an account selector rather than free text: `z_sendmany` requires an
//! address as its source, so the selected account is resolved to one of its addresses (a
//! unified address where possible) at submit time.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::{App, PRIVACY_POLICIES, SendField};

/// Handles a key event for the send view.
///
/// View navigation (`Esc`, `Tab`, `BackTab`) is handled by the caller unless a text field
/// is being edited, in which case all keys are routed here.
pub(crate) async fn on_key(app: &mut App, key: KeyEvent) {
    // While confirming, only y/n are meaningful.
    if app.send.confirming {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                app.send.confirming = false;
                submit(app).await;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.send.confirming = false;
                app.info(crate::fl!("tui-send-cancelled"));
            }
            _ => {}
        }
        return;
    }

    // Editing mode: keystrokes go into the focused text field until Esc/Enter leaves it.
    if app.send.editing {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => app.send.editing = false,
            KeyCode::Backspace => {
                if let Some(buf) = text_field_mut(app) {
                    buf.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(buf) = text_field_mut(app) {
                    buf.push(c);
                }
            }
            _ => {}
        }
        return;
    }

    // Navigation mode: keys move between fields and operate selectors.
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => app.send.field = next_field(app.send.field),
        KeyCode::Up | KeyCode::Char('k') => app.send.field = prev_field(app.send.field),

        // Selectors respond to left/right (and h/l).
        KeyCode::Left | KeyCode::Char('h') if app.send.field == SendField::From => {
            select_prev_account(app)
        }
        KeyCode::Right | KeyCode::Char('l') if app.send.field == SendField::From => {
            select_next_account(app)
        }
        KeyCode::Left | KeyCode::Char('h') if app.send.field == SendField::PrivacyPolicy => {
            app.send.privacy_policy = app.send.privacy_policy.saturating_sub(1);
        }
        KeyCode::Right | KeyCode::Char('l') if app.send.field == SendField::PrivacyPolicy => {
            app.send.privacy_policy = (app.send.privacy_policy + 1).min(PRIVACY_POLICIES.len() - 1);
        }

        // `Enter` and `i` (vim-style insert) both begin editing a text field. On the
        // Submit row, `Enter` reviews & sends.
        KeyCode::Char('i') if app.send.field.is_text() => {
            app.send.editing = true;
        }
        KeyCode::Enter => {
            if app.send.field.is_text() {
                // Begin editing this text field.
                app.send.editing = true;
            } else if app.send.field == SendField::Submit {
                // Review & send.
                if let Err(msg) = validate(app) {
                    app.error(msg);
                } else {
                    app.send.confirming = true;
                }
            }
        }
        _ => {}
    }
}

fn select_next_account(app: &mut App) {
    if !app.data.accounts.is_empty() {
        app.send.from_account = (app.send.from_account + 1) % app.data.accounts.len();
    }
}

fn select_prev_account(app: &mut App) {
    if !app.data.accounts.is_empty() {
        let n = app.data.accounts.len();
        app.send.from_account = (app.send.from_account + n - 1) % n;
    }
}

fn validate(app: &App) -> Result<(), String> {
    if app.data.accounts.is_empty() {
        return Err(crate::fl!("tui-send-err-no-accounts"));
    }
    let account = app
        .data
        .accounts
        .get(app.send.from_account)
        .ok_or_else(|| crate::fl!("tui-send-err-no-source"))?;
    if account.spend_source_address().is_none() {
        return Err(crate::fl!("tui-send-err-no-spendable"));
    }
    if app.send.to.trim().is_empty() {
        return Err(crate::fl!("tui-send-err-recipient-required"));
    }
    if app.send.amount.trim().is_empty() {
        return Err(crate::fl!("tui-send-err-amount-required"));
    }
    if !is_valid_amount(app.send.amount.trim()) {
        return Err(crate::fl!("tui-send-err-amount-nan"));
    }
    Ok(())
}

/// Whether `s` is a well-formed, strictly-positive ZEC amount: a non-negative decimal with
/// at most 8 fractional digits (the zatoshi precision), and not zero.
///
/// Parsing as `f64` was deliberately avoided: it would accept `inf`, `NaN`, and scientific
/// notation that the wallet then rejects, and it cannot represent zatoshi exactly.
fn is_valid_amount(s: &str) -> bool {
    let (whole, frac) = match s.split_once('.') {
        Some((whole, frac)) => (whole, Some(frac)),
        None => (s, None),
    };

    let is_digits = |d: &str| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit());
    if !is_digits(whole) {
        return false;
    }
    if let Some(frac) = frac {
        if frac.len() > 8 || !is_digits(frac) {
            return false;
        }
    }

    // Reject zero (e.g. "0", "0.0"): the wallet will not send it.
    whole
        .bytes()
        .chain(frac.unwrap_or("").bytes())
        .any(|b| b != b'0')
}

async fn submit(app: &mut App) {
    let Some(account) = app.data.accounts.get(app.send.from_account) else {
        app.error(crate::fl!("tui-send-err-no-source-selected"));
        return;
    };
    let Some(from) = account.spend_source_address().map(|s| s.to_string()) else {
        app.error(crate::fl!("tui-send-err-no-spendable"));
        return;
    };

    let to = app.send.to.trim().to_string();
    let amount = app.send.amount.trim().to_string();
    let memo = {
        let m = app.send.memo.trim();
        if m.is_empty() {
            None
        } else {
            Some(m.to_string())
        }
    };
    let policy = PRIVACY_POLICIES[app.send.privacy_policy];

    match app
        .client()
        .send_many(&from, &to, &amount, memo.as_deref(), policy)
        .await
    {
        Ok(Ok(opid)) => {
            app.info(crate::fl!("tui-send-submitted", opid = opid.clone()));
            app.send.pending_opid = Some(opid);
            app.send.pending_status = None;
            app.poll_send().await;
        }
        Ok(Err(e)) if e.is_unlock_needed() => {
            app.error(crate::fl!("tui-err-locked-press-u-upper"));
        }
        Ok(Err(e)) => app.error(crate::fl!(
            "tui-err-rpc-call",
            method = "z_sendmany",
            error = e.to_string()
        )),
        Err(e) => app.error(e.to_string()),
    }
}

pub(crate) fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(6)])
        .split(area);

    render_form(app, frame, chunks[0]);
    render_status(app, frame, chunks[1]);
}

fn render_form(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let f = &app.send;
    let policy = PRIVACY_POLICIES[f.privacy_policy];

    // The "from" account selector.
    let from_label = match app.data.accounts.get(f.from_account) {
        Some(account) => {
            let mut s = format!("◀ {} ▶", account.label());
            if account.spend_source_address().is_none() {
                s.push_str("  ");
                s.push_str(&crate::fl!("tui-send-no-spendable-suffix"));
            }
            s
        }
        None => crate::fl!("tui-addr-no-accounts"),
    };

    let mut lines = vec![
        Line::from(vec![
            label_span(&crate::fl!("tui-send-from"), f.field == SendField::From),
            Span::styled(from_label, Style::default().fg(Color::Cyan)),
        ]),
        input_line(
            &crate::fl!("tui-send-to"),
            &f.to,
            f.field == SendField::To,
            f.editing,
        ),
        input_line(
            &crate::fl!("tui-send-amount"),
            &f.amount,
            f.field == SendField::Amount,
            f.editing,
        ),
        input_line(
            &crate::fl!("tui-send-memo"),
            &f.memo,
            f.field == SendField::Memo,
            f.editing,
        ),
        Line::from(vec![
            label_span(
                &crate::fl!("tui-send-privacy-policy"),
                f.field == SendField::PrivacyPolicy,
            ),
            Span::styled(format!("◀ {policy} ▶"), policy_style(f.privacy_policy)),
        ]),
        Line::from(""),
        // The "Review & send" action row.
        Line::from(Span::styled(
            format!("  {}", crate::fl!("tui-send-review")),
            if f.field == SendField::Submit {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Green)
            },
        )),
        Line::from(""),
        Line::from(Span::styled(
            crate::fl!("tui-send-fees-note"),
            Style::default().fg(Color::DarkGray),
        )),
    ];

    if f.privacy_policy > 0 {
        lines.push(Line::from(Span::styled(
            crate::fl!("tui-send-privacy-warning"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
    }

    lines.push(Line::from(""));
    let hint = if f.editing {
        crate::fl!("tui-send-hint-editing")
    } else if f.field.is_text() {
        crate::fl!("tui-send-hint-text")
    } else if f.field == SendField::Submit {
        crate::fl!("tui-send-hint-submit")
    } else {
        crate::fl!("tui-send-hint-select")
    };
    lines.push(Line::from(Span::styled(
        hint,
        Style::default().fg(Color::DarkGray),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", crate::fl!("tui-send-title")));
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_status(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", crate::fl!("tui-send-operation-title")));

    let lines: Vec<Line<'_>> = if app.send.confirming {
        let f = &app.send;
        let from = app
            .data
            .accounts
            .get(f.from_account)
            .map(|a| a.label())
            .unwrap_or_default();
        vec![
            Line::from(Span::styled(
                crate::fl!("tui-send-confirm"),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(format!(
                "  {}",
                crate::fl!(
                    "tui-send-confirm-summary",
                    amount = f.amount.trim(),
                    from = from,
                    to = f.to.trim()
                )
            )),
            Line::from(Span::styled(
                format!("  {}", crate::fl!("tui-send-confirm-hint")),
                Style::default().fg(Color::DarkGray),
            )),
        ]
    } else if let Some(opid) = &app.send.pending_opid {
        let status = app
            .send
            .pending_status
            .as_ref()
            .map(|s| s.status.clone())
            .unwrap_or_else(|| crate::fl!("tui-send-queued"));
        vec![
            Line::from(crate::fl!("tui-send-operation", opid = opid.clone())),
            Line::from(Span::styled(
                crate::fl!("tui-send-status", status = status),
                Style::default().fg(Color::Cyan),
            )),
        ]
    } else if let Some(status) = &app.send.pending_status {
        // Finished op; show the result/error.
        match status.status.as_str() {
            "success" => {
                let txid = status
                    .result
                    .as_ref()
                    .and_then(|r| r.get("txid"))
                    .and_then(|t| t.as_str())
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| crate::fl!("tui-value-unknown"));
                vec![
                    Line::from(Span::styled(
                        crate::fl!("tui-send-succeeded"),
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    )),
                    Line::from(crate::fl!("tui-send-txid", txid = txid)),
                ]
            }
            _ => vec![Line::from(Span::styled(
                crate::fl!("tui-send-failed"),
                Style::default().fg(Color::Red),
            ))],
        }
    } else {
        vec![Line::from(Span::styled(
            crate::fl!("tui-send-placeholder"),
            Style::default().fg(Color::DarkGray),
        ))]
    };

    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

fn input_line(label: &str, value: &str, focused: bool, editing: bool) -> Line<'static> {
    // Only show the text cursor when actively editing this (focused) field.
    let cursor = if focused && editing { "_" } else { "" };
    let value_style = if focused && editing {
        Style::default().fg(Color::White)
    } else {
        Style::default()
    };
    Line::from(vec![
        label_span(label, focused),
        Span::styled(format!("{value}{cursor}"), value_style),
    ])
}

fn label_span(label: &str, focused: bool) -> Span<'static> {
    let style = if focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Span::styled(format!(" {label:>14}: "), style)
}

fn policy_style(index: usize) -> Style {
    if index == 0 {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Yellow)
    }
}

fn next_field(field: SendField) -> SendField {
    match field {
        SendField::From => SendField::To,
        SendField::To => SendField::Amount,
        SendField::Amount => SendField::Memo,
        SendField::Memo => SendField::PrivacyPolicy,
        SendField::PrivacyPolicy => SendField::Submit,
        SendField::Submit => SendField::From,
    }
}

fn prev_field(field: SendField) -> SendField {
    match field {
        SendField::From => SendField::Submit,
        SendField::To => SendField::From,
        SendField::Amount => SendField::To,
        SendField::Memo => SendField::Amount,
        SendField::PrivacyPolicy => SendField::Memo,
        SendField::Submit => SendField::PrivacyPolicy,
    }
}

/// Returns the editable text buffer for the currently-focused field, or `None` if the
/// focused field is a selector rather than a text field.
fn text_field_mut(app: &mut App) -> Option<&mut String> {
    match app.send.field {
        SendField::To => Some(&mut app.send.to),
        SendField::Amount => Some(&mut app.send.amount),
        SendField::Memo => Some(&mut app.send.memo),
        SendField::From | SendField::PrivacyPolicy | SendField::Submit => None,
    }
}

#[cfg(test)]
mod tests {
    use super::is_valid_amount;

    #[test]
    fn accepts_well_formed_positive_amounts() {
        for s in ["1", "0.1", "1.23456789", "100", "0.00000001"] {
            assert!(is_valid_amount(s), "{s} should be valid");
        }
    }

    #[test]
    fn rejects_malformed_or_nonpositive_amounts() {
        for s in [
            "",            // empty
            "0",           // zero
            "0.0",         // zero
            "-1",          // negative
            "1.",          // trailing dot, empty fraction
            ".5",          // missing whole part
            "1.234567890", // too many fractional digits
            "1e3",         // scientific notation
            "inf",         // non-finite
            "NaN",         // non-finite
            "abc",         // not a number
            "1.2.3",       // two dots
        ] {
            assert!(!is_valid_amount(s), "{s} should be rejected");
        }
    }

    use proptest::prelude::*;

    proptest! {
        /// Every canonical fixed-point decimal (≤8 fractional digits, non-zero) is accepted.
        #[test]
        fn accepts_canonical_decimals(whole in 0u64..1_000_000, frac in 0u32..100_000_000) {
            prop_assume!(whole != 0 || frac != 0);
            let amount = format!("{whole}.{frac:08}");
            prop_assert!(is_valid_amount(&amount));
        }

        /// More than 8 fractional digits is always rejected (exceeds zatoshi precision).
        #[test]
        fn rejects_excess_fractional_digits(
            whole in 0u64..1_000_000,
            frac in 100_000_000u64..u64::MAX,
        ) {
            let amount = format!("{whole}.{frac}");
            prop_assert!(!is_valid_amount(&amount));
        }

        /// Validation terminates without panicking on any input.
        #[test]
        fn never_panics(s in ".*") {
            let _ = is_valid_amount(&s);
        }
    }
}
