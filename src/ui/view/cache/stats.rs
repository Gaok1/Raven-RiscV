// ui/view/cache/stats.rs — Cache statistics subtab
use ratatui::{
    Frame,
    prelude::*,
    widgets::{
        Axis, Block, BorderType, Borders, Chart, Dataset, Gauge, GraphType, List, ListItem,
        Paragraph,
    },
};

use crate::ui::app::{App, CacheScope};

// Note: Reset/Pause/Scope controls are in the shared controls bar (mod.rs).

pub(super) fn render_stats(f: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // metric gauges (4 lines per cache + border)
            Constraint::Min(8),    // chart
            Constraint::Length(8), // miss-by-PC table
        ])
        .split(area);

    render_metrics(f, layout[0], app);
    render_chart(f, layout[1], app);
    render_miss_table(f, layout[2], app);
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

fn render_miss_table(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title("Top Miss PCs (I-Cache)");
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let mut miss_vec: Vec<(u32, u64)> =
        app.run.mem.icache.stats.miss_pcs.iter().map(|(&k, &v)| (k, v)).collect();
    miss_vec.sort_by(|a, b| b.1.cmp(&a.1));

    let visible = inner.height as usize;
    let start = app.cache.stats_scroll.min(miss_vec.len().saturating_sub(1));

    let header = ListItem::new(Line::from(vec![
        Span::styled(format!("{:<12}", "PC"), Style::default().fg(Color::Yellow).bold()),
        Span::styled(format!("{:>8}", "Misses"), Style::default().fg(Color::Yellow).bold()),
    ]));

    let mut items = vec![header];
    for (pc, count) in miss_vec.iter().skip(start).take(visible.saturating_sub(1)) {
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("0x{pc:08x}  "),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!("{count:>8}"), Style::default().fg(Color::Red)),
        ])));
    }

    if miss_vec.is_empty() {
        items.push(ListItem::new(
            Span::styled("  No misses recorded", Style::default().fg(Color::DarkGray)),
        ));
    }

    f.render_widget(List::new(items), inner);
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
