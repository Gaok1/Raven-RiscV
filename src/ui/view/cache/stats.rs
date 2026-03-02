// ui/view/cache/stats.rs — Cache statistics subtab
use ratatui::{
    Frame,
    prelude::*,
    widgets::{Axis, Block, BorderType, Borders, Chart, Dataset, Gauge, GraphType, Paragraph},
};

use crate::ui::app::{App, CacheScope};

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
    let has_snap = app.cache.loaded_snapshot.is_some();
    if has_snap {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),  // comparison banner
                Constraint::Length(11), // cache metrics (AMAT + delta lines)
                Constraint::Length(1),  // program summary line
                Constraint::Min(8),     // chart
            ])
            .split(area);
        render_comparison_banner(f, layout[0], app);
        render_metrics(f, layout[1], app);
        render_program_summary(f, layout[2], app);
        render_chart(f, layout[3], app);
    } else {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(11), // cache metrics (includes AMAT line)
                Constraint::Length(1),  // program summary line
                Constraint::Min(8),     // chart
            ])
            .split(area);
        render_metrics(f, layout[0], app);
        render_program_summary(f, layout[1], app);
        render_chart(f, layout[2], app);
    }
}

fn render_comparison_banner(f: &mut Frame, area: Rect, app: &App) {
    if let Some(snap) = &app.cache.loaded_snapshot {
        let line = Line::from(vec![
            Span::styled(" Comparing with: ", Style::default().fg(Color::DarkGray)),
            Span::styled(snap.label.clone(), Style::default().fg(Color::LightBlue).bold()),
            Span::styled("   [c] clear", Style::default().fg(Color::DarkGray)),
        ]);
        f.render_widget(Paragraph::new(line), area);
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
        Span::styled(" Program total \u{2014} ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("Cycles:{total}"), Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled(format!("CPI:{cpi:.2}"), Style::default().fg(Color::Magenta)),
        Span::raw("  "),
        Span::styled(format!("IPC:{ipc:.2}"), Style::default().fg(Color::LightMagenta)),
        Span::raw("  "),
        Span::styled(format!("Instrs:{instr}"), Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(format!("I$:{i_cyc}"), Style::default().fg(Color::Cyan)),
        Span::raw(" + "),
        Span::styled(format!("D$:{d_cyc}"), Style::default().fg(Color::Green)),
    ];

    for (i, lvl) in app.run.mem.extra_levels.iter().enumerate() {
        let name = crate::falcon::cache::CacheController::extra_level_name(i);
        spans.push(Span::raw(" + "));
        spans.push(Span::styled(
            format!("{name}$:{}", lvl.stats.total_cycles),
            Style::default().fg(Color::Yellow),
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
        Color::Green
    } else if hit_rate >= 70.0 {
        Color::Yellow
    } else {
        Color::Red
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(label, Style::default().fg(Color::Cyan).bold()));
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
        "H:{hits}  M:{misses}  MR:{miss_rate:.1}%  MPKI:{mpki:.1}"
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
        format!("  WB:{}", stats.writebacks)
    };
    let fills = if cfg.is_valid_config() && cfg.line_size > 0 {
        stats.bytes_loaded / cfg.line_size as u64
    } else {
        0
    };
    let line3 = format!(
        "Acc:{total}  Evict:{}{wb_part}  Fills:{fills}",
        stats.evictions
    );
    f.render_widget(
        Paragraph::new(Span::styled(line3, Style::default().fg(Color::DarkGray))),
        Rect::new(inner.x, inner.y + 2, inner.width, 1),
    );

    if inner.height < 4 {
        return;
    }

    // Line 4: RAM traffic
    let line4 = format!(
        "RAM R:{}  RAM W:{}",
        fmt_bytes(stats.bytes_loaded),
        fmt_bytes(stats.ram_write_bytes)
    );
    f.render_widget(
        Paragraph::new(Span::styled(line4, Style::default().fg(Color::Cyan))),
        Rect::new(inner.x, inner.y + 3, inner.width, 1),
    );

    if inner.height < 5 {
        return;
    }

    // Line 5: CPU store bytes (D-cache only)
    if !icache {
        let line5 = format!("CPU Stores:{}", fmt_bytes(stats.bytes_stored));
        f.render_widget(
            Paragraph::new(Span::styled(line5, Style::default().fg(Color::DarkGray))),
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
    let line6 = format!("Cycles:{cycles}  Avg:{avg:.2}c/a  CPI:{cpi_contrib:.2}");
    f.render_widget(
        Paragraph::new(Span::styled(line6, Style::default().fg(Color::Magenta))),
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
        Paragraph::new(Span::styled(line7, Style::default().fg(Color::DarkGray))),
        Rect::new(inner.x, inner.y + 6, inner.width, 1),
    );

    if inner.height < 8 {
        return;
    }

    // Line 8: AMAT
    let amat = if icache { app.run.mem.icache_amat() } else { app.run.mem.dcache_amat() };
    let line8 = format!("AMAT:{amat:.2}cyc");
    f.render_widget(
        Paragraph::new(Span::styled(line8, Style::default().fg(Color::Yellow))),
        Rect::new(inner.x, inner.y + 7, inner.width, 1),
    );

    if inner.height < 9 {
        return;
    }

    // Line 9: delta comparison (only when baseline snapshot loaded)
    if let Some(snap) = &app.cache.loaded_snapshot {
        let snap_lvl = if icache { &snap.icache } else { &snap.dcache };
        let snap_total = snap_lvl.hits + snap_lvl.misses;
        let snap_hit_rate = if snap_total == 0 { 0.0 }
            else { snap_lvl.hits as f64 / snap_total as f64 * 100.0 };
        let d_hit = hit_rate - snap_hit_rate;
        let snap_mpki = if instructions == 0 { 0.0 }
            else { snap_lvl.misses as f64 / instructions as f64 * 1000.0 };
        let d_mpki = mpki - snap_mpki;
        let d_amat = amat - snap_lvl.amat;
        let d_cyc = stats.total_cycles as i64 - snap_lvl.total_cycles as i64;
        let sh = if d_hit >= 0.0 { "+" } else { "" };
        let sm = if d_mpki >= 0.0 { "+" } else { "" };
        let sa = if d_amat >= 0.0 { "+" } else { "" };
        let sc = if d_cyc >= 0 { "+" } else { "" };
        let line9 = format!(
            "Vs base: \u{394}Hit {sh}{d_hit:.1}%  \u{394}MPKI {sm}{d_mpki:.1}  \u{394}AMAT {sa}{d_amat:.2}c  \u{394}Cyc {sc}{d_cyc}"
        );
        f.render_widget(
            Paragraph::new(Span::styled(line9, Style::default().fg(Color::LightBlue))),
            Rect::new(inner.x, inner.y + 8, inner.width, 1),
        );
    }
}

fn render_chart(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title("Hit Rate History (%)");
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    let i_data: Vec<(f64, f64)> = app.run.mem.icache.stats.history.iter().cloned().collect();
    let d_data: Vec<(f64, f64)> = app.run.mem.dcache.stats.history.iter().cloned().collect();

    if i_data.is_empty() && d_data.is_empty() {
        let msg = Paragraph::new("No data yet — run the program to collect cache statistics.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(msg, inner);
        return;
    }

    let scope = app.cache.scope;

    let mut datasets = Vec::new();
    let show_i = matches!(scope, CacheScope::ICache | CacheScope::Both) && !i_data.is_empty();
    let show_d = matches!(scope, CacheScope::DCache | CacheScope::Both) && !d_data.is_empty();

    if show_i {
        datasets.push(
            Dataset::default()
                .name("I-Cache")
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Cyan))
                .data(&i_data),
        );
    }
    if show_d {
        datasets.push(
            Dataset::default()
                .name("D-Cache")
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Green))
                .data(&d_data),
        );
    }

    // Use a rolling X window instead of always starting at 0 — otherwise after ~MAX_HISTORY
    // samples the chart "squeezes" into the right edge as x_max grows.
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
                .style(Style::default().fg(Color::DarkGray))
                .bounds([x_min, x_max])
                .labels(vec![
                    Span::raw(format!("{x_min:.0}")),
                    Span::raw(format!("{x_mid:.0}")),
                    Span::raw(format!("{x_max:.0}")),
                ]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
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
    let hit_color = if hit_rate >= 90.0 { Color::Green } else if hit_rate >= 70.0 { Color::Yellow } else { Color::Red };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(label, Style::default().fg(Color::Cyan).bold()));
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
            format!("H:{hits}  M:{misses}  MR:{miss_rate:.1}%  MPKI:{mpki:.1}"),
            Style::default().fg(Color::Gray),
        )),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );
    if inner.height < 3 { return; }

    let fills = if cfg.is_valid_config() && cfg.line_size > 0 {
        stats.bytes_loaded / cfg.line_size as u64
    } else { 0 };
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("Acc:{total}  Evict:{}  WB:{}  Fills:{fills}", stats.evictions, stats.writebacks),
            Style::default().fg(Color::DarkGray),
        )),
        Rect::new(inner.x, inner.y + 2, inner.width, 1),
    );
    if inner.height < 4 { return; }

    f.render_widget(
        Paragraph::new(Span::styled(
            format!("RAM R:{}  RAM W:{}", fmt_bytes(stats.bytes_loaded), fmt_bytes(stats.ram_write_bytes)),
            Style::default().fg(Color::Cyan),
        )),
        Rect::new(inner.x, inner.y + 3, inner.width, 1),
    );
    if inner.height < 5 { return; }

    let cycles = stats.total_cycles;
    let avg = if total == 0 { 0.0_f64 } else { cycles as f64 / total as f64 };
    let cpi_contrib = if instructions == 0 { 0.0_f64 } else { cycles as f64 / instructions as f64 };
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("Cycles:{cycles}  Avg:{avg:.2}c/a  CPI:{cpi_contrib:.2}"),
            Style::default().fg(Color::Magenta),
        )),
        Rect::new(inner.x, inner.y + 4, inner.width, 1),
    );
    if inner.height < 6 { return; }

    let hit_cyc  = cfg.tag_search_cycles();
    let miss_cyc = hit_cyc + cfg.miss_penalty + cfg.line_transfer_cycles();
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("Cost model: Hit={hit_cyc}cyc  Miss={miss_cyc}cyc"),
            Style::default().fg(Color::DarkGray),
        )),
        Rect::new(inner.x, inner.y + 5, inner.width, 1),
    );
}

fn render_unified_chart(f: &mut Frame, area: Rect, app: &App, extra_idx: usize) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title("Hit Rate History (%)");
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 3 || inner.width < 10 { return; }

    let data: Vec<(f64, f64)> = app.run.mem.extra_levels[extra_idx]
        .stats.history.iter().cloned().collect();

    if data.is_empty() {
        f.render_widget(
            Paragraph::new("No data yet — run the program to collect cache statistics.")
                .style(Style::default().fg(Color::DarkGray))
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
            .style(Style::default().fg(Color::Cyan))
            .data(&data),
    ];

    let chart = Chart::new(datasets)
        .x_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([x_min, x_max])
                .labels(vec![
                    Span::raw(format!("{x_min:.0}")),
                    Span::raw(format!("{x_mid:.0}")),
                    Span::raw(format!("{x_max:.0}")),
                ]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
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
