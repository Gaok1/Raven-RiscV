// ui/view/cache/stats.rs — Cache statistics subtab
use ratatui::{
    Frame,
    prelude::*,
    widgets::{Axis, Block, BorderType, Borders, Chart, Dataset, Gauge, GraphType, Paragraph},
};

use crate::ui::app::{App, CacheScope};
use crate::ui::theme;

// Note: Reset/Pause/Scope controls are in the shared controls bar (mod.rs).
// Run Controls widget is rendered at the cache tab level (always visible).

pub(super) fn render_stats(f: &mut Frame, area: Rect, app: &App) {
    if app.cache.selected_level == 0 {
        render_l1_stats(f, area, app);
    } else {
        let idx = app.cache.selected_level - 1;
        if idx < app.run.mem.extra_levels.len() {
            render_unified_stats(f, area, app, idx);
        }
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
            Constraint::Length(11), // cache metrics
            Constraint::Length(1),  // program summary line
            Constraint::Min(8),     // chart
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
    let total = app.run.mem.total_program_cycles();
    let cpi   = app.run.mem.overall_cpi();
    let ipc   = app.run.mem.ipc();
    let instr = app.run.mem.instruction_count;
    let i_cyc = app.run.mem.icache.stats.total_cycles;
    let d_cyc = app.run.mem.dcache.stats.total_cycles;

    let mut spans = vec![
        Span::styled(" Program total \u{2014} ", Style::default().fg(theme::LABEL)),
        Span::styled(format!("Cycles: {total}"), Style::default().fg(theme::METRIC_CYC)),
        Span::raw("  "),
        Span::styled(format!("Cycles/Instr: {cpi:.2}"), Style::default().fg(theme::METRIC_CPI)),
        Span::raw("  "),
        Span::styled(format!("Instrs/Cycle: {ipc:.2}"), Style::default().fg(theme::METRIC_IPC)),
        Span::raw("  "),
        Span::styled(format!("Instructions: {instr}"), Style::default().fg(theme::LABEL)),
        Span::raw("  "),
        Span::styled(format!("I-Cache: {i_cyc}"), Style::default().fg(theme::CACHE_I)),
        Span::raw(" + "),
        Span::styled(format!("D-Cache: {d_cyc}"), Style::default().fg(theme::CACHE_D)),
    ];

    for (i, lvl) in app.run.mem.extra_levels.iter().enumerate() {
        let name = crate::falcon::cache::CacheController::extra_level_name(i);
        spans.push(Span::raw(" + "));
        spans.push(Span::styled(
            format!("{name}: {}", lvl.stats.total_cycles),
            Style::default().fg(theme::CACHE_L2),
        ));
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
        let target = if matches!(scope, CacheScope::Both) { cols[1] } else { cols[0] };
        render_cache_metrics(f, target, app, false);
    }
}

fn render_cache_metrics(f: &mut Frame, area: Rect, app: &App, icache: bool) {
    let (label, cache, instructions) = if icache {
        ("I-Cache", &app.run.mem.icache, app.run.mem.instruction_count)
    } else {
        ("D-Cache", &app.run.mem.dcache, app.run.mem.instruction_count)
    };
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

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(label, Style::default().fg(theme::ACCENT).bold()));
    let inner = block.inner(area);
    f.render_widget(block, area);

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

    let hits = stats.hits;
    let misses = stats.misses;
    let total = stats.total_accesses();
    let miss_rate = if total == 0 { 0.0 } else { 100.0 - hit_rate };
    let mpki = stats.mpki(instructions);

    // Line 2: hits / misses / miss rate / MPKI
    let line2 = format!(
        "Hits: {hits}  Misses: {misses}  Miss Rate: {miss_rate:.1}%  Misses per 1K Instrs: {mpki:.1}"
    );
    f.render_widget(
        Paragraph::new(Span::styled(line2, Style::default().fg(Color::Gray))),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );

    if inner.height < 3 {
        return;
    }

    // Line 3: accesses / evictions / writebacks / line fills
    let wb_part = if icache {
        String::new()
    } else {
        format!("  Writebacks: {}", stats.writebacks)
    };
    let fills = if cfg.is_valid_config() && cfg.line_size > 0 {
        stats.bytes_loaded / cfg.line_size as u64
    } else {
        0
    };
    let line3 = format!(
        "Accesses: {total}  Evictions: {}{wb_part}  Line Fills: {fills}",
        stats.evictions
    );
    f.render_widget(
        Paragraph::new(Span::styled(line3, Style::default().fg(theme::LABEL))),
        Rect::new(inner.x, inner.y + 2, inner.width, 1),
    );

    if inner.height < 4 {
        return;
    }

    // Line 4: RAM traffic
    let line4 = format!(
        "RAM Reads: {}  RAM Writes: {}",
        fmt_bytes(stats.bytes_loaded),
        fmt_bytes(stats.ram_write_bytes)
    );
    f.render_widget(
        Paragraph::new(Span::styled(line4, Style::default().fg(theme::METRIC_CYC))),
        Rect::new(inner.x, inner.y + 3, inner.width, 1),
    );

    if inner.height < 5 {
        return;
    }

    // Line 5: CPU store bytes (D-cache only)
    if !icache {
        let line5 = format!("CPU Stores:{}", fmt_bytes(stats.bytes_stored));
        f.render_widget(
            Paragraph::new(Span::styled(line5, Style::default().fg(theme::LABEL))),
            Rect::new(inner.x, inner.y + 4, inner.width, 1),
        );
    }

    if inner.height < 6 {
        return;
    }

    // Line 6: cycles/access and CPI contribution
    let cycles = stats.total_cycles;
    let avg = if total == 0 { 0.0_f64 } else { cycles as f64 / total as f64 };
    let cpi_contrib = if instructions == 0 { 0.0_f64 } else { cycles as f64 / instructions as f64 };
    let line6 = format!("Cycles: {cycles}  Average: {avg:.2} cyc/access  Cycles/Instr: {cpi_contrib:.2}");
    f.render_widget(
        Paragraph::new(Span::styled(line6, Style::default().fg(theme::METRIC_CPI))),
        Rect::new(inner.x, inner.y + 5, inner.width, 1),
    );

    if inner.height < 7 {
        return;
    }

    // Line 7: cost model summary
    let hit_cyc  = cfg.tag_search_cycles();
    let miss_cyc = hit_cyc + cfg.miss_penalty + cfg.line_transfer_cycles();
    let line7 = format!("Cost model: Hit={hit_cyc}cyc  Miss={miss_cyc}cyc");
    f.render_widget(
        Paragraph::new(Span::styled(line7, Style::default().fg(theme::LABEL))),
        Rect::new(inner.x, inner.y + 6, inner.width, 1),
    );

    if inner.height < 8 {
        return;
    }

    // Line 8: AMAT
    let amat = if icache { app.run.mem.icache_amat() } else { app.run.mem.dcache_amat() };
    let line8 = format!("Memory Access Time: {amat:.2} cyc");
    f.render_widget(
        Paragraph::new(Span::styled(line8, Style::default().fg(theme::CACHE_L2))),
        Rect::new(inner.x, inner.y + 7, inner.width, 1),
    );
}


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

    // Scroll the view so the selected entry is always visible
    let start = if scroll + 1 > visible { scroll + 1 - visible } else { 0 };

    for (i, snap) in history.iter().enumerate().skip(start).take(visible) {
        let row = (i - start) as u16;
        if row >= inner.height {
            break;
        }

        let i_total = snap.icache.hits + snap.icache.misses;
        let i_hit = if i_total == 0 { 0.0 } else { snap.icache.hits as f64 / i_total as f64 * 100.0 };
        let d_total = snap.dcache.hits + snap.dcache.misses;
        let d_hit = if d_total == 0 { 0.0 } else { snap.dcache.hits as f64 / d_total as f64 * 100.0 };
        let total_misses = snap.icache.misses + snap.dcache.misses;
        let mpki = if snap.instruction_count == 0 { 0.0 }
            else { total_misses as f64 / snap.instruction_count as f64 * 1000.0 };
        let amat_i = snap.icache.amat;
        let cyc = snap.total_cycles;

        let is_selected = i == scroll;

        let text = format!(
            "  {:<14}  I-Cache: {:.1}%  D-Cache: {:.1}%  Miss/1K: {:.1}  Access Time: {:.2}  Cycles: {}",
            snap.label, i_hit, d_hit, mpki, amat_i, cyc
        );

        let style = if is_running {
            // Entries are greyed out while running — Enter is disabled
            if is_selected {
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::REVERSED)
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

fn render_chart(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title("Hit Rate History (%)");
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let scope = app.cache.scope;
    let i_data: Vec<(f64, f64)> = app.run.mem.icache.stats.history.iter().cloned().collect();
    let d_data: Vec<(f64, f64)> = app.run.mem.dcache.stats.history.iter().cloned().collect();

    if i_data.is_empty() && d_data.is_empty() {
        let msg = Paragraph::new("No data yet — run the program to collect cache statistics.")
            .style(Style::default().fg(theme::LABEL))
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
                .labels(vec![
                    Span::raw("0%"),
                    Span::raw("50%"),
                    Span::raw("100%"),
                ]),
        );
    f.render_widget(chart, inner);
}


fn render_unified_metrics(f: &mut Frame, area: Rect, app: &App, extra_idx: usize) {
    let cache = &app.run.mem.extra_levels[extra_idx];
    let level_name = crate::falcon::cache::CacheController::extra_level_name(extra_idx);
    let label = format!("{level_name} (Unified)");
    let stats = &cache.stats;
    let cfg = &cache.config;
    let instructions = app.run.mem.instruction_count;

    let hit_rate = stats.hit_rate();
    let hit_color = if hit_rate >= 90.0 { theme::RUNNING } else if hit_rate >= 70.0 { theme::PAUSED } else { theme::DANGER };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(label, Style::default().fg(theme::ACCENT).bold()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height == 0 { return; }

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
    if inner.height < 2 { return; }

    let hits = stats.hits;
    let misses = stats.misses;
    let total = stats.total_accesses();
    let miss_rate = if total == 0 { 0.0 } else { 100.0 - hit_rate };
    let mpki = stats.mpki(instructions);
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("Hits: {hits}  Misses: {misses}  Miss Rate: {miss_rate:.1}%  Misses per 1K Instrs: {mpki:.1}"),
            Style::default().fg(theme::TEXT),
        )),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );
    if inner.height < 3 { return; }

    let fills = if cfg.is_valid_config() && cfg.line_size > 0 {
        stats.bytes_loaded / cfg.line_size as u64
    } else { 0 };
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("Accesses: {total}  Evictions: {}  Writebacks: {}  Line Fills: {fills}", stats.evictions, stats.writebacks),
            Style::default().fg(theme::LABEL),
        )),
        Rect::new(inner.x, inner.y + 2, inner.width, 1),
    );
    if inner.height < 4 { return; }

    f.render_widget(
        Paragraph::new(Span::styled(
            format!("RAM Reads: {}  RAM Writes: {}", fmt_bytes(stats.bytes_loaded), fmt_bytes(stats.ram_write_bytes)),
            Style::default().fg(theme::METRIC_CYC),
        )),
        Rect::new(inner.x, inner.y + 3, inner.width, 1),
    );
    if inner.height < 5 { return; }

    let cycles = stats.total_cycles;
    let avg = if total == 0 { 0.0_f64 } else { cycles as f64 / total as f64 };
    let cpi_contrib = if instructions == 0 { 0.0_f64 } else { cycles as f64 / instructions as f64 };
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("Cycles: {cycles}  Average: {avg:.2} cyc/access  Cycles/Instr: {cpi_contrib:.2}"),
            Style::default().fg(theme::METRIC_CPI),
        )),
        Rect::new(inner.x, inner.y + 4, inner.width, 1),
    );
    if inner.height < 6 { return; }

    let hit_cyc  = cfg.tag_search_cycles();
    let miss_cyc = hit_cyc + cfg.miss_penalty + cfg.line_transfer_cycles();
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("Cost model: Hit={hit_cyc}cyc  Miss={miss_cyc}cyc"),
            Style::default().fg(theme::LABEL),
        )),
        Rect::new(inner.x, inner.y + 5, inner.width, 1),
    );
    if inner.height < 7 { return; }

    let amat = app.run.mem.extra_level_amat(extra_idx);
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("Memory Access Time: {amat:.2} cyc"),
            Style::default().fg(theme::CACHE_L2),
        )),
        Rect::new(inner.x, inner.y + 6, inner.width, 1),
    );
}

fn render_unified_chart(f: &mut Frame, area: Rect, app: &App, extra_idx: usize) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title("Hit Rate History (%)");
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 3 || inner.width < 10 { return; }

    let data: Vec<(f64, f64)> = app.run.mem.extra_levels[extra_idx]
        .stats.history.iter().cloned().collect();

    if data.is_empty() {
        f.render_widget(
            Paragraph::new("No data yet — run the program to collect cache statistics.")
                .style(Style::default().fg(theme::LABEL))
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let x_min = data.first().map(|(x, _)| *x).unwrap_or(0.0);
    let mut x_max = data.last().map(|(x, _)| *x).unwrap_or(x_min + 1.0);
    if x_max <= x_min { x_max = x_min + 1.0; }
    let x_mid = (x_min + x_max) / 2.0;

    let level_name = crate::falcon::cache::CacheController::extra_level_name(extra_idx);
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
