// ui/view/tlb/stats.rs — TLB metrics + hit-rate history chart.

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Axis, Block, BorderType, Borders, Chart, Dataset, GraphType, Paragraph},
};

use crate::ui::app::App;
use crate::ui::theme;

pub(super) fn render_stats(f: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(0)])
        .split(area);

    render_stats_metrics(f, layout[0], app);
    render_hit_chart(f, layout[1], app);
}

fn render_stats_metrics(f: &mut Frame, area: Rect, app: &App) {
    let mmu = app.run.mem.mmu();
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

    let pts: Vec<(f64, f64)> = app.run.mem.mmu().tlb.stats.history.iter().copied().collect();
    if pts.is_empty() {
        let inner = block.inner(area);
        f.render_widget(block, area);
        let msg = if !app.run.vm_enabled {
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
