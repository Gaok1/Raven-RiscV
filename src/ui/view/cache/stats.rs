// ui/view/cache/stats.rs — Cache statistics subtab
use ratatui::{
    Frame,
    prelude::*,
    widgets::{Axis, Chart, Dataset, Gauge, GraphType, Paragraph},
};

use crate::ui::app::{App, CacheScope};
use crate::ui::theme;
use crate::ui::view::components::overlay::{self, OverlayStyle};
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{SbGeom, vertical_scrollbar};
use crate::ui::view::style;
use raven_engine::capability::CacheRole;

// Note: Reset/Pause/Scope controls are in the shared controls bar (mod.rs).
// Run Controls widget is rendered at the cache tab level (always visible).

pub(super) fn render_stats(f: &mut Frame, area: Rect, app: &App) {
    let levels = app
        .cache_hierarchy()
        .map_or(0, |caches| caches.level_count());
    if app.cache.selected_level == 0 {
        render_l1_stats(f, area, app);
    } else if app.cache.selected_level < levels {
        render_unified_stats(f, area, app, app.cache.selected_level - 1);
    }
}

fn render_l1_stats(f: &mut Frame, area: Rect, app: &App) {
    let history_h = if app.cache.session_history.is_empty() {
        0
    } else {
        (app.cache.session_history.len() as u16 + 2).min(6)
    };

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11),        // cache metrics
            Constraint::Length(1),         // program summary line
            Constraint::Min(8),            // chart
            Constraint::Length(history_h), // history panel (0 = hidden)
        ])
        .split(area);

    render_metrics(f, layout[0], app);
    render_program_summary(f, layout[1], app);
    render_chart(f, layout[2], app);
    if history_h > 0 {
        render_history_table(f, layout[3], app);
    }
}

fn render_unified_stats(f: &mut Frame, area: Rect, app: &App, extra_idx: usize) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11), // metric block
            Constraint::Length(1),  // program summary line
            Constraint::Min(8),     // hit rate chart
        ])
        .split(area);

    render_unified_metrics(f, layout[0], app, extra_idx);
    render_program_summary(f, layout[1], app);
    render_unified_chart(f, layout[2], app, extra_idx);
}

fn render_program_summary(f: &mut Frame, area: Rect, app: &App) {
    let (total, cpi, ipc, instr) = if let Some(pipeline) = app.aggregate_pipeline_snapshot() {
        let cycles = pipeline.cycles;
        let committed = pipeline.committed;
        let cpi = if committed > 0 {
            cycles as f64 / committed as f64
        } else {
            0.0
        };
        let ipc = if cycles > 0 {
            committed as f64 / cycles as f64
        } else {
            0.0
        };
        (cycles, cpi, ipc, committed)
    } else {
        // Without a pipeline model, the hierarchy's own cycle count is the
        // program total, and instructions come from the machine.
        let cycles = app.cache_total_cycles();
        let committed = app.instructions_retired();
        let cpi = if committed == 0 {
            0.0
        } else {
            cycles as f64 / committed as f64
        };
        let ipc = if cycles == 0 { 0.0 } else { 1.0 / cpi };
        (cycles, cpi, ipc, committed)
    };
    let role_cycles = |role| {
        app.cache_hierarchy()
            .and_then(|caches| caches.cache(0, role))
            .map_or(0, |cache| cache.stats.total_cycles)
    };
    let i_cyc = role_cycles(CacheRole::Instruction);
    let d_cyc = role_cycles(CacheRole::Data);

    let mut spans = vec![Span::styled(
        if app.aggregate_pipeline_snapshot().is_some() {
            " program total (pipeline aggregate)"
        } else {
            " program total"
        },
        style::label(),
    )];
    for item in [
        style::readout("cyc", total, theme::METRIC_CYC),
        style::readout("CPI", format!("{cpi:.2}"), theme::METRIC_CPI),
        style::readout("IPC", format!("{ipc:.2}"), theme::METRIC_IPC),
        style::readout("instr", instr, theme::TEXT),
        style::readout("I-cache svc", i_cyc, theme::CACHE_I),
        style::readout("D-cache svc", d_cyc, theme::CACHE_D),
    ] {
        spans.push(Span::raw("    "));
        spans.extend(item);
    }

    // Every level past L1, named by the hierarchy rather than a table here, so
    // a backend with three levels shows three.
    if let Some(caches) = app.cache_hierarchy() {
        for level in 1..caches.level_count() {
            let Some(cache) = caches.cache(level, CacheRole::Unified) else {
                continue;
            };
            spans.push(Span::raw("    "));
            spans.extend(style::readout(
                &format!("{} svc", caches.level_name(level)),
                cache.stats.total_cycles,
                theme::CACHE_L2,
            ));
        }
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_metrics(f: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let scope = app.cache.scope;
    if matches!(scope, CacheScope::ICache | CacheScope::Both) {
        render_cache_metrics(f, cols[0], app, true);
    }
    if matches!(scope, CacheScope::DCache | CacheScope::Both) {
        let target = if matches!(scope, CacheScope::Both) {
            cols[1]
        } else {
            cols[0]
        };
        render_cache_metrics(f, target, app, false);
    }
}

fn render_cache_metrics(f: &mut Frame, area: Rect, app: &App, icache: bool) {
    let (label, role) = if icache {
        ("I-Cache", CacheRole::Instruction)
    } else {
        ("D-Cache", CacheRole::Data)
    };
    let Some(cache) = app
        .cache_hierarchy()
        .and_then(|caches| caches.cache(0, role))
    else {
        return;
    };
    let instructions = app.instructions_retired();
    let stats = &cache.stats;
    let cfg = &cache.config;

    let hit_rate = stats.hit_rate();
    let hit_color = if hit_rate >= 90.0 {
        theme::RUNNING
    } else if hit_rate >= 70.0 {
        theme::PAUSED
    } else {
        theme::DANGER
    };

    let inner = render_panel(f, area, panel::panel(label, PanelKind::Accent));

    if inner.height == 0 {
        return;
    }

    // Line 1: Hit rate gauge
    let gauge_area = Rect::new(inner.x, inner.y, inner.width, 1);
    let hit_u8 = hit_rate.clamp(0.0, 100.0) as u16;
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(hit_color))
        .percent(hit_u8)
        .label(format!("Hit {hit_rate:.1}%"));
    f.render_widget(gauge, gauge_area);

    if inner.height < 2 {
        return;
    }

    let total = stats.total_accesses();
    let miss_rate = if total == 0 { 0.0 } else { 100.0 - hit_rate };
    let fills = if cfg.enabled && cfg.line_size > 0 {
        stats.bytes_loaded / cfg.line_size as u64
    } else {
        0
    };
    let cycles = stats.total_cycles;
    let avg = if total == 0 {
        0.0_f64
    } else {
        cycles as f64 / total as f64
    };
    let cpi_contrib = if instructions == 0 {
        0.0_f64
    } else {
        cycles as f64 / instructions as f64
    };
    let hit_cyc = cfg.tag_search_cycles();
    let miss_cyc = hit_cyc + cfg.miss_penalty + cfg.line_transfer_cycles();
    let amat = app
        .cache_hierarchy()
        .map_or(0.0, |caches| caches.amat(0, role));

    // Grouped `label value` readouts — counts, then traffic, then timing. This
    // was eight flat `"Hits: 0  Misses: 0  Miss Rate: 0.0%"` strings, each one
    // painted a single colour, so the numbers a reader is actually after were
    // typographically identical to the twenty words naming them.
    use style::readout as ro;
    let mut rows = vec![
        style::readout_row(vec![
            ro("hits", stats.hits, theme::TEXT),
            ro("misses", stats.misses, theme::TEXT),
            ro("miss rate", format!("{miss_rate:.1}%"), hit_color),
            ro(
                "MPKI",
                format!("{:.1}", stats.mpki(instructions)),
                theme::TEXT,
            ),
        ]),
        style::readout_row(vec![
            ro("accesses", total, theme::TEXT),
            ro("evictions", stats.evictions, theme::TEXT),
            ro("line fills", fills, theme::TEXT),
        ]),
    ];
    // Writebacks and CPU stores only exist on a cache the program writes to.
    let mut traffic = vec![
        ro("RAM read", fmt_bytes(stats.bytes_loaded), theme::METRIC_CYC),
        ro(
            "RAM written",
            fmt_bytes(stats.ram_write_bytes),
            theme::METRIC_CYC,
        ),
    ];
    if !icache {
        traffic.push(ro(
            "CPU stored",
            fmt_bytes(stats.bytes_stored),
            theme::METRIC_CYC,
        ));
        traffic.push(ro("writebacks", stats.writebacks, theme::LABEL));
    }
    rows.push(style::readout_row(traffic));
    rows.push(style::readout_row(vec![
        ro("service", format!("{cycles} cyc"), theme::METRIC_CPI),
        ro("per access", format!("{avg:.2}"), theme::METRIC_CPI),
        ro("per instr", format!("{cpi_contrib:.2}"), theme::METRIC_CPI),
    ]));
    rows.push(style::readout_row(vec![
        ro("hit", format!("{hit_cyc} cyc"), theme::LABEL),
        ro("miss", format!("{miss_cyc} cyc"), theme::LABEL),
        ro("AMAT", format!("{amat:.2} cyc"), theme::CACHE_L2),
    ]));

    let body = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        inner.height.saturating_sub(1),
    );
    f.render_widget(Paragraph::new(rows), body);
}

fn render_history_table(f: &mut Frame, area: Rect, app: &App) {
    let is_running = app.session.is_running;
    let title = if is_running {
        " Snapshots (\u{23f8} to view) "
    } else {
        " Snapshots "
    };
    let inner = render_panel(f, area, panel::panel(title, PanelKind::Plain));

    if inner.height == 0 {
        return;
    }

    let history = &app.cache.session_history;
    let scroll = app.cache.history_scroll;
    let visible = inner.height as usize;

    // Scroll the view so the selected entry is always visible
    let start = if scroll + 1 > visible {
        scroll + 1 - visible
    } else {
        0
    };

    // Reserve a right column for the scrollbar when the list overflows.
    let needs_sb = history.len() > visible;
    let text_w = inner.width.saturating_sub(u16::from(needs_sb));

    for (i, snap) in history.iter().enumerate().skip(start).take(visible) {
        let row = (i - start) as u16;
        if row >= inner.height {
            break;
        }

        let i_total = snap.icache.hits + snap.icache.misses;
        let i_hit = if i_total == 0 {
            0.0
        } else {
            snap.icache.hits as f64 / i_total as f64 * 100.0
        };
        let d_total = snap.dcache.hits + snap.dcache.misses;
        let d_hit = if d_total == 0 {
            0.0
        } else {
            snap.dcache.hits as f64 / d_total as f64 * 100.0
        };
        let total_misses = snap.icache.misses + snap.dcache.misses;
        let mpki = if snap.instruction_count == 0 {
            0.0
        } else {
            total_misses as f64 / snap.instruction_count as f64 * 1000.0
        };
        let amat_i = snap.icache.amat;
        let cyc = snap.total_cycles;

        let is_selected = i == scroll;

        let text = format!(
            "  {:<14}  I-Cache: {:.1}%  D-Cache: {:.1}%  Miss/1K: {:.1}  Access Time: {:.2}  Total: {}",
            snap.label, i_hit, d_hit, mpki, amat_i, cyc
        );

        let style = if is_running {
            // Entries are greyed out while running — Enter is disabled
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
            style::value()
        };

        f.render_widget(
            Paragraph::new(Span::styled(text, style)),
            Rect::new(inner.x, inner.y + row, text_w, 1),
        );
    }

    // Register the bar for mouse click-to-jump + thumb drag; dragging moves
    // the *window* (so the thumb stays glued to the cursor) and the mouse
    // handler pins the selection to the window's edge.
    if needs_sb {
        vertical_scrollbar(f, inner, history.len(), visible, start);
        app.cache.history_sb.set(Some(SbGeom {
            start: inner.y,
            len: inner.height,
            cross: inner.x + inner.width.saturating_sub(1),
            content: history.len(),
            viewport: visible,
            offset: start,
            max: history.len() - visible,
        }));
    }
}

fn render_chart(f: &mut Frame, area: Rect, app: &App) {
    let inner = render_panel(
        f,
        area,
        panel::panel("Hit Rate History (%)", PanelKind::Plain),
    );

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let scope = app.cache.scope;
    let history = |role| {
        app.cache_hierarchy()
            .and_then(|caches| caches.cache(0, role))
            .map(|cache| cache.stats.history.iter().copied().collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let i_data = history(CacheRole::Instruction);
    let d_data = history(CacheRole::Data);

    if i_data.is_empty() && d_data.is_empty() {
        let msg = Paragraph::new("No data yet — run the program to collect cache statistics.")
            .style(style::label())
            .alignment(Alignment::Center);
        f.render_widget(msg, inner);
        return;
    }

    let mut datasets = Vec::new();
    let show_i = matches!(scope, CacheScope::ICache | CacheScope::Both) && !i_data.is_empty();
    let show_d = matches!(scope, CacheScope::DCache | CacheScope::Both) && !d_data.is_empty();

    if show_i {
        datasets.push(
            Dataset::default()
                .name("I-Cache")
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(theme::CACHE_I))
                .data(&i_data),
        );
    }
    if show_d {
        datasets.push(
            Dataset::default()
                .name("D-Cache")
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(theme::CACHE_D))
                .data(&d_data),
        );
    }

    let mut x_min = None::<f64>;
    let mut x_max = None::<f64>;
    for series in [
        (show_i, i_data.first(), i_data.last()),
        (show_d, d_data.first(), d_data.last()),
    ] {
        let (enabled, first, last) = series;
        if !enabled {
            continue;
        }
        if let Some((x, _)) = first {
            x_min = Some(x_min.map_or(*x, |m| m.min(*x)));
        }
        if let Some((x, _)) = last {
            x_max = Some(x_max.map_or(*x, |m| m.max(*x)));
        }
    }

    let x_min = x_min.unwrap_or(0.0);
    let mut x_max = x_max.unwrap_or(x_min + 1.0);
    if x_max <= x_min {
        x_max = x_min + 1.0;
    }
    let x_mid = (x_min + x_max) / 2.0;

    let chart = Chart::new(datasets)
        .x_axis(
            Axis::default()
                .style(Style::default().fg(theme::BORDER))
                .bounds([x_min, x_max])
                .labels(vec![
                    Span::raw(format!("{x_min:.0}")),
                    Span::raw(format!("{x_mid:.0}")),
                    Span::raw(format!("{x_max:.0}")),
                ]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(theme::BORDER))
                .bounds([0.0, 100.0])
                .labels(vec![Span::raw("0%"), Span::raw("50%"), Span::raw("100%")]),
        );
    f.render_widget(chart, inner);
}

fn render_unified_metrics(f: &mut Frame, area: Rect, app: &App, extra_idx: usize) {
    let level = extra_idx + 1;
    let Some(caches) = app.cache_hierarchy() else {
        return;
    };
    let Some(cache) = caches.cache(level, CacheRole::Unified) else {
        return;
    };
    let label = format!("{} (Unified)", caches.level_name(level));
    let stats = &cache.stats;
    let cfg = &cache.config;
    let instructions = app.instructions_retired();

    let hit_rate = stats.hit_rate();
    let hit_color = if hit_rate >= 90.0 {
        theme::RUNNING
    } else if hit_rate >= 70.0 {
        theme::PAUSED
    } else {
        theme::DANGER
    };

    let inner = render_panel(f, area, panel::panel(label, PanelKind::Accent));

    if inner.height == 0 {
        return;
    }

    // Line 1: Hit rate gauge
    let gauge_area = Rect::new(inner.x, inner.y, inner.width, 1);
    let hit_u8 = hit_rate.clamp(0.0, 100.0) as u16;
    f.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(hit_color))
            .percent(hit_u8)
            .label(format!("Hit {hit_rate:.1}%")),
        gauge_area,
    );
    if inner.height < 2 {
        return;
    }

    // Same grouped readouts as the split L1 panels above — one shape for one
    // kind of information, whichever level is on screen.
    let total = stats.total_accesses();
    let miss_rate = if total == 0 { 0.0 } else { 100.0 - hit_rate };
    let fills = if cfg.enabled && cfg.line_size > 0 {
        stats.bytes_loaded / cfg.line_size as u64
    } else {
        0
    };
    let cycles = stats.total_cycles;
    let avg = if total == 0 {
        0.0_f64
    } else {
        cycles as f64 / total as f64
    };
    let cpi_contrib = if instructions == 0 {
        0.0_f64
    } else {
        cycles as f64 / instructions as f64
    };
    let hit_cyc = cfg.tag_search_cycles();
    let miss_cyc = hit_cyc + cfg.miss_penalty + cfg.line_transfer_cycles();
    let amat = app
        .cache_hierarchy()
        .map_or(0.0, |caches| caches.amat(extra_idx + 1, CacheRole::Unified));

    use style::readout as ro;
    let rows = vec![
        style::readout_row(vec![
            ro("hits", stats.hits, theme::TEXT),
            ro("misses", stats.misses, theme::TEXT),
            ro("miss rate", format!("{miss_rate:.1}%"), theme::TEXT),
            ro(
                "MPKI",
                format!("{:.1}", stats.mpki(instructions)),
                theme::TEXT,
            ),
        ]),
        style::readout_row(vec![
            ro("accesses", total, theme::TEXT),
            ro("evictions", stats.evictions, theme::TEXT),
            ro("writebacks", stats.writebacks, theme::TEXT),
            ro("line fills", fills, theme::TEXT),
        ]),
        style::readout_row(vec![
            ro("RAM read", fmt_bytes(stats.bytes_loaded), theme::METRIC_CYC),
            ro(
                "RAM written",
                fmt_bytes(stats.ram_write_bytes),
                theme::METRIC_CYC,
            ),
        ]),
        style::readout_row(vec![
            ro("service", format!("{cycles} cyc"), theme::METRIC_CPI),
            ro("per access", format!("{avg:.2}"), theme::METRIC_CPI),
            ro("per instr", format!("{cpi_contrib:.2}"), theme::METRIC_CPI),
        ]),
        style::readout_row(vec![
            ro("hit", format!("{hit_cyc} cyc"), theme::LABEL),
            ro("miss", format!("{miss_cyc} cyc"), theme::LABEL),
            ro("AMAT", format!("{amat:.2} cyc"), theme::CACHE_L2),
        ]),
    ];

    f.render_widget(
        Paragraph::new(rows),
        Rect::new(
            inner.x,
            inner.y + 1,
            inner.width,
            inner.height.saturating_sub(1),
        ),
    );
}

fn render_unified_chart(f: &mut Frame, area: Rect, app: &App, extra_idx: usize) {
    let inner = render_panel(
        f,
        area,
        panel::panel("Hit Rate History (%)", PanelKind::Plain),
    );

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let data: Vec<(f64, f64)> = app
        .cache_hierarchy()
        .and_then(|caches| caches.cache(extra_idx + 1, CacheRole::Unified))
        .map(|cache| cache.stats.history.iter().copied().collect())
        .unwrap_or_default();

    if data.is_empty() {
        f.render_widget(
            Paragraph::new("No data yet — run the program to collect cache statistics.")
                .style(style::label())
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let x_min = data.first().map(|(x, _)| *x).unwrap_or(0.0);
    let mut x_max = data.last().map(|(x, _)| *x).unwrap_or(x_min + 1.0);
    if x_max <= x_min {
        x_max = x_min + 1.0;
    }
    let x_mid = (x_min + x_max) / 2.0;

    let level_name = app.cache_hierarchy().map_or_else(
        || format!("L{}", extra_idx + 2),
        |caches| caches.level_name(extra_idx + 1),
    );
    let datasets = vec![
        Dataset::default()
            .name(level_name)
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(theme::CACHE_L2))
            .data(&data),
    ];

    let chart = Chart::new(datasets)
        .x_axis(
            Axis::default()
                .style(Style::default().fg(theme::BORDER))
                .bounds([x_min, x_max])
                .labels(vec![
                    Span::raw(format!("{x_min:.0}")),
                    Span::raw(format!("{x_mid:.0}")),
                    Span::raw(format!("{x_max:.0}")),
                ]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(theme::BORDER))
                .bounds([0.0, 100.0])
                .labels(vec![Span::raw("0%"), Span::raw("50%"), Span::raw("100%")]),
        );
    f.render_widget(chart, inner);
}

pub(in crate::ui::view) fn render_snapshot_popup(f: &mut Frame, area: Rect, app: &App) {
    let idx = match app.cache.viewing_snapshot {
        Some(i) => i,
        None => return,
    };
    let snap = match app.cache.session_history.get(idx) {
        Some(s) => s,
        None => return,
    };

    // Centered popup: 90% width, up to 24 rows tall
    let pop_w = (area.width * 9 / 10).min(110);
    let pop_h = 24u16.min(area.height.saturating_sub(4));
    let popup = Rect::new(
        area.x + (area.width.saturating_sub(pop_w)) / 2,
        area.y + (area.height.saturating_sub(pop_h)) / 2,
        pop_w.min(area.width),
        pop_h.min(area.height),
    );

    let inner = overlay::overlay(
        f,
        popup,
        OverlayStyle {
            border: theme::ACCENT,
            title: Span::styled(
                format!(" Snapshot {} ", snap.label),
                Style::default().fg(theme::ACCENT).bold(),
            ),
            bottom: Some(Line::from(Span::styled(" [esc] close ", style::label()))),
        },
    );

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    // Build lines
    let mut lines: Vec<Line> = Vec::new();

    // ── Program summary ───────────────────────────────────────────────────────
    lines.push(Line::from(vec![
        Span::styled("Program  ", style::label()),
        Span::styled(
            format!("Cycles: {}", snap.total_cycles),
            style::metric(style::Metric::Cycles),
        ),
        Span::raw("   "),
        Span::styled(
            format!("CPI: {:.2}", snap.cpi),
            style::metric(style::Metric::Cpi),
        ),
        Span::raw("   "),
        Span::styled(
            format!("IPC: {:.2}", snap.ipc),
            style::metric(style::Metric::Ipc),
        ),
        Span::raw("   "),
        Span::styled(
            format!("Instructions: {}", snap.instruction_count),
            style::label(),
        ),
    ]));
    lines.push(Line::raw(""));

    // ── Per-level helper closure ──────────────────────────────────────────────
    let level_lines = |lvl: &crate::ui::app::LevelSnapshot, label: &str| -> Vec<Line<'static>> {
        let total = lvl.hits + lvl.misses;
        let hit_pct = if total == 0 {
            0.0
        } else {
            lvl.hits as f64 / total as f64 * 100.0
        };
        let mpki = if snap.instruction_count == 0 {
            0.0
        } else {
            lvl.misses as f64 / snap.instruction_count as f64 * 1000.0
        };
        let label = label.to_string();
        vec![
            Line::from(vec![
                Span::styled(format!("{label:<9}"), Style::default().fg(theme::ACCENT)),
                Span::styled(
                    format!("Hit: {hit_pct:.1}%"),
                    Style::default().fg(if hit_pct >= 90.0 {
                        theme::RUNNING
                    } else if hit_pct >= 70.0 {
                        theme::PAUSED
                    } else {
                        theme::DANGER
                    }),
                ),
                Span::raw("   "),
                Span::styled(
                    format!("Hits: {}  Misses: {}", lvl.hits, lvl.misses),
                    style::value(),
                ),
                Span::raw("   "),
                Span::styled(format!("Miss/1K: {mpki:.1}"), style::label()),
                Span::raw("   "),
                Span::styled(
                    format!("AMAT: {:.2} cyc", lvl.amat),
                    style::metric(style::Metric::Cpi),
                ),
            ]),
            Line::from(vec![
                Span::raw("          "),
                Span::styled(
                    format!(
                        "Svc Cycles: {}   Evictions: {}   RAM: {} read / {} written",
                        lvl.total_cycles,
                        lvl.evictions,
                        fmt_bytes(lvl.bytes_loaded),
                        fmt_bytes(lvl.ram_write_bytes)
                    ),
                    style::label(),
                ),
            ]),
        ]
    };

    for l in level_lines(&snap.icache, "I-Cache") {
        lines.push(l);
    }
    lines.push(Line::raw(""));
    for l in level_lines(&snap.dcache, "D-Cache") {
        lines.push(l);
    }

    for (i, extra) in snap.extra_levels.iter().enumerate() {
        lines.push(Line::raw(""));
        let name = format!("L{}", i + 2);
        for l in level_lines(extra, &name) {
            lines.push(l);
        }
    }

    // ── TLB / virtual memory ──────────────────────────────────────────────────
    if let Some(t) = &snap.tlb {
        let hit_pct = t.hit_rate();
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("TLB      ", Style::default().fg(theme::ACCENT)),
            Span::styled(
                format!("Hit: {hit_pct:.1}%"),
                Style::default().fg(if hit_pct >= 90.0 {
                    theme::RUNNING
                } else if hit_pct >= 70.0 {
                    theme::PAUSED
                } else {
                    theme::DANGER
                }),
            ),
            Span::raw("   "),
            Span::styled(
                format!("Hits: {}  Misses: {}", t.hits, t.misses),
                Style::default().fg(theme::TEXT),
            ),
            Span::raw("   "),
            Span::styled(
                format!("Page Faults: {}", t.page_faults),
                Style::default().fg(if t.page_faults > 0 {
                    theme::DANGER
                } else {
                    theme::LABEL
                }),
            ),
            Span::raw("   "),
            Span::styled(
                format!("Evictions: {}", t.evictions),
                Style::default().fg(theme::LABEL),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("          "),
            Span::styled(
                format!(
                    "vm={} · page {}  · {} levels · {} entries {}-way {} · Svc Cycles: {}",
                    t.vm_mode,
                    fmt_bytes(t.page_size),
                    t.levels,
                    t.entry_count,
                    t.associativity,
                    t.replacement,
                    t.total_cycles
                ),
                Style::default().fg(theme::LABEL),
            ),
        ]));
    }

    // ── Miss hotspots ─────────────────────────────────────────────────────────
    if !snap.miss_hotspots.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "I-Cache miss hotspots (top PCs):",
            style::label(),
        )));
        for (pc, count) in snap.miss_hotspots.iter().take(5) {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("0x{pc:08x}"), Style::default().fg(theme::ACCENT)),
                Span::styled(format!("  ×{count}"), style::value()),
            ]));
        }
    }

    let para = Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false });
    f.render_widget(para, inner);
}

fn fmt_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;

    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KiB", bytes as f64 / KIB)
    } else {
        format!("{:.1}MiB", bytes as f64 / MIB)
    }
}
