//! Per-account balances view, with a `minconf` control.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap},
};

use crate::app::App;
use crate::format::{short_uuid, zec_decimal};

pub(crate) fn on_key(app: &mut App, key: KeyEvent) {
    match key.code {
        // Adjust minconf with +/- (and the unshifted '=' for convenience).
        KeyCode::Char('+' | '=') => {
            app.data.minconf = app.data.minconf.saturating_add(1);
        }
        KeyCode::Char('-') => {
            app.data.minconf = app.data.minconf.saturating_sub(1);
        }
        _ => {}
    }
}

pub(crate) fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let header = Row::new(vec![
        Cell::from(crate::fl!("tui-bal-header-account")),
        Cell::from(crate::fl!("tui-bal-header-transparent")),
        Cell::from(crate::fl!("tui-bal-header-sapling")),
        Cell::from(crate::fl!("tui-bal-header-orchard")),
        Cell::from(crate::fl!("tui-bal-header-ironwood")),
        Cell::from(crate::fl!("tui-bal-header-pending")),
        Cell::from(crate::fl!("tui-bal-header-total")),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row<'_>> = match &app.data.balances {
        Some(balances) => balances
            .accounts
            .iter()
            .map(|acct| {
                let name = app
                    .data
                    .accounts
                    .iter()
                    .find(|a| a.account_uuid == acct.account_uuid)
                    .and_then(|a| a.name.clone())
                    .unwrap_or_else(|| short_uuid(&acct.account_uuid));
                // Pool columns show the spendable balance; the dedicated Pending column
                // carries everything not yet spendable, so Total = pools + Pending.
                Row::new(vec![
                    Cell::from(name),
                    Cell::from(zec(acct.transparent_zat())),
                    Cell::from(zec(acct.sapling_zat())),
                    Cell::from(zec(acct.orchard_zat())),
                    Cell::from(zec(acct.ironwood_zat())),
                    Cell::from(zec(acct.pending_total_zat())),
                    Cell::from(zec(acct.total_zat())),
                ])
            })
            .collect(),
        None => Vec::new(),
    };

    let title = format!(
        " {} ",
        crate::fl!("tui-bal-title", minconf = app.data.minconf)
    );

    if app.data.balances_syncing {
        let p = Paragraph::new(crate::fl!("tui-bal-syncing"))
            .block(Block::default().borders(Borders::ALL).title(title))
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(p, area);
        return;
    }

    if rows.is_empty() {
        let p = Paragraph::new(crate::fl!("tui-bal-empty"))
            .block(Block::default().borders(Borders::ALL).title(title))
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, area);
        return;
    }

    // ZIP 318 asks wallets to surface at-risk Orchard balances: since NU6.3 the Orchard
    // pool is sealed, so any spendable Orchard funds can only move (publicly, in amount)
    // into the account's own Ironwood pool. Show a notice whenever such funds exist.
    let orchard_at_risk = app
        .data
        .balances
        .as_ref()
        .is_some_and(|b| b.accounts.iter().any(|a| a.orchard_zat() > 0));
    let (table_area, note_area) = if orchard_at_risk && area.height > 4 {
        let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };
    if let Some(note_area) = note_area {
        let note = Paragraph::new(crate::fl!("tui-bal-orchard-sealed"))
            .style(Style::default().fg(Color::Yellow))
            .wrap(Wrap { trim: true });
        frame.render_widget(note, note_area);
    }
    let area = table_area;

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(22),
            Constraint::Percentage(13),
            Constraint::Percentage(13),
            Constraint::Percentage(13),
            Constraint::Percentage(13),
            Constraint::Percentage(13),
            Constraint::Percentage(13),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(table, area);
}

/// Formats a (non-negative) zatoshi balance as a fixed-point ZEC string.
fn zec(zat: i64) -> String {
    zec_decimal(zat.unsigned_abs())
}
