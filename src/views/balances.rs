//! Per-account balances view, with a `minconf` control.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::app::App;
use crate::format::{short_uuid, zec_decimal};

pub(crate) fn on_key(app: &mut App, key: KeyEvent) {
    match key.code {
        // Adjust minconf with +/- (and the unshifted '=' for convenience).
        KeyCode::Char('+') | KeyCode::Char('=') => {
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

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
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
