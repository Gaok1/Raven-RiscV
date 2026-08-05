// ui/view/tlb/stats.rs — TLB metrics + hit-rate history chart + the shared
// session-snapshot history (captured with `s`, same list as the Cache tab).

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Axis, Chart, Dataset, GraphType, Paragraph},
};

use crate::ui::app::App;
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::style;

pub(super) fn render_stats(f: &mut Frame, area: Rect, app: &App) {
    let history_h = if app.cache.session_history.is_empty() {
        0
    } else {
        (app.cache.session_history.len() as u16 + 2).min(6)
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Min(8),
            Constraint::Length(history_h), // snapshot history (0 = hidden)
        ])
        .split(area);

    render_stats_metrics(f, layout[0], app);
    render_hit_chart(f, layout[1], app);
    if history_h > 0 {
        render_history_table(f, layout[2], app);
    }
}

/// Session snapshots, TLB lens: same shared list as the Cache tab, but the
/// columns show the translation-side numbers.
fn render_history_table(f: &mut Frame, area: Rect, app: &App) {
    let is_running = app.session.is_running;
    // Why the list is inert is *state*, so it goes in the right-hand slot the
    // panel keeps for exactly that.
    let state = if is_running { "pause to inspect" } else { "" };
    let inner = render_panel(
        f,
        area,
        panel::panel_state("Snapshots", state, PanelKind::Plain),
    );

    if inner.height == 0 {
        return;
    }

    let history = &app.cache.session_history;
    let scroll = app.cache.history_scroll;
    let visible = inner.height as usize;

    // Scroll the view so the selected entry is always visible.
    let start = if scroll + 1 > visible {
        scroll + 1 - visible
    } else {
        0
    };

    for (i, snap) in history.iter().enumerate().skip(start).take(visible) {
        let row = (i - start) as u16;
        if row >= inner.height {
            break;
        }

        let text = match &snap.tlb {
            Some(t) => format!(
                "  {:<14}  TLB: {:.1}%  Hits: {}  Misses: {}  Page Faults: {}  Evictions: {}",
                snap.label,
                t.hit_rate(),
                t.hits,
                t.misses,
                t.page_faults,
                t.evictions
            ),
            None => format!("  {:<14}  (VM was off during this window)", snap.label),
        };

        let is_selected = i == scroll;
        let style = if is_running {
            // Entries are greyed out while running — Enter is disabled.
            if is_selected {
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(Color::DarkGray)
            }
        } else if is_selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(theme::TEXT)
        };

        f.render_widget(
            Paragraph::new(Span::styled(text, style)),
            Rect::new(inner.x, inner.y + row, inner.width, 1),
        );
    }
}

fn render_stats_metrics(f: &mut Frame, area: Rect, app: &App) {
    let Some(translation) = app.translation() else {
        return;
    };
    let stats = translation.tlb_stats();
    let hit_rate = stats.hit_rate();
    let valid_entries = (0..translation.tlb_len())
        .filter_map(|i| translation.tlb_entry(i))
        .filter(|entry| entry.valid)
        .count();
    // Three grouped lines instead of seven stacked ones. The list ran down the
    // left edge of a 150-column panel with each label padded to its own guessed
    // width — so `Page Faults:` and `Valid Entries:` overran the column the
    // rows above had set up, and the chart below lost four rows to the gap.
    let metric = |label: &str, value: String, color: Color| {
        vec![
            Span::styled(label.to_string(), style::idle()),
            Span::styled(
                format!(" {value}"),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ]
    };
    let gap = || Span::raw("    ");

    let hit_color = if hit_rate >= 80.0 {
        theme::RUNNING
    } else if hit_rate >= 50.0 {
        theme::ACCENT
    } else {
        theme::PAUSED
    };
    let fault_color = if stats.faults > 0 {
        theme::DANGER
    } else {
        theme::TEXT
    };
    // Ways follow from the geometry the backend reports, so this stays right
    // for a fully-associative or direct-mapped TLB.
    let ways = translation.tlb_len() / translation.tlb_sets().max(1);

    let mut line1 = vec![Span::raw(" ")];
    line1.extend(metric("hits", stats.hits.to_string(), theme::TEXT));
    line1.push(gap());
    line1.extend(metric("misses", stats.misses.to_string(), theme::TEXT));
    line1.push(gap());
    line1.extend(metric("hit rate", format!("{hit_rate:.1}%"), hit_color));

    let mut line2 = vec![Span::raw(" ")];
    line2.extend(metric("evictions", stats.evictions.to_string(), theme::TEXT));
    line2.push(gap());
    line2.extend(metric("page faults", stats.faults.to_string(), fault_color));
    line2.push(gap());
    line2.extend(metric(
        "valid entries",
        format!("{valid_entries} / {}", translation.tlb_len()),
        theme::TEXT,
    ));

    let mut line3 = vec![Span::raw(" ")];
    line3.extend(metric(
        "geometry",
        format!("{} sets × {ways} ways", translation.tlb_sets()),
        theme::LABEL,
    ));

    let inner = render_panel(f, area, panel::panel("TLB Metrics", PanelKind::Plain));
    f.render_widget(
        Paragraph::new(vec![
            Line::from(line1),
            Line::from(line2),
            Line::raw(""),
            Line::from(line3),
        ]),
        inner,
    );
}

fn render_hit_chart(f: &mut Frame, area: Rect, app: &App) {
    let block = panel::panel("Hit Rate History", PanelKind::Plain);

    let pts: Vec<(f64, f64)> = app
        .translation()
        .map(|translation| translation.hit_rate_history())
        .unwrap_or_default();
    if pts.is_empty() {
        let inner = block.inner(area);
        f.render_widget(block, area);
        let msg = if !app.session.vm_enabled() {
            "  (enable Virtual Memory in Settings to populate the TLB)"
        } else if !super::translation_active(app) {
            "  (vm=on but translation inactive — see Status subview for why)"
        } else {
            "  (no data yet — run a program that touches paged memory)"
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                msg,
                Style::default().fg(theme::LABEL),
            ))),
            inner,
        );
        return;
    }
    if area.height < 5 {
        // Have data, but the panel is too short to render a chart. Show a
        // distinct hint instead of the misleading "no data" copy.
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  (chart hidden — resize terminal for more height)",
                Style::default().fg(theme::LABEL),
            ))),
            inner,
        );
        return;
    }
    let x_min = pts.first().map(|p| p.0).unwrap_or(0.0);
    let x_max = pts.last().map(|p| p.0).unwrap_or(1.0).max(x_min + 1.0);
    let datasets = vec![
        Dataset::default()
            .name("hit %")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(theme::RUNNING))
            .data(&pts),
    ];
    let chart = Chart::new(datasets)
        .block(block)
        .x_axis(
            Axis::default()
                .style(Style::default().fg(theme::BORDER))
                .bounds([x_min, x_max])
                .labels(vec![
                    Span::raw(format!("{:.0}", x_min)),
                    Span::raw(format!("{:.0}", x_max)),
                ]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(theme::BORDER))
                .bounds([0.0, 100.0])
                .labels(vec![Span::raw("0%"), Span::raw("100%")]),
        );
    f.render_widget(chart, area);
}
