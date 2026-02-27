// ui/view/cache/view.rs — Cache matrix visualization
// Educational view: sets × ways matrix showing tag, dirty bit, data bytes, policy metadata.
//
// Layout:
//   ┌── Area (from mod.rs, already excludes the shared controls bar) ──────────┐
//   │ ┌─ I-Cache / D-Cache matrix ──────────────────────────────────────────┐  │
//   │ │ Set | Way 0                        | Way 1                          │  │
//   │ │   0 | V -  T:00001  DE AD BE EF r:0│ . -  (empty)                  │  │
//   │ │   1 | V D  T:00002  01 02 03 04 r:1│ V -  T:0000A  AA BB CC DD r:0 │  │
//   │ └─────────────────────────────────────────────────────────────────────┘  │
//   │  V=valid  .=inv  D=dirty  r:0=MRU  r:last=evict   ↑↓  1/32 sets         │  ← legend bar
//   └────────────────────────────────────────────────────────────────────────────┘

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};

use crate::falcon::cache::{CacheLineView, CacheSetView, ReplacementPolicy};
use crate::ui::app::{App, CacheScope};

pub(super) fn render_view(f: &mut Frame, area: Rect, app: &App) {
    // Split: matrix area(s) + 1-line legend bar
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

// ── Legend bar (outside the matrix block, 1 line, no border) ─────────────────

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
        CacheScope::DCache => {
            if dcfg.is_valid_config() { dcfg.num_sets() } else { 0 }
        }
        _ => {
            if icfg.is_valid_config() { icfg.num_sets() } else { 0 }
        }
    };
    let scroll = app.cache.view_scroll.min(num_sets.saturating_sub(1));
    let scroll_hint = format!("↑↓ ←→  {}/{} sets", scroll + 1, num_sets);

    // Build styled line so key symbols match the colors used in the matrix
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("V", Style::default().fg(Color::Green).bold()),
        Span::styled("=valid", Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(".", Style::default().fg(Color::DarkGray)),
        Span::styled("=inv", Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled("D", Style::default().fg(Color::Yellow).bold()),
        Span::styled("=dirty", Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(policy_hint, Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(scroll_hint, Style::default().fg(Color::DarkGray)),
    ]);

    f.render_widget(Paragraph::new(line), area);
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

    // Tag hex width: ceil((32 - offset_bits - index_bits) / 4)
    let tag_bits = 32u32.saturating_sub(cfg.offset_bits() + cfg.index_bits());
    let tag_hex_w = ((tag_bits + 3) / 4) as usize;

    // Column widths
    let set_col_w: usize = 5; // " NNN "
    let sep_w: usize = 1;     // "|"

    // How many data bytes fit in each way cell
    // Fixed overhead: "V D  T:XXXXXX  " + policy width
    let policy_w = match policy {
        ReplacementPolicy::Lfu    => 6, // "f:9999"
        ReplacementPolicy::Clock  => 4, // ">R" or "> " etc.
        ReplacementPolicy::Random => 2,
        _                         => 4, // "r:NN"
    };
    let cell_overhead = tag_hex_w + 8 + policy_w + 2;

    // way_col_w: prefer fitting the screen, but guarantee a useful minimum so
    // that content is always readable and horizontal scrolling becomes possible.
    let total_way_space = (inner.width as usize)
        .saturating_sub(set_col_w + sep_w + ways.saturating_sub(1) * sep_w);
    let ideal_way_col_w = total_way_space / ways.max(1);
    let min_way_col_w = (cell_overhead + 2).max(28); // at least readable
    let way_col_w = ideal_way_col_w.max(min_way_col_w);

    let bytes_to_show = if way_col_w > cell_overhead {
        ((way_col_w - cell_overhead) / 3).min(cfg.line_size).min(8)
    } else {
        0
    };

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
    let max_scroll = num_sets.saturating_sub(rows_h as usize);
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
    for row_idx in 0..rows_h {
        let set_idx = scroll + row_idx as usize;
        if set_idx >= num_sets {
            break;
        }
        let set = &sets_view[set_idx];
        let y = inner.y + header_h + row_idx;

        let has_valid = set.lines.iter().any(|l| l.valid);
        let set_label = format!("{:>width$} ", set_idx, width = set_col_w - 1);
        let set_style = if has_valid {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let mut spans: Vec<Span> = vec![
            Span::styled(set_label, set_style),
            Span::styled("|", Style::default().fg(Color::DarkGray)),
        ];

        for w in 0..ways {
            let cell = build_cell(
                &set.lines[w],
                set,
                w,
                icache,
                policy,
                tag_hex_w,
                bytes_to_show,
                way_col_w,
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
    }

    // ── Horizontal scrollbar ─────────────────────────────────────────────────
    if need_h_scrollbar {
        let sb_y = inner.y + inner.height - 1;
        let sb_area = Rect::new(inner.x, sb_y, inner.width, 1);
        let mut sb_state = ScrollbarState::new(max_h_scroll)
            .position(h_scroll as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
                .begin_symbol(Some("◄"))
                .end_symbol(Some("►")),
            sb_area,
            &mut sb_state,
        );
    }
}

// ── Cell builder ──────────────────────────────────────────────────────────────

fn build_cell(
    line: &CacheLineView,
    set: &CacheSetView,
    way: usize,
    is_dcache: bool,
    policy: ReplacementPolicy,
    tag_hex_w: usize,
    bytes_to_show: usize,
    cell_width: usize,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    if !line.valid {
        let s = format!("{:<width$}", ". -  (empty)", width = cell_width);
        spans.push(Span::styled(s, Style::default().fg(Color::DarkGray)));
        return spans;
    }

    // Valid
    spans.push(Span::styled("V", Style::default().fg(Color::Green).bold()));
    spans.push(Span::raw(" "));

    // Dirty (I-cache lines are never dirty — show dim dash)
    if is_dcache && line.dirty {
        spans.push(Span::styled("D", Style::default().fg(Color::Yellow).bold()));
    } else {
        spans.push(Span::styled("-", Style::default().fg(Color::DarkGray)));
    }
    spans.push(Span::raw("  "));

    // Tag — highlight MRU line for policies that track recency
    let is_mru = matches!(
        policy,
        ReplacementPolicy::Lru | ReplacementPolicy::Mru | ReplacementPolicy::Lfu
    ) && set.lru_order.first() == Some(&way);

    let tag_str = format!("T:{:0>width$X}", line.tag, width = tag_hex_w);
    let tag_style = if is_mru {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::White)
    };
    spans.push(Span::styled(tag_str, tag_style));

    // Data bytes
    if bytes_to_show > 0 {
        spans.push(Span::raw("  "));
        for i in 0..bytes_to_show.min(line.data.len()) {
            let b = line.data[i];
            let byte_style = if b == 0 {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };
            spans.push(Span::styled(format!("{b:02X}"), byte_style));
            if i + 1 < bytes_to_show {
                spans.push(Span::raw(" "));
            }
        }
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

    // Pad to fill cell_width
    let used: usize = spans.iter().map(|s| s.content.len()).sum();
    if used < cell_width {
        spans.push(Span::raw(" ".repeat(cell_width - used)));
    }

    spans
}
