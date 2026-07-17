// ui/view/tlb/stats.rs — TLB metrics + hit-rate history chart + the shared
// session-snapshot history (captured with `s`, same list as the Cache tab).

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Axis, Block, BorderType, Borders, Chart, Dataset, GraphType, Paragraph},
};

use crate::ui::app::App;
use crate::ui::theme;

pub(super) fn render_stats(f: &mut Frame, area: Rect, app: &App) {
    let history_h = if app.cache.session_history.is_empty() {
        0
    } else {
        (app.cache.session_history.len() as u16 + 2).min(6)
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9),
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
    let is_running = app.run.is_running;
    let title = if is_running {
        " Snapshots (\u{23f8} to view) "
    } else {
        " Snapshots (\u{2191}\u{2193} \u{b7} Enter=view \u{b7} D=delete) "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(title, Style::default().fg(theme::LABEL)));
    let inner = block.inner(area);
    f.render_widget(block, area);

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
    let mmu = app.run.mem().mmu();
    let stats = &mmu.tlb.stats;
    let total = stats.hits + stats.misses;
    let hit_rate = if total == 0 {
        0.0
    } else {
        stats.hits as f64 / total as f64 * 100.0
    };
    let valid_entries = mmu.tlb.entries.iter().filter(|e| e.valid).count();
    let lines = vec![
        Line::from(vec![
            Span::styled(" Hits:       ", Style::default().fg(theme::LABEL)),
            Span::styled(format!("{}", stats.hits), Style::default().fg(theme::TEXT)),
        ]),
        Line::from(vec![
            Span::styled(" Misses:     ", Style::default().fg(theme::LABEL)),
            Span::styled(
                format!("{}", stats.misses),
                Style::default().fg(theme::TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Hit Rate:   ", Style::default().fg(theme::LABEL)),
            Span::styled(
                format!("{:.1}%", hit_rate),
                Style::default().fg(if hit_rate >= 80.0 {
                    theme::RUNNING
                } else if hit_rate >= 50.0 {
                    theme::ACCENT
                } else {
                    theme::PAUSED
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Evictions:  ", Style::default().fg(theme::LABEL)),
            Span::styled(
                format!("{}", stats.evictions),
                Style::default().fg(theme::TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Page Faults:", Style::default().fg(theme::LABEL)),
            Span::styled(
                format!(" {}", stats.page_faults),
                Style::default().fg(if stats.page_faults > 0 {
                    theme::DANGER
                } else {
                    theme::TEXT
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Valid Entries: ", Style::default().fg(theme::LABEL)),
            Span::styled(
                format!("{} / {}", valid_entries, mmu.tlb.entries.len()),
                Style::default().fg(theme::TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled(" Sets:       ", Style::default().fg(theme::LABEL)),
            Span::styled(
                format!("{}", mmu.tlb.num_sets()),
                Style::default().fg(theme::BORDER),
            ),
            Span::raw("   "),
            Span::styled(" Ways:       ", Style::default().fg(theme::LABEL)),
            Span::styled(
                format!("{}", mmu.tlb.config.associativity),
                Style::default().fg(theme::BORDER),
            ),
        ]),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled("Metrics", Style::default().fg(theme::LABEL)));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_hit_chart(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            "Hit Rate History",
            Style::default().fg(theme::LABEL),
        ));

    let pts: Vec<(f64, f64)> = app.run.mem().mmu().tlb.stats.history.iter().copied().collect();
    if pts.is_empty() {
        let inner = block.inner(area);
        f.render_widget(block, area);
        let msg = if !app.run.vm_enabled() {
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
