//! Accounts view.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
};

use crate::app::App;
use crate::client::AccountBalance;
use crate::format::{short_uuid, zec_decimal};

/// State for the Accounts view.
#[derive(Default)]
pub(crate) struct AccountsState {
    /// The selected account, as an index into the accounts list.
    pub(crate) selected: usize,
}

pub(crate) async fn on_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            if app.accounts.selected + 1 < app.data.accounts.len() {
                app.accounts.selected += 1;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.accounts.selected = app.accounts.selected.saturating_sub(1);
        }
        KeyCode::Char('n') => app.open_new_account_prompt(),
        _ => {}
    }
}

pub(crate) fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    if app.data.accounts.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", crate::fl!("tui-accounts-title")));
        let p = ratatui::widgets::Paragraph::new(crate::fl!("tui-accounts-empty"))
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, area);
        return;
    }

    // Build a per-account balance lookup once, keyed by account UUID, so rendering each row
    // is O(1) rather than scanning the full balance list per account.
    let balances: HashMap<&str, &AccountBalance> = app
        .data
        .balances
        .as_ref()
        .map(|b| {
            b.accounts
                .iter()
                .map(|a| (a.account_uuid.as_str(), a))
                .collect()
        })
        .unwrap_or_default();

    let items: Vec<ListItem<'_>> = app
        .data
        .accounts
        .iter()
        .map(|acct| {
            let name = acct
                .name
                .clone()
                .unwrap_or_else(|| crate::fl!("tui-value-unnamed"));
            let balance = account_balance(&balances, &acct.account_uuid);
            let line = Line::from(vec![
                Span::styled(
                    format!("{name:<24}"),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{}  ", short_uuid(&acct.account_uuid)),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(balance, Style::default().fg(Color::Green)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", crate::fl!("tui-accounts-title-list"))),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    state.select(Some(app.accounts.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

/// The account's total balance as a display string, annotated with any pending amount.
fn account_balance(balances: &HashMap<&str, &AccountBalance>, uuid: &str) -> String {
    let Some(acct) = balances.get(uuid) else {
        return String::new();
    };

    let mut label = crate::fl!(
        "tui-amount-zec",
        amount = zec_decimal(acct.total_zat().unsigned_abs())
    );
    let pending = acct.pending_total_zat();
    if pending > 0 {
        label.push_str("  ");
        label.push_str(&crate::fl!(
            "tui-accounts-balance-pending",
            amount = zec_decimal(pending.unsigned_abs())
        ));
    }
    label
}
