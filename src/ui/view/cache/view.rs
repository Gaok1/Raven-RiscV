// ui/view/cache/view.rs — Cache matrix visualization
// Educational view: sets × ways matrix showing V/D bits, address, data bytes, policy metadata.
//
// Layout:
//   ┌── Area (from mod.rs, already excludes the shared controls bar) ──────────┐
//   │ ┌─ I-Cache / D-Cache matrix ──────────────────────────────────────────┐  │
//   │ │ Set | Way 0                           | Way 1                       │  │
//   │ │   0 | 1 -  0x00001000  DE AD BE EF r:0│ 0 -  (empty)               │  │
//   │ │   1 | 1 1  0x00002000  01 02 03 04 r:1│ 1 -  0x0000A000  AA BB r:0 │  │
//   │ └─────────────────────────────────────────────────────────────────────┘  │
//   │  V D=valid/dirty bits  [m:HEX] [g:1B]  r:N recency  ↑↓ ←→ N/M sets     │  ← legend bar
//   └────────────────────────────────────────────────────────────────────────────┘

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use unicode_truncate::UnicodeTruncateStr;

use crate::falcon::cache::{CacheConfig, CacheController, CacheLineView, CacheSetView, ReplacementPolicy};
use crate::ui::app::{App, CacheDataFmt, CacheDataGroup, CacheScope};

const DIRTY_COLOR: Color = Color::Rgb(180, 100, 255);
const DIRTY_ADDR_COLOR: Color = Color::Rgb(110, 70, 160);

pub(super) fn render_view(f: &mut Frame, area: Rect, app: &App) {
    if app.cache.selected_level == 0 {
        render_l1_view(f, area, app);
    } else {
        let idx = app.cache.selected_level - 1;
        if idx < app.run.mem.extra_levels.len() {
            render_unified_view(f, area, app, idx);
        }
    }
}

fn render_l1_view(f: &mut Frame, area: Rect, app: &App) {
    // Reset both scrollbar track slots; render_cache_matrix will fill them.
    app.cache.hscroll_tracks.set([(0, 0); 2]);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    match app.cache.scope {
        CacheScope::Both => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(layout[0]);
            render_cache_matrix(f, cols[0], app, true);
            render_cache_matrix(f, cols[1], app, false);
        }
        CacheScope::ICache => render_cache_matrix(f, layout[0], app, true),
        CacheScope::DCache => render_cache_matrix(f, layout[0], app, false),
    }

    render_legend_bar(f, layout[1], app);
}

fn render_unified_view(f: &mut Frame, area: Rect, app: &App, extra_idx: usize) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    render_extra_cache_matrix(f, layout[0], app, extra_idx);
    render_unified_legend_bar(f, layout[1], app, extra_idx);
}

// ── Legend bar (outside the matrix block, 1 line, no border) ─────────────────

fn render_unified_legend_bar(f: &mut Frame, area: Rect, app: &App, extra_idx: usize) {
    let cfg = &app.run.mem.extra_levels[extra_idx].config;
    let num_sets = if cfg.is_valid_config() { cfg.num_sets() } else { 0 };
    let scroll = app.cache.view_scroll.min(num_sets.saturating_sub(1));
    let scroll_hint = format!("↑↓ ←→  {}/{} sets", scroll + 1, num_sets);
    let policy_hint = policy_hint_str(cfg.replacement);

    let (fmt_style, group_style, fmt_label, group_label) =
        legend_button_styles(app);

    // " V D=valid/dirty bits  " is 23 chars
    let prefix_len: u16 = 23;
    let fmt_x0 = area.x + prefix_len;
    let fmt_x1 = fmt_x0 + fmt_label.len() as u16;
    let group_x0 = fmt_x1 + 1;
    let group_x1 = group_x0 + group_label.len() as u16;
    app.cache.view_fmt_btn.set((area.y, fmt_x0, fmt_x1));
    app.cache.view_group_btn.set((area.y, group_x0, group_x1));

    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("V D", Style::default().fg(Color::Yellow)),
        Span::styled("=valid/dirty bits  ", Style::default().fg(Color::DarkGray)),
        Span::styled(fmt_label, fmt_style),
        Span::raw(" "),
        Span::styled(group_label, group_style),
        Span::styled("  ", Style::default()),
        Span::styled(policy_hint, Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(scroll_hint, Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_legend_bar(f: &mut Frame, area: Rect, app: &App) {
    let scope = app.cache.scope;
    let icfg = &app.run.mem.icache.config;
    let dcfg = &app.run.mem.dcache.config;

    // Policy-specific hint — if both caches have the same policy, show once
    let policy_hint: String = match scope {
        CacheScope::ICache => policy_hint_str(icfg.replacement),
        CacheScope::DCache => policy_hint_str(dcfg.replacement),
        CacheScope::Both => {
            if icfg.replacement == dcfg.replacement {
                policy_hint_str(icfg.replacement)
            } else {
                format!(
                    "I:{} D:{}",
                    policy_hint_short(icfg.replacement),
                    policy_hint_short(dcfg.replacement)
                )
            }
        }
    };

    // Scroll indicator (use the cache with more sets as reference)
    let num_sets = match scope {
        CacheScope::ICache => {
            if icfg.is_valid_config() { icfg.num_sets() } else { 0 }
        }
        CacheScope::DCache => {
            if dcfg.is_valid_config() { dcfg.num_sets() } else { 0 }
        }
        CacheScope::Both => {
            let i = if icfg.is_valid_config() { icfg.num_sets() } else { 0 };
            let d = if dcfg.is_valid_config() { dcfg.num_sets() } else { 0 };
            i.max(d)
        }
    };
    let scroll = app.cache.view_scroll.min(num_sets.saturating_sub(1));
    let scroll_hint = format!("↑↓ ←→  {}/{} sets", scroll + 1, num_sets);

    let (fmt_style, group_style, fmt_label, group_label) =
        legend_button_styles(app);

    // " V D=valid/dirty bits  " is 23 chars
    let prefix_len: u16 = 23;
    let fmt_x0 = area.x + prefix_len;
    let fmt_x1 = fmt_x0 + fmt_label.len() as u16;
    let group_x0 = fmt_x1 + 1;
    let group_x1 = group_x0 + group_label.len() as u16;
    app.cache.view_fmt_btn.set((area.y, fmt_x0, fmt_x1));
    app.cache.view_group_btn.set((area.y, group_x0, group_x1));

    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("V D", Style::default().fg(Color::Yellow)),
        Span::styled("=valid/dirty bits  ", Style::default().fg(Color::DarkGray)),
        Span::styled(fmt_label, fmt_style),
        Span::raw(" "),
        Span::styled(group_label, group_style),
        Span::styled("  ", Style::default()),
        Span::styled(policy_hint, Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(scroll_hint, Style::default().fg(Color::DarkGray)),
    ]);

    f.render_widget(Paragraph::new(line), area);
}

/// Returns (fmt_style, group_style, fmt_label, group_label) for legend bar buttons.
fn legend_button_styles(app: &App) -> (Style, Style, String, String) {
    use crate::ui::app::CacheDataFmt;
    let fmt = app.cache.data_fmt;
    // Include the key hint in the label: "[m:HEX]"
    let fmt_label = format!("[m:{}]", fmt.label());
    let is_float = fmt == CacheDataFmt::Float;
    let group_label = if is_float {
        "[g:4B]".to_string()
    } else {
        format!("[g:{}]", app.cache.data_group.label())
    };

    let fmt_style = if app.cache.hover_view_fmt {
        Style::default().fg(Color::Black).bg(Color::Cyan)
    } else {
        Style::default().fg(Color::Cyan)
    };
    let group_style = if is_float {
        Style::default().fg(Color::DarkGray)
    } else if app.cache.hover_view_group {
        Style::default().fg(Color::Black).bg(Color::Green)
    } else {
        Style::default().fg(Color::Green)
    };

    (fmt_style, group_style, fmt_label, group_label)
}

/// Full policy hint (for single-scope display).
fn policy_hint_str(p: ReplacementPolicy) -> String {
    match p {
        ReplacementPolicy::Lru   => "r:N = recency rank  (cyan 0=just used / red N=evict next)".into(),
        ReplacementPolicy::Mru   => "r:N = recency rank  (red 0=just used=EVICT / cyan N=safe)".into(),
        ReplacementPolicy::Fifo  => "r:N = arrival order  (cyan 0=newest / red N=oldest=evict next)".into(),
        ReplacementPolicy::Lfu   => "f:N = access count  (red=fewest accesses=evict next)".into(),
        ReplacementPolicy::Clock => "> = clock pointer  R = recently used (protected)  > no R = evict next".into(),
        ReplacementPolicy::Random => "random eviction — no priority ordering".into(),
    }
}

/// Short policy hint used when showing both caches side by side with different policies.
fn policy_hint_short(p: ReplacementPolicy) -> &'static str {
    match p {
        ReplacementPolicy::Lru   => "LRU  r:N recency  cyan=safe  red=evict",
        ReplacementPolicy::Mru   => "MRU  r:N recency  red=just-used=EVICT",
        ReplacementPolicy::Fifo  => "FIFO  r:N order  cyan=newest  red=evict",
        ReplacementPolicy::Lfu   => "LFU  f:N=count  red=fewest=evict",
        ReplacementPolicy::Clock => "Clock  >=pointer  R=protected",
        ReplacementPolicy::Random => "Random",
    }
}

// ── Cache matrix ──────────────────────────────────────────────────────────────

fn render_extra_cache_matrix(f: &mut Frame, area: Rect, app: &App, extra_idx: usize) {
    let cache = &app.run.mem.extra_levels[extra_idx];
    let level_name = CacheController::extra_level_name(extra_idx);
    let cfg = &cache.config;

    let title = if cfg.is_valid_config() {
        let policy_str = match cfg.replacement {
            ReplacementPolicy::Lru    => "LRU",
            ReplacementPolicy::Mru    => "MRU",
            ReplacementPolicy::Fifo   => "FIFO",
            ReplacementPolicy::Random => "Rand",
            ReplacementPolicy::Lfu    => "LFU",
            ReplacementPolicy::Clock  => "Clock",
        };
        format!(
            "{level_name} Unified · {}B · {}S · {}W · {}B/L · {policy_str}",
            cfg.size, cfg.num_sets(), cfg.associativity, cfg.line_size
        )
    } else {
        format!("{level_name}: disabled")
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(title, Style::default().fg(Color::Cyan).bold()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if !cfg.is_valid_config() {
        if inner.height > 0 {
            f.render_widget(
                Paragraph::new("Cache disabled — configure it in the Config tab")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center),
                inner,
            );
        }
        return;
    }

    if inner.height < 2 || inner.width < 20 { return; }

    let sets_view = cache.view();
    let num_sets = sets_view.len();
    let ways = cfg.associativity;
    let policy = cfg.replacement;

    let set_col_w: usize = 5;
    let sep_w: usize = 1;
    let policy_w = match policy {
        ReplacementPolicy::Lfu    => 6,
        ReplacementPolicy::Clock  => 4,
        ReplacementPolicy::Random => 2,
        _                         => 4,
    };
    // Fixed overhead: ○  0x00001000  = 1+2+10 = 13, + "  " before bytes + "  " before policy = 4
    let cell_overhead = 17 + policy_w;
    let total_way_space = (inner.width as usize).saturating_sub(set_col_w + sep_w + ways.saturating_sub(1) * sep_w);
    let ideal_way_col_w = total_way_space / ways.max(1);
    let min_way_col_w = (cell_overhead + 2).max(28);
    let way_col_w = ideal_way_col_w.max(min_way_col_w);
    let fmt   = app.cache.data_fmt;
    let group = if fmt == CacheDataFmt::Float { CacheDataGroup::B4 } else { app.cache.data_group };

    // Expand way_col_w so that all line_size bytes can fit in a single row (h-scroll if needed)
    let (unit_chars, unit_bytes) = unit_metrics(fmt, group);
    let units_for_all = cfg.line_size / unit_bytes.max(1);
    let min_way_col_for_all = cell_overhead + units_for_all * unit_chars;
    let way_col_w = way_col_w.max(min_way_col_for_all);

    let bytes_per_row = if way_col_w > cell_overhead {
        bytes_from_budget(way_col_w - cell_overhead, fmt, group, cfg.line_size)
    } else { 0 };
    let row_height = if bytes_per_row == 0 || cfg.line_size == 0 {
        1
    } else {
        cfg.line_size.div_ceil(bytes_per_row)
    }.max(1);
    let total_content_w = set_col_w + sep_w + ways * way_col_w + ways.saturating_sub(1) * sep_w;
    let max_h_scroll = total_content_w.saturating_sub(inner.width as usize);
    let h_scroll = app.cache.view_h_scroll.min(max_h_scroll) as u16;
    let need_h_scrollbar = max_h_scroll > 0;
    let header_h: u16 = 1;
    let scrollbar_h: u16 = if need_h_scrollbar { 1 } else { 0 };
    let rows_h = inner.height.saturating_sub(header_h + scrollbar_h);
    let visible_sets = (rows_h as usize) / row_height;
    let max_scroll = num_sets.saturating_sub(visible_sets.max(1));
    let scroll = app.cache.view_scroll.min(max_scroll);

    // Header
    {
        let mut spans: Vec<Span> = vec![
            Span::styled(
                format!("{:^width$}", "Set", width = set_col_w),
                Style::default().fg(Color::Yellow).bold(),
            ),
            Span::styled("|", Style::default().fg(Color::DarkGray)),
        ];
        for w in 0..ways {
            spans.push(Span::styled(
                format!("{:^width$}", format!("Way {w}"), width = way_col_w),
                Style::default().fg(Color::Yellow).bold(),
            ));
            if w + 1 < ways {
                spans.push(Span::styled("|", Style::default().fg(Color::DarkGray)));
            }
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)).scroll((0, h_scroll)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
    }

    // Set rows (unified — D-cache lines can be dirty)
    let mut term_row: u16 = 0;
    'sets: for set_idx in scroll.. {
        if set_idx >= num_sets || term_row >= rows_h { break; }
        let set = &sets_view[set_idx];
        let has_valid = set.lines.iter().any(|l| l.valid);
        let set_style = if has_valid { Style::default().fg(Color::White) }
                        else         { Style::default().fg(Color::DarkGray) };
        for sub_row in 0..row_height {
            if term_row >= rows_h { break 'sets; }
            let y = inner.y + header_h + term_row;
            let set_col = if sub_row == 0 {
                Span::styled(format!("{:>width$} ", set_idx, width = set_col_w - 1), set_style)
            } else {
                Span::raw(" ".repeat(set_col_w))
            };
            let byte_offset = sub_row * bytes_per_row;
            let mut spans = vec![set_col, Span::styled("|", Style::default().fg(Color::DarkGray))];
            for w in 0..ways {
                let cell = build_cell(
                    &set.lines[w], set, w,
                    true, // unified = can be dirty
                    policy, cfg, set_idx,
                    bytes_per_row, byte_offset, sub_row == 0,
                    way_col_w, fmt, group,
                );
                spans.extend(cell);
                if w + 1 < ways {
                    spans.push(Span::styled("|", Style::default().fg(Color::DarkGray)));
                }
            }
            f.render_widget(
                Paragraph::new(Line::from(spans)).scroll((0, h_scroll)),
                Rect::new(inner.x, y, inner.width, 1),
            );
            term_row += 1;
        }
    }

    // Horizontal scrollbar
    if need_h_scrollbar {
        let sb_y = inner.y + inner.height - 1;
        let sb_area = Rect::new(inner.x, sb_y, inner.width, 1);
        let track_x = inner.x + 1;
        let track_w = inner.width.saturating_sub(2);
        // Unified/extra levels use slot 0 only
        app.cache.hscroll_tracks.set([(track_x, track_w), (0, 0)]);
        app.cache.hscroll_row.set(sb_y);
        app.cache.hscroll_max.set(max_h_scroll);
        let hovered = app.cache.hover_hscrollbar
            && app.cache.hscroll_hover_track_x == track_x;
        let style = if hovered {
            Style::default().fg(Color::White).bg(Color::Rgb(50, 50, 70))
        } else {
            Style::default()
        };
        let mut sb_state = ScrollbarState::new(max_h_scroll).position(h_scroll as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
                .begin_symbol(Some("◄"))
                .end_symbol(Some("►"))
                .style(style),
            sb_area,
            &mut sb_state,
        );
    }
}

fn render_cache_matrix(f: &mut Frame, area: Rect, app: &App, icache: bool) {
    let cache = if icache { &app.run.mem.icache } else { &app.run.mem.dcache };
    let label = if icache { "I-Cache" } else { "D-Cache" };
    let cfg = &cache.config;

    let title = if cfg.is_valid_config() {
        let policy_str = match cfg.replacement {
            ReplacementPolicy::Lru    => "LRU",
            ReplacementPolicy::Mru    => "MRU",
            ReplacementPolicy::Fifo   => "FIFO",
            ReplacementPolicy::Random => "Rand",
            ReplacementPolicy::Lfu    => "LFU",
            ReplacementPolicy::Clock  => "Clock",
        };
        format!(
            "{label} · {}B · {}S · {}W · {}B/L · {policy_str}",
            cfg.size,
            cfg.num_sets(),
            cfg.associativity,
            cfg.line_size
        )
    } else {
        format!("{label}: disabled")
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(title, Style::default().fg(Color::Cyan).bold()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if !cfg.is_valid_config() {
        if inner.height > 0 {
            f.render_widget(
                Paragraph::new("Cache disabled — configure it in the Config tab")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center),
                inner,
            );
        }
        return;
    }

    if inner.height < 2 || inner.width < 20 {
        return;
    }

    let sets_view = cache.view();
    let num_sets = sets_view.len();
    let ways = cfg.associativity;
    let policy = cfg.replacement;

    // Column widths
    let set_col_w: usize = 5; // " NNN "
    let sep_w: usize = 1;     // "|"

    // Fixed overhead: ○  0x00001000  = 1+2+10 = 13, + "  " before bytes + "  " before policy = 4
    let policy_w = match policy {
        ReplacementPolicy::Lfu    => 6, // "f:9999"
        ReplacementPolicy::Clock  => 4, // ">R" or "> " etc.
        ReplacementPolicy::Random => 2,
        _                         => 4, // "r:NN"
    };
    let cell_overhead = 17 + policy_w;

    // way_col_w: prefer fitting the screen, but guarantee a useful minimum so
    // that content is always readable and horizontal scrolling becomes possible.
    let total_way_space = (inner.width as usize)
        .saturating_sub(set_col_w + sep_w + ways.saturating_sub(1) * sep_w);
    let ideal_way_col_w = total_way_space / ways.max(1);
    let min_way_col_w = (cell_overhead + 2).max(28); // at least readable
    let way_col_w = ideal_way_col_w.max(min_way_col_w);

    let fmt = app.cache.data_fmt;
    let group = if fmt == CacheDataFmt::Float { CacheDataGroup::B4 } else { app.cache.data_group };

    // Expand way_col_w so that all line_size bytes can fit in a single row (h-scroll if needed)
    let (unit_chars, unit_bytes) = unit_metrics(fmt, group);
    let units_for_all = cfg.line_size / unit_bytes.max(1);
    let min_way_col_for_all = cell_overhead + units_for_all * unit_chars;
    let way_col_w = way_col_w.max(min_way_col_for_all);

    let bytes_per_row = if way_col_w > cell_overhead {
        bytes_from_budget(way_col_w - cell_overhead, fmt, group, cfg.line_size)
    } else {
        0
    };
    let row_height = if bytes_per_row == 0 || cfg.line_size == 0 {
        1
    } else {
        cfg.line_size.div_ceil(bytes_per_row)
    }.max(1);

    // Total logical content width (may exceed inner.width for large associativity)
    let total_content_w =
        set_col_w + sep_w + ways * way_col_w + ways.saturating_sub(1) * sep_w;

    // Horizontal scroll (clamp to valid range)
    let max_h_scroll = total_content_w.saturating_sub(inner.width as usize);
    let h_scroll = app.cache.view_h_scroll.min(max_h_scroll) as u16;
    let need_h_scrollbar = max_h_scroll > 0;

    // Heights: header(1) + optional scrollbar(1) + set rows (rest)
    let header_h: u16 = 1;
    let scrollbar_h: u16 = if need_h_scrollbar { 1 } else { 0 };
    let rows_h = inner.height.saturating_sub(header_h + scrollbar_h);

    // Clamp vertical scroll
    let visible_sets = (rows_h as usize) / row_height;
    let max_scroll = num_sets.saturating_sub(visible_sets.max(1));
    let scroll = app.cache.view_scroll.min(max_scroll);

    // ── Header row ───────────────────────────────────────────────────────────
    {
        let mut spans: Vec<Span> = vec![
            Span::styled(
                format!("{:^width$}", "Set", width = set_col_w),
                Style::default().fg(Color::Yellow).bold(),
            ),
            Span::styled("|", Style::default().fg(Color::DarkGray)),
        ];
        for w in 0..ways {
            spans.push(Span::styled(
                format!("{:^width$}", format!("Way {w}"), width = way_col_w),
                Style::default().fg(Color::Yellow).bold(),
            ));
            if w + 1 < ways {
                spans.push(Span::styled("|", Style::default().fg(Color::DarkGray)));
            }
        }
        f.render_widget(
            Paragraph::new(Line::from(spans)).scroll((0, h_scroll)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
    }

    // ── Set rows ─────────────────────────────────────────────────────────────
    let mut term_row: u16 = 0;
    'sets: for set_idx in scroll.. {
        if set_idx >= num_sets || term_row >= rows_h { break; }
        let set = &sets_view[set_idx];
        let has_valid = set.lines.iter().any(|l| l.valid);
        let set_style = if has_valid {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        for sub_row in 0..row_height {
            if term_row >= rows_h { break 'sets; }
            let y = inner.y + header_h + term_row;
            let set_col = if sub_row == 0 {
                Span::styled(format!("{:>width$} ", set_idx, width = set_col_w - 1), set_style)
            } else {
                Span::raw(" ".repeat(set_col_w))
            };
            let byte_offset = sub_row * bytes_per_row;
            let mut spans = vec![set_col, Span::styled("|", Style::default().fg(Color::DarkGray))];
            for w in 0..ways {
                let cell = build_cell(
                    &set.lines[w], set, w,
                    !icache, policy, cfg, set_idx,
                    bytes_per_row, byte_offset, sub_row == 0,
                    way_col_w, fmt, group,
                );
                spans.extend(cell);
                if w + 1 < ways {
                    spans.push(Span::styled("|", Style::default().fg(Color::DarkGray)));
                }
            }
            f.render_widget(
                Paragraph::new(Line::from(spans)).scroll((0, h_scroll)),
                Rect::new(inner.x, y, inner.width, 1),
            );
            term_row += 1;
        }
    }

    // ── Horizontal scrollbar ─────────────────────────────────────────────────
    if need_h_scrollbar {
        let sb_y = inner.y + inner.height - 1;
        let sb_area = Rect::new(inner.x, sb_y, inner.width, 1);
        let track_x = inner.x + 1;
        let track_w = inner.width.saturating_sub(2);
        // slot 0 = I-cache, slot 1 = D-cache
        let slot = if icache { 0 } else { 1 };
        let mut tracks = app.cache.hscroll_tracks.get();
        tracks[slot] = (track_x, track_w);
        app.cache.hscroll_tracks.set(tracks);
        app.cache.hscroll_row.set(sb_y);
        app.cache.hscroll_max.set(max_h_scroll);
        // Highlight if this specific scrollbar is hovered
        let hovered = app.cache.hover_hscrollbar
            && app.cache.hscroll_hover_track_x == track_x;
        let style = if hovered {
            Style::default().fg(Color::White).bg(Color::Rgb(50, 50, 70))
        } else {
            Style::default()
        };
        let mut sb_state = ScrollbarState::new(max_h_scroll)
            .position(h_scroll as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
                .begin_symbol(Some("◄"))
                .end_symbol(Some("►"))
                .style(style),
            sb_area,
            &mut sb_state,
        );
    }
}

// ── Cell builder helpers ───────────────────────────────────────────────────────

/// Chars per display unit and bytes consumed, given format + grouping.
fn unit_metrics(fmt: CacheDataFmt, group: CacheDataGroup) -> (usize, usize) {
    let g = if fmt == CacheDataFmt::Float { 4 } else { group.bytes() };
    let chars = match (fmt, g) {
        (CacheDataFmt::Hex,  1) => 3,  // "XX "
        (CacheDataFmt::Hex,  2) => 5,  // "XXXX "
        (CacheDataFmt::Hex,  4) => 9,  // "XXXXXXXX "
        (CacheDataFmt::DecU, 1) => 4,  // "NNN " (0–255)
        (CacheDataFmt::DecU, 2) => 6,  // "NNNNN " (0–65535)
        (CacheDataFmt::DecU, 4) => 11, // "NNNNNNNNNN " (0–4294967295)
        (CacheDataFmt::DecS, 1) => 5,  // "-NNN " (−128–127)
        (CacheDataFmt::DecS, 2) => 7,  // "-NNNNN " (−32768–32767)
        (CacheDataFmt::DecS, 4) => 12, // "-NNNNNNNNNN "
        (CacheDataFmt::Float, _) => 10, // "±NNN.NNN "
        _ => 3,
    };
    (chars, g)
}

/// How many bytes fit in `budget` chars for the given format + grouping.
fn bytes_from_budget(budget: usize, fmt: CacheDataFmt, group: CacheDataGroup, line_size: usize) -> usize {
    let (chars, g) = unit_metrics(fmt, group);
    let units = budget / chars;
    (units * g).min(line_size)
}

fn read_le(data: &[u8], offset: usize, size: usize) -> u64 {
    let mut val = 0u64;
    for i in 0..size {
        val |= (data.get(offset + i).copied().unwrap_or(0) as u64) << (i * 8);
    }
    val
}

fn render_data(data: &[u8], max_bytes: usize, fmt: CacheDataFmt, group: CacheDataGroup, tint: Option<Color>) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let dim = Style::default().fg(Color::DarkGray);
    let g = if fmt == CacheDataFmt::Float { 4 } else { group.bytes() };
    let n_bytes = max_bytes.min(data.len());
    let n_units = n_bytes / g;

    for i in 0..n_units {
        let offset = i * g;
        let (s, is_zero, neg) = match fmt {
            CacheDataFmt::Hex => {
                let v = read_le(data, offset, g);
                let s = match g {
                    1 => format!("{:02X}", v as u8),
                    2 => format!("{:04X}", v as u16),
                    _ => format!("{:08X}", v as u32),
                };
                (s, v == 0, false)
            }
            CacheDataFmt::DecU => {
                let v = read_le(data, offset, g);
                let s = match g {
                    1 => format!("{:3}",  v as u8),
                    2 => format!("{:5}",  v as u16),
                    _ => format!("{:10}", v as u32),
                };
                (s, v == 0, false)
            }
            CacheDataFmt::DecS => {
                let v = read_le(data, offset, g);
                let (s, neg) = match g {
                    1 => { let x = v as i8;  (format!("{x:4}"),  x < 0) }
                    2 => { let x = v as i16; (format!("{x:6}"),  x < 0) }
                    _ => { let x = v as i32; (format!("{x:11}"), x < 0) }
                };
                (s, v == 0, neg)
            }
            CacheDataFmt::Float => {
                let bytes = [
                    data.get(offset    ).copied().unwrap_or(0),
                    data.get(offset + 1).copied().unwrap_or(0),
                    data.get(offset + 2).copied().unwrap_or(0),
                    data.get(offset + 3).copied().unwrap_or(0),
                ];
                let f = f32::from_le_bytes(bytes);
                let s = if f.is_nan()          { "     NaN".to_string() }
                        else if f.is_infinite() { if f > 0.0 { "    +Inf".to_string() } else { "    -Inf".to_string() } }
                        else                    { format!("{f:8.3}") };
                (s, f == 0.0, f < 0.0)
            }
        };

        let style = tint.map_or_else(
            || if is_zero { dim }
               else if neg { Style::default().fg(Color::LightRed) }
               else { Style::default().fg(Color::White) },
            |c| Style::default().fg(c),
        );
        spans.push(Span::styled(s, style));
        if i + 1 < n_units { spans.push(Span::raw(" ")); }
    }
    spans
}

// ── Cell builder ──────────────────────────────────────────────────────────────

fn build_cell(
    line: &CacheLineView,
    set: &CacheSetView,
    way: usize,
    is_dcache: bool,
    policy: ReplacementPolicy,
    cfg: &CacheConfig,
    set_idx: usize,
    bytes_per_row: usize,
    byte_offset: usize,
    is_first_row: bool,
    cell_width: usize,
    fmt: CacheDataFmt,
    group: CacheDataGroup,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    // ── Invalid line ─────────────────────────────────────────────────────────
    if !line.valid {
        if is_first_row {
            spans.push(Span::styled("0", Style::default().fg(Color::DarkGray)));
            spans.push(Span::raw(" "));
            spans.push(Span::styled("-", Style::default().fg(Color::DarkGray)));
            let rest = cell_width.saturating_sub(3);
            spans.push(Span::styled(
                format!("{:<width$}", "  (empty)", width = rest),
                Style::default().fg(Color::DarkGray),
            ));
        } else {
            spans.push(Span::raw(" ".repeat(cell_width)));
        }
        return spans;
    }

    // ── Valid line, continuation row ─────────────────────────────────────────
    if !is_first_row {
        let is_dirty = is_dcache && line.dirty;
        // 17 chars = V(1)+sp(1)+D(1)+sp(2)+addr(10)+sp(2) — align data under first row
        spans.push(Span::raw(" ".repeat(17)));
        if byte_offset < line.data.len() && bytes_per_row > 0 {
            let tint = if is_dirty { Some(DIRTY_COLOR) } else { None };
            spans.extend(render_data(&line.data[byte_offset..], bytes_per_row, fmt, group, tint));
        }
        // enforce cell_width (truncate/pad handled by the common block below)
        let used: usize = spans.iter().map(Span::width).sum();
        if used < cell_width {
            spans.push(Span::raw(" ".repeat(cell_width - used)));
        } else if used > cell_width {
            let mut out: Vec<Span<'static>> = Vec::with_capacity(spans.len());
            let mut budget = cell_width;
            for span in spans {
                if budget == 0 { break; }
                let width = span.width();
                if width <= budget {
                    budget -= width;
                    out.push(span);
                } else {
                    let (s, actual_width) = span.content.as_ref().unicode_truncate(budget);
                    if actual_width > 0 {
                        out.push(Span::styled(s.to_string(), span.style));
                    }
                    budget -= actual_width;
                    break;
                }
            }
            if budget > 0 { out.push(Span::raw(" ".repeat(budget))); }
            spans = out;
        }
        return spans;
    }

    let is_dirty = is_dcache && line.dirty;
    let base = (line.tag << (cfg.offset_bits() + cfg.index_bits()))
        | ((set_idx as u32) << cfg.offset_bits());

    // V bit
    spans.push(Span::styled("1", Style::default().fg(Color::Green).bold()));
    spans.push(Span::raw(" "));

    // D bit
    if is_dcache {
        if is_dirty {
            spans.push(Span::styled("1", Style::default().fg(DIRTY_COLOR).bold()));
        } else {
            spans.push(Span::styled("0", Style::default().fg(Color::DarkGray)));
        }
    } else {
        spans.push(Span::styled("-", Style::default().fg(Color::DarkGray)));
    }
    spans.push(Span::raw("  "));

    // Address (derived from tag + set index)
    // Cyan "MRU" highlight only for policies where recency == safety (LRU, MRU).
    // LFU evicts by frequency, not recency — cyan highlight would be misleading there.
    let is_mru = matches!(policy, ReplacementPolicy::Lru | ReplacementPolicy::Mru)
        && set.lru_order.first() == Some(&way);
    let addr_style = if is_dirty {
        Style::default().fg(DIRTY_COLOR)
    } else if is_mru {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::White)
    };
    spans.push(Span::styled(format!("0x{base:08X}"), addr_style));

    // Data (first row: bytes [byte_offset .. byte_offset + bytes_per_row])
    if bytes_per_row > 0 && byte_offset < line.data.len() {
        spans.push(Span::raw("  "));
        let tint = if is_dirty { Some(DIRTY_COLOR) } else { None };
        spans.extend(render_data(&line.data[byte_offset..], bytes_per_row, fmt, group, tint));
    }

    // Policy metadata
    spans.push(Span::raw("  "));
    match policy {
        ReplacementPolicy::Lru => {
            let rank = set.lru_order.iter().position(|&w| w == way).unwrap_or(0);
            let n = set.lru_order.len();
            let style = if rank == 0 {
                Style::default().fg(Color::Cyan)       // MRU = safest
            } else if rank + 1 == n {
                Style::default().fg(Color::Red).bold() // LRU = evicted next
            } else {
                Style::default().fg(Color::Gray)
            };
            spans.push(Span::styled(format!("r:{rank}"), style));
        }
        ReplacementPolicy::Mru => {
            let rank = set.lru_order.iter().position(|&w| w == way).unwrap_or(0);
            let n = set.lru_order.len();
            // MRU evicts the most recently used → rank 0 is the DANGER zone
            let style = if rank == 0 {
                Style::default().fg(Color::Red).bold() // MRU = evicted next!
            } else if rank + 1 == n {
                Style::default().fg(Color::Cyan)       // LRU = safest for MRU
            } else {
                Style::default().fg(Color::Gray)
            };
            spans.push(Span::styled(format!("r:{rank}"), style));
        }
        ReplacementPolicy::Fifo => {
            let pos = set.fifo_order.iter().position(|&w| w == way).unwrap_or(0);
            let n = set.fifo_order.len();
            let style = if pos == 0 {
                Style::default().fg(Color::Cyan)       // newest
            } else if pos + 1 == n {
                Style::default().fg(Color::Red).bold() // oldest = evicted next
            } else {
                Style::default().fg(Color::Gray)
            };
            spans.push(Span::styled(format!("r:{pos}"), style));
        }
        ReplacementPolicy::Lfu => {
            let freq = line.freq;
            let min_freq = set.lines.iter().filter(|l| l.valid).map(|l| l.freq).min().unwrap_or(0);
            let style = if freq == min_freq {
                Style::default().fg(Color::Red).bold()
            } else {
                Style::default().fg(Color::Magenta)
            };
            let freq_str = if freq >= 1_000_000 {
                format!("f:{}M", freq / 1_000_000)
            } else if freq >= 1_000 {
                format!("f:{}K", freq / 1_000)
            } else {
                format!("f:{freq}")
            };
            spans.push(Span::styled(freq_str, style));
        }
        ReplacementPolicy::Clock => {
            let n = set.lines.len().max(1);
            let is_hand = (set.clock_hand % n) == way;
            let (icon, style) = match (is_hand, line.ref_bit) {
                (true,  true)  => (">R", Style::default().fg(Color::Yellow).bold()),
                (true,  false) => ("> ", Style::default().fg(Color::Red).bold()),
                (false, true)  => (" R", Style::default().fg(Color::Yellow)),
                (false, false) => ("  ", Style::default().fg(Color::DarkGray)),
            };
            spans.push(Span::styled(icon, style));
        }
        ReplacementPolicy::Random => {
            spans.push(Span::styled("??", Style::default().fg(Color::DarkGray)));
        }
    }

    // Enforce exact cell_width: truncate if over, pad if under
    let used: usize = spans.iter().map(Span::width).sum();
    if used < cell_width {
        spans.push(Span::raw(" ".repeat(cell_width - used)));
    } else if used > cell_width {
        let mut out: Vec<Span<'static>> = Vec::with_capacity(spans.len());
        let mut budget = cell_width;
        for span in spans {
            if budget == 0 { break; }
            let width = span.width();
            if width <= budget {
                budget -= width;
                out.push(span);
            } else {
                let (s, actual_width) = span.content.as_ref().unicode_truncate(budget);
                if actual_width > 0 {
                    out.push(Span::styled(s.to_string(), span.style));
                }
                budget -= actual_width;
                break;
            }
        }
        if budget > 0 {
            out.push(Span::raw(" ".repeat(budget)));
        }
        spans = out;
    }

    spans
}
