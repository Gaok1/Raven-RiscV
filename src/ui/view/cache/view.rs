// ui/view/cache/view.rs — Cache matrix visualization
// Educational view: sets × ways matrix showing V/D bits, address, data bytes, policy metadata.
//
// Layout:
//   ┌── Area (from mod.rs, already excludes the shared controls bar) ──────────┐
//   │ ┌─ I-Cache / D-Cache matrix ──────────────────────────────────────────┐  │
//   │ │ Set │ Way 0                           │ Way 1                       │  │
//   │ │   0 │ 1 -  0x00001000  DE AD BE EF r:0│ 0 -  (empty)               │  │
//   │ │   1 │ 1 1  0x00002000  01 02 03 04 r:1│ 1 -  0x0000A000  AA BB r:0 │  │
//   │ └─────────────────────────────────────────────────────────────────────┘  │
//   │  V D valid / dirty   cells hex  group 1b  addr base   r:N recency …     │  ← legend bar
//   └────────────────────────────────────────────────────────────────────────────┘

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use unicode_truncate::UnicodeTruncateStr;

use crate::ui::app::{
    App, CacheAddrMode, CacheDataFmt, CacheDataGroup, CacheHoverTarget, CacheScope,
};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{ControlState, SbGeom, SpanRow, Toolbar};
use crate::ui::view::style;
use raven_engine::capability::{
    CacheHierarchy, CacheLevelConfig as CacheConfig, CacheLineView,
    CacheReplacementPolicy as ReplacementPolicy, CacheRole, CacheSetView,
};

const DIRTY_COLOR: Color = theme::DIRTY;
fn addr_mode_hint(addr_mode: CacheAddrMode, cfgs: &[&CacheConfig]) -> String {
    if !matches!(addr_mode, CacheAddrMode::Breakdown) {
        return String::new();
    }
    let valid: Vec<&CacheConfig> = cfgs
        .iter()
        .copied()
        .filter(|cfg| cfg.is_enabled())
        .collect();
    if valid.is_empty() {
        return String::new();
    }

    let first = valid[0];
    let off = first.offset_bits();
    let idx = first.index_bits();
    let tag = first.address_bits.saturating_sub(off + idx);
    let same = valid
        .iter()
        .all(|cfg| cfg.offset_bits() == off && cfg.index_bits() == idx);

    if same {
        format!(" off:{off}b idx:{idx}b tag:{tag}b")
    } else {
        " off|idx|tag".to_string()
    }
}

fn addr_text_width(addr_mode: CacheAddrMode, cfg: &CacheConfig) -> usize {
    match addr_mode {
        CacheAddrMode::Base => 2 + cfg.address_bits.div_ceil(4),
        CacheAddrMode::Breakdown => {
            let off_hex = (cfg.offset_bits().saturating_add(3) / 4).max(1);
            let idx_digits = cfg.num_sets().saturating_sub(1).to_string().len().max(1);
            let tag_hex = cfg
                .address_bits
                .saturating_sub(cfg.offset_bits() + cfg.index_bits())
                .div_ceil(4)
                .max(1);
            2 + off_hex + 3 + idx_digits + 3 + tag_hex
        }
    }
}

pub(super) fn render_view(f: &mut Frame, area: Rect, app: &App) {
    app.cache.view_num_sets.set(0);
    app.cache.view_num_sets_d.set(0);
    app.cache.view_visible_sets.set(0);
    app.cache.view_visible_sets_d.set(0);
    app.cache.view_scroll_max.set(0);
    app.cache.view_scroll_max_d.set(0);

    let Some(caches) = app.cache_hierarchy() else {
        return;
    };
    if app.cache.selected_level == 0 {
        render_l1_view(f, area, app, caches);
    } else {
        let level = app.cache.selected_level;
        if caches.cache(level, CacheRole::Unified).is_some() {
            render_unified_view(f, area, app, caches, level);
        }
    }
}

fn render_l1_view(f: &mut Frame, area: Rect, app: &App, caches: &dyn CacheHierarchy) {
    // Reset both scrollbar slots; render_cache_matrix will fill them.
    app.cache.hscroll_bars.set([None; 2]);

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
            render_cache_matrix(f, cols[0], app, caches, CacheRole::Instruction);
            render_cache_matrix(f, cols[1], app, caches, CacheRole::Data);
        }
        CacheScope::ICache => {
            render_cache_matrix(f, layout[0], app, caches, CacheRole::Instruction)
        }
        CacheScope::DCache => render_cache_matrix(f, layout[0], app, caches, CacheRole::Data),
    }

    render_legend_bar(f, layout[1], app, caches);
}

fn render_unified_view(
    f: &mut Frame,
    area: Rect,
    app: &App,
    caches: &dyn CacheHierarchy,
    level: usize,
) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    render_extra_cache_matrix(f, layout[0], app, caches, level);
    render_unified_legend_bar(f, layout[1], app, caches, level);
}

// ── Legend bar (outside the matrix block, 1 line, no border) ─────────────────

fn render_unified_legend_bar(
    f: &mut Frame,
    area: Rect,
    app: &App,
    caches: &dyn CacheHierarchy,
    level: usize,
) {
    let cfg = &caches.cache(level, CacheRole::Unified).unwrap().config;
    render_legend(
        f,
        area,
        app,
        addr_mode_hint(app.cache.addr_mode, &[cfg]),
        policy_hint_str(cfg.replacement),
    );
}

fn render_legend_bar(f: &mut Frame, area: Rect, app: &App, caches: &dyn CacheHierarchy) {
    let scope = app.cache.scope;
    let icache = caches.cache(0, CacheRole::Instruction).unwrap();
    let dcache = caches.cache(0, CacheRole::Data).unwrap();
    let icfg = &icache.config;
    let dcfg = &dcache.config;

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

    let addr_hint = match scope {
        CacheScope::ICache => addr_mode_hint(app.cache.addr_mode, &[icfg]),
        CacheScope::DCache => addr_mode_hint(app.cache.addr_mode, &[dcfg]),
        CacheScope::Both => addr_mode_hint(app.cache.addr_mode, &[icfg, dcfg]),
    };
    render_legend(f, area, app, addr_hint, policy_hint);
}

/// The three display toggles on the matrix legend.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewBtn {
    Fmt,
    Group,
    Addr,
}

/// The legend's toggle group, laid out once for both the spans and the columns
/// the mouse reads back.
///
/// They used to be spelled `[m:HEX] [g:1B] [t:BASE]` — the bracket naming the
/// *key* that cycles each one, which is the collision the design pass exists to
/// remove: a bracket means a keyboard key and lives in the footer, a control
/// wears a surface. And they were placed by a hand-counted `prefix_len = 23`
/// with `" V D=valid/dirty bits  " is 23 chars` written above it, a second copy
/// of the layout that any edit to the prefix would have silently invalidated.
pub(crate) fn build_view_legend_bar(app: &App) -> Toolbar<ViewBtn> {
    use crate::ui::app::CacheDataFmt;
    let hov = |t: CacheHoverTarget| app.cache.hover == Some(t);
    let is_float = app.cache.data_fmt == CacheDataFmt::Float;

    let mut bar = Toolbar::new();
    bar.toggle(
        ViewBtn::Fmt,
        "cells",
        &app.cache.data_fmt.label().to_lowercase(),
        ControlState::chip(true, hov(CacheHoverTarget::ViewFmt)),
        theme::ACCENT,
    )
    .toggle(
        ViewBtn::Group,
        "group",
        // Float cells are always four bytes wide, so the grouping control has
        // nothing to offer — it goes inert rather than disappearing, which
        // would reflow the two controls after it.
        &if is_float {
            "4b".to_string()
        } else {
            app.cache.data_group.label().to_lowercase()
        },
        if is_float {
            ControlState::Disabled
        } else {
            ControlState::chip(true, hov(CacheHoverTarget::ViewGroup))
        },
        theme::CACHE_D,
    )
    .toggle(
        ViewBtn::Addr,
        "addr",
        &app.cache.addr_mode.label().to_lowercase(),
        ControlState::chip(true, hov(CacheHoverTarget::ViewTag)),
        theme::LABEL_Y,
    );
    bar
}

/// Draw the legend: a dim prefix, the toggle group, then the read-only notes.
/// Records each toggle's rendered columns so `mouse` can ask where they landed.
fn render_legend(
    f: &mut Frame,
    area: Rect,
    app: &App,
    addr_hint: String,
    policy_hint: String,
) {
    let mut row = SpanRow::new(area.x, area.y);
    row.push(Span::raw(" "));
    row.push(Span::styled("V D", Style::default().fg(theme::LABEL_Y)));
    row.push(Span::styled(" valid / dirty", style::label()));
    row.gap(3);

    let bar = build_view_legend_bar(app);
    let origin = row.cursor();
    for (id, start, end) in bar.cells() {
        let cell = match id {
            ViewBtn::Fmt => &app.cache.view_fmt_btn,
            ViewBtn::Group => &app.cache.view_group_btn,
            ViewBtn::Addr => &app.cache.view_tag_btn,
        };
        cell.set((area.y, origin + start, origin + end));
    }
    for span in bar.spans() {
        row.push(span);
    }

    if !addr_hint.is_empty() {
        row.push(Span::styled(addr_hint, style::label()));
    }
    row.gap(3);
    row.push(Span::styled(policy_hint, style::label()));
    row.gap(3);
    row.push(Span::styled(vertical_scroll_hint(app), style::label()));

    f.render_widget(Paragraph::new(row.into_line()), area);
}

fn update_vertical_scroll_stats(
    app: &App,
    icache: bool,
    num_sets: usize,
    visible_sets: usize,
    max_scroll: usize,
) {
    let num_sets_cell = if icache {
        &app.cache.view_num_sets
    } else {
        &app.cache.view_num_sets_d
    };
    num_sets_cell.set(num_sets_cell.get().max(num_sets));
    let visible_cell = if icache {
        &app.cache.view_visible_sets
    } else {
        &app.cache.view_visible_sets_d
    };
    let cur_visible = visible_cell.get();
    visible_cell.set(if cur_visible == 0 {
        visible_sets
    } else {
        cur_visible.min(visible_sets)
    });
    let max_cell = if icache {
        &app.cache.view_scroll_max
    } else {
        &app.cache.view_scroll_max_d
    };
    max_cell.set(max_cell.get().max(max_scroll));
}

fn vertical_scroll_hint(app: &App) -> String {
    let use_d = app.cache.selected_level == 0 && matches!(app.cache.scope, CacheScope::DCache)
        || (app.cache.selected_level == 0
            && matches!(app.cache.scope, CacheScope::Both)
            && matches!(app.cache.view_focus, crate::ui::app::CacheViewFocus::DCache));
    let num_sets = if use_d {
        app.cache.view_num_sets_d.get()
    } else {
        app.cache.view_num_sets.get()
    };
    // Which rows the window is showing, not which keys move it — the arrows
    // that used to lead this readout are in the footer with the other keys.
    if num_sets == 0 {
        return "sets 0/0".to_string();
    }
    let visible_sets = if use_d {
        app.cache.view_visible_sets_d.get()
    } else {
        app.cache.view_visible_sets.get()
    }
    .max(1);
    let scroll = if use_d {
        app.cache
            .view_scroll_d
            .min(app.cache.view_scroll_max_d.get())
    } else {
        app.cache.view_scroll.min(app.cache.view_scroll_max.get())
    };
    let start = scroll + 1;
    let end = (scroll + visible_sets).min(num_sets);
    format!("sets {start}-{end}/{num_sets}")
}

/// Full policy hint (for single-scope display).
fn policy_hint_str(p: ReplacementPolicy) -> String {
    match p {
        ReplacementPolicy::Lru => {
            "r:N = recency rank  (cyan 0=just used / red N=evict next)".into()
        }
        ReplacementPolicy::Mru => {
            "r:N = recency rank  (red 0=just used=EVICT / cyan N=safe)".into()
        }
        ReplacementPolicy::Fifo => {
            "r:N = arrival order  (cyan 0=newest / red N=oldest=evict next)".into()
        }
        ReplacementPolicy::Lfu => "f:N = access count  (red=fewest accesses=evict next)".into(),
        ReplacementPolicy::Clock => {
            "> = clock pointer  R = recently used (protected)  > no R = evict next".into()
        }
        ReplacementPolicy::Random => "random eviction — no priority ordering".into(),
    }
}

/// Short policy hint used when showing both caches side by side with different policies.
fn policy_hint_short(p: ReplacementPolicy) -> &'static str {
    match p {
        ReplacementPolicy::Lru => "LRU  r:N recency  cyan=safe  red=evict",
        ReplacementPolicy::Mru => "MRU  r:N recency  red=just-used=EVICT",
        ReplacementPolicy::Fifo => "FIFO  r:N order  cyan=newest  red=evict",
        ReplacementPolicy::Lfu => "LFU  f:N=count  red=fewest=evict",
        ReplacementPolicy::Clock => "Clock  >=pointer  R=protected",
        ReplacementPolicy::Random => "Random",
    }
}

// ── Cache matrix ──────────────────────────────────────────────────────────────

fn render_extra_cache_matrix(
    f: &mut Frame,
    area: Rect,
    app: &App,
    caches: &dyn CacheHierarchy,
    level: usize,
) {
    let cache = caches.cache(level, CacheRole::Unified).unwrap();
    let level_name = &cache.name;
    let cfg = &cache.config;

    let title = if cfg.is_enabled() {
        let policy_str = match cfg.replacement {
            ReplacementPolicy::Lru => "LRU",
            ReplacementPolicy::Mru => "MRU",
            ReplacementPolicy::Fifo => "FIFO",
            ReplacementPolicy::Random => "Rand",
            ReplacementPolicy::Lfu => "LFU",
            ReplacementPolicy::Clock => "Clock",
        };
        format!(
            "{level_name} Unified · {}B · {}S · {}W · {}B/L · {policy_str}",
            cfg.size,
            cfg.num_sets(),
            cfg.associativity,
            cfg.line_size
        )
    } else {
        format!("{level_name}: disabled")
    };

    let inner = render_panel(f, area, panel::panel(title, PanelKind::Accent));

    if !cfg.is_enabled() {
        if inner.height > 0 {
            f.render_widget(
                Paragraph::new("Cache disabled — configure it in the Settings tab")
                    .style(style::label())
                    .alignment(Alignment::Center),
                inner,
            );
        }
        return;
    }

    if inner.height < 2 || inner.width < 20 {
        return;
    }

    let num_sets = cfg.num_sets();
    let ways = cfg.associativity;
    let policy = cfg.replacement;

    let set_col_w: usize = 5;
    let sep_w: usize = 1;
    let policy_w = match policy {
        ReplacementPolicy::Lfu => 6,
        ReplacementPolicy::Clock => 4,
        ReplacementPolicy::Random => 2,
        _ => 4,
    };
    // Fixed overhead: V + D + spacing + address block + spacing before data/policy.
    let cell_overhead = 17 + policy_w;
    let cell_overhead = cell_overhead - 10 + addr_text_width(app.cache.addr_mode, cfg);
    let total_way_space =
        (inner.width as usize).saturating_sub(set_col_w + sep_w + ways.saturating_sub(1) * sep_w);
    let ideal_way_col_w = total_way_space / ways.max(1);
    let min_way_col_w = (cell_overhead + 2).max(28);
    let way_col_w = ideal_way_col_w.max(min_way_col_w);
    let fmt = app.cache.data_fmt;
    let group = if fmt == CacheDataFmt::Float {
        CacheDataGroup::B4
    } else {
        app.cache.data_group
    };

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
    }
    .max(1);
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
    update_vertical_scroll_stats(app, true, num_sets, visible_sets.max(1), max_scroll);

    // Header
    {
        let mut spans: Vec<Span> = vec![
            Span::styled(
                format!("{:^width$}", "Set", width = set_col_w),
                Style::default().fg(theme::LABEL_Y).bold(),
            ),
            Span::styled("│", Style::default().fg(theme::BORDER)),
        ];
        for w in 0..ways {
            spans.push(Span::styled(
                format!("{:^width$}", format!("Way {w}"), width = way_col_w),
                Style::default().fg(theme::LABEL_Y).bold(),
            ));
            if w + 1 < ways {
                spans.push(Span::styled("│", Style::default().fg(theme::BORDER)));
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
        if set_idx >= num_sets || term_row >= rows_h {
            break;
        }
        let Some(set) = caches.set(level, CacheRole::Unified, set_idx) else {
            break;
        };
        let has_valid = set.lines.iter().any(|l| l.valid);
        let set_style = if has_valid {
            style::value()
        } else {
            style::label()
        };
        for sub_row in 0..row_height {
            if term_row >= rows_h {
                break 'sets;
            }
            let y = inner.y + header_h + term_row;
            let set_col = if sub_row == 0 {
                Span::styled(
                    format!("{:>width$} ", set_idx, width = set_col_w - 1),
                    set_style,
                )
            } else {
                Span::raw(" ".repeat(set_col_w))
            };
            let byte_offset = sub_row * bytes_per_row;
            let mut spans = vec![
                set_col,
                Span::styled("│", Style::default().fg(theme::BORDER)),
            ];
            for w in 0..ways {
                let cell = build_cell(
                    &set.lines[w],
                    &set,
                    w,
                    true, // unified = can be dirty
                    policy,
                    cfg,
                    set_idx,
                    bytes_per_row,
                    byte_offset,
                    sub_row == 0,
                    way_col_w,
                    fmt,
                    group,
                    app.cache.addr_mode,
                );
                spans.extend(cell);
                if w + 1 < ways {
                    spans.push(Span::styled("│", Style::default().fg(theme::BORDER)));
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
        // Unified/extra levels use slot 0 only
        app.cache.hscroll_bars.set([
            Some(SbGeom {
                start: inner.x,
                len: inner.width,
                cross: sb_y,
                content: max_h_scroll,
                viewport: 0, // ratatui fall-back: viewport = bar length
                offset: h_scroll as usize,
                max: max_h_scroll,
            }),
            None,
        ]);
        let hovered = matches!(
            app.cache.hover,
            Some(CacheHoverTarget::Hscrollbar { is_dcache: false })
        );
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

fn render_cache_matrix(
    f: &mut Frame,
    area: Rect,
    app: &App,
    caches: &dyn CacheHierarchy,
    role: CacheRole,
) {
    let cache = caches.cache(0, role).unwrap();
    let icache = role == CacheRole::Instruction;
    let label = &cache.name;
    let cfg = &cache.config;

    let title = if cfg.is_enabled() {
        let policy_str = match cfg.replacement {
            ReplacementPolicy::Lru => "LRU",
            ReplacementPolicy::Mru => "MRU",
            ReplacementPolicy::Fifo => "FIFO",
            ReplacementPolicy::Random => "Rand",
            ReplacementPolicy::Lfu => "LFU",
            ReplacementPolicy::Clock => "Clock",
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

    let inner = render_panel(f, area, panel::panel(title, PanelKind::Accent));

    if !cfg.is_enabled() {
        if inner.height > 0 {
            f.render_widget(
                Paragraph::new("Cache disabled — configure it in the Settings tab")
                    .style(style::label())
                    .alignment(Alignment::Center),
                inner,
            );
        }
        return;
    }

    if inner.height < 2 || inner.width < 20 {
        return;
    }

    let num_sets = cfg.num_sets();
    let ways = cfg.associativity;
    let policy = cfg.replacement;

    // Column widths
    let set_col_w: usize = 5; // " NNN "
    let sep_w: usize = 1; // "│"

    // Fixed overhead: V + D + spacing + address block + spacing before data/policy.
    let policy_w = match policy {
        ReplacementPolicy::Lfu => 6,   // "f:9999"
        ReplacementPolicy::Clock => 4, // ">R" or "> " etc.
        ReplacementPolicy::Random => 2,
        _ => 4, // "r:NN"
    };
    let cell_overhead = 17 + policy_w - 10 + addr_text_width(app.cache.addr_mode, cfg);

    // way_col_w: prefer fitting the screen, but guarantee a useful minimum so
    // that content is always readable and horizontal scrolling becomes possible.
    let total_way_space =
        (inner.width as usize).saturating_sub(set_col_w + sep_w + ways.saturating_sub(1) * sep_w);
    let ideal_way_col_w = total_way_space / ways.max(1);
    let min_way_col_w = (cell_overhead + 2).max(28); // at least readable
    let way_col_w = ideal_way_col_w.max(min_way_col_w);

    let fmt = app.cache.data_fmt;
    let group = if fmt == CacheDataFmt::Float {
        CacheDataGroup::B4
    } else {
        app.cache.data_group
    };

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
    }
    .max(1);

    // Total logical content width (may exceed inner.width for large associativity)
    let total_content_w = set_col_w + sep_w + ways * way_col_w + ways.saturating_sub(1) * sep_w;

    // Horizontal scroll (each panel has its own scroll position)
    let max_h_scroll = total_content_w.saturating_sub(inner.width as usize);
    let raw_h_scroll = if icache {
        app.cache.view_h_scroll
    } else {
        app.cache.view_h_scroll_d
    };
    let h_scroll = raw_h_scroll.min(max_h_scroll) as u16;
    let need_h_scrollbar = max_h_scroll > 0;

    // Heights: header(1) + optional scrollbar(1) + set rows (rest)
    let header_h: u16 = 1;
    let scrollbar_h: u16 = if need_h_scrollbar { 1 } else { 0 };
    let rows_h = inner.height.saturating_sub(header_h + scrollbar_h);

    // Clamp vertical scroll
    let visible_sets = (rows_h as usize) / row_height;
    let max_scroll = num_sets.saturating_sub(visible_sets.max(1));
    let scroll = if icache {
        app.cache.view_scroll
    } else {
        app.cache.view_scroll_d
    }
    .min(max_scroll);
    update_vertical_scroll_stats(app, icache, num_sets, visible_sets.max(1), max_scroll);

    // ── Header row ───────────────────────────────────────────────────────────
    {
        let mut spans: Vec<Span> = vec![
            Span::styled(
                format!("{:^width$}", "Set", width = set_col_w),
                Style::default().fg(theme::LABEL_Y).bold(),
            ),
            Span::styled("│", Style::default().fg(theme::BORDER)),
        ];
        for w in 0..ways {
            spans.push(Span::styled(
                format!("{:^width$}", format!("Way {w}"), width = way_col_w),
                Style::default().fg(theme::LABEL_Y).bold(),
            ));
            if w + 1 < ways {
                spans.push(Span::styled("│", Style::default().fg(theme::BORDER)));
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
        if set_idx >= num_sets || term_row >= rows_h {
            break;
        }
        let Some(set) = caches.set(0, role, set_idx) else {
            break;
        };
        let has_valid = set.lines.iter().any(|l| l.valid);
        let set_style = if has_valid {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        for sub_row in 0..row_height {
            if term_row >= rows_h {
                break 'sets;
            }
            let y = inner.y + header_h + term_row;
            let set_col = if sub_row == 0 {
                Span::styled(
                    format!("{:>width$} ", set_idx, width = set_col_w - 1),
                    set_style,
                )
            } else {
                Span::raw(" ".repeat(set_col_w))
            };
            let byte_offset = sub_row * bytes_per_row;
            let mut spans = vec![
                set_col,
                Span::styled("│", Style::default().fg(theme::BORDER)),
            ];
            for w in 0..ways {
                let cell = build_cell(
                    &set.lines[w],
                    &set,
                    w,
                    !icache,
                    policy,
                    cfg,
                    set_idx,
                    bytes_per_row,
                    byte_offset,
                    sub_row == 0,
                    way_col_w,
                    fmt,
                    group,
                    app.cache.addr_mode,
                );
                spans.extend(cell);
                if w + 1 < ways {
                    spans.push(Span::styled("│", Style::default().fg(theme::BORDER)));
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
        // slot 0 = I-cache, slot 1 = D-cache
        let slot = usize::from(!icache);
        let mut bars = app.cache.hscroll_bars.get();
        bars[slot] = Some(SbGeom {
            start: inner.x,
            len: inner.width,
            cross: sb_y,
            content: max_h_scroll,
            viewport: 0, // ratatui fall-back: viewport = bar length
            offset: raw_h_scroll.min(max_h_scroll),
            max: max_h_scroll,
        });
        app.cache.hscroll_bars.set(bars);
        // Highlight if this specific scrollbar is hovered
        let hovered = matches!(app.cache.hover, Some(CacheHoverTarget::Hscrollbar { is_dcache }) if is_dcache == !icache);
        let style = if hovered {
            Style::default().fg(Color::White).bg(Color::Rgb(50, 50, 70))
        } else {
            Style::default()
        };
        let mut sb_state =
            ScrollbarState::new(max_h_scroll).position(raw_h_scroll.min(max_h_scroll));
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
    let g = if fmt == CacheDataFmt::Float {
        4
    } else {
        group.bytes()
    };
    let chars = match (fmt, g) {
        (CacheDataFmt::Hex, 1) => 3,    // "XX "
        (CacheDataFmt::Hex, 2) => 5,    // "XXXX "
        (CacheDataFmt::Hex, 4) => 9,    // "XXXXXXXX "
        (CacheDataFmt::DecU, 1) => 4,   // "NNN " (0–255)
        (CacheDataFmt::DecU, 2) => 6,   // "NNNNN " (0–65535)
        (CacheDataFmt::DecU, 4) => 11,  // "NNNNNNNNNN " (0–4294967295)
        (CacheDataFmt::DecS, 1) => 5,   // "-NNN " (−128–127)
        (CacheDataFmt::DecS, 2) => 7,   // "-NNNNN " (−32768–32767)
        (CacheDataFmt::DecS, 4) => 12,  // "-NNNNNNNNNN "
        (CacheDataFmt::Float, _) => 10, // "±NNN.NNN "
        _ => 3,
    };
    (chars, g)
}

/// How many bytes fit in `budget` chars for the given format + grouping.
fn bytes_from_budget(
    budget: usize,
    fmt: CacheDataFmt,
    group: CacheDataGroup,
    line_size: usize,
) -> usize {
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

fn render_data(
    data: &[u8],
    max_bytes: usize,
    fmt: CacheDataFmt,
    group: CacheDataGroup,
    tint: Option<Color>,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let dim = style::label();
    let g = if fmt == CacheDataFmt::Float {
        4
    } else {
        group.bytes()
    };
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
                    1 => format!("{:3}", v as u8),
                    2 => format!("{:5}", v as u16),
                    _ => format!("{:10}", v as u32),
                };
                (s, v == 0, false)
            }
            CacheDataFmt::DecS => {
                let v = read_le(data, offset, g);
                let (s, neg) = match g {
                    1 => {
                        let x = v as i8;
                        (format!("{x:4}"), x < 0)
                    }
                    2 => {
                        let x = v as i16;
                        (format!("{x:6}"), x < 0)
                    }
                    _ => {
                        let x = v as i32;
                        (format!("{x:11}"), x < 0)
                    }
                };
                (s, v == 0, neg)
            }
            CacheDataFmt::Float => {
                let bytes = [
                    data.get(offset).copied().unwrap_or(0),
                    data.get(offset + 1).copied().unwrap_or(0),
                    data.get(offset + 2).copied().unwrap_or(0),
                    data.get(offset + 3).copied().unwrap_or(0),
                ];
                let f = f32::from_le_bytes(bytes);
                let s = if f.is_nan() {
                    "     NaN".to_string()
                } else if f.is_infinite() {
                    if f > 0.0 {
                        "    +Inf".to_string()
                    } else {
                        "    -Inf".to_string()
                    }
                } else {
                    format!("{f:8.3}")
                };
                (s, f == 0.0, f < 0.0)
            }
        };

        let style = tint.map_or_else(
            || {
                if is_zero {
                    dim
                } else if neg {
                    style::danger()
                } else {
                    style::value()
                }
            },
            |c| Style::default().fg(c),
        );
        spans.push(Span::styled(s, style));
        if i + 1 < n_units {
            spans.push(Span::raw(" "));
        }
    }
    spans
}

// ── Cell builder ──────────────────────────────────────────────────────────────

fn build_cell(
    line: &CacheLineView<'_>,
    set: &CacheSetView<'_>,
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
    addr_mode: CacheAddrMode,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    // ── Invalid line ─────────────────────────────────────────────────────────
    if !line.valid {
        if is_first_row {
            spans.push(Span::styled("0", style::label()));
            spans.push(Span::raw(" "));
            spans.push(Span::styled("-", style::label()));
            let rest = cell_width.saturating_sub(3);
            spans.push(Span::styled(
                format!("{:<width$}", "  (empty)", width = rest),
                style::label(),
            ));
        } else {
            spans.push(Span::raw(" ".repeat(cell_width)));
        }
        return spans;
    }

    // ── Valid line, continuation row ─────────────────────────────────────────
    if !is_first_row {
        let is_dirty = is_dcache && line.dirty;
        let addr_w = addr_text_width(addr_mode, cfg);
        // Prefix width = V(1)+sp(1)+D(1)+sp(2)+addr(addr_w)+sp(2)
        spans.push(Span::raw(" ".repeat(7 + addr_w)));
        if byte_offset < line.data.len() && bytes_per_row > 0 {
            let tint = if is_dirty { Some(DIRTY_COLOR) } else { None };
            spans.extend(render_data(
                &line.data[byte_offset..],
                bytes_per_row,
                fmt,
                group,
                tint,
            ));
        }
        // enforce cell_width (truncate/pad handled by the common block below)
        let used: usize = spans.iter().map(Span::width).sum();
        if used < cell_width {
            spans.push(Span::raw(" ".repeat(cell_width - used)));
        } else if used > cell_width {
            let mut out: Vec<Span<'static>> = Vec::with_capacity(spans.len());
            let mut budget = cell_width;
            for span in spans {
                if budget == 0 {
                    break;
                }
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
        return spans;
    }

    let is_dirty = is_dcache && line.dirty;

    // V bit
    spans.push(Span::styled("1", style::success().bold()));
    spans.push(Span::raw(" "));

    // D bit
    if is_dcache {
        if is_dirty {
            spans.push(Span::styled("1", Style::default().fg(DIRTY_COLOR).bold()));
        } else {
            spans.push(Span::styled("0", style::label()));
        }
    } else {
        spans.push(Span::styled("-", style::label()));
    }
    spans.push(Span::raw("  "));

    // Address block: either the line base or its off/index/tag decomposition.
    // Cyan "MRU" highlight only for policies where recency == safety (LRU, MRU).
    // LFU evicts by frequency, not recency — cyan highlight would be misleading there.
    let is_mru = matches!(policy, ReplacementPolicy::Lru | ReplacementPolicy::Mru)
        && set.lru_order.first() == Some(&way);
    let addr_style = if is_dirty {
        Style::default().fg(DIRTY_COLOR)
    } else if is_mru {
        Style::default().fg(theme::ACCENT).bold()
    } else {
        style::value()
    };
    match addr_mode {
        CacheAddrMode::Base => {
            let base = (line.tag << (cfg.offset_bits() + cfg.index_bits()))
                | ((set_idx as u64) << cfg.offset_bits());
            let width = cfg.address_bits.div_ceil(4);
            spans.push(Span::styled(format!("0x{base:0width$X}"), addr_style));
        }
        CacheAddrMode::Breakdown => {
            let off_hex_w = (cfg.offset_bits().saturating_add(3) / 4).max(1);
            let idx_w = cfg.num_sets().saturating_sub(1).to_string().len().max(1);
            let tag_w = cfg
                .address_bits
                .saturating_sub(cfg.offset_bits() + cfg.index_bits())
                .div_ceil(4)
                .max(1);
            let breakdown = format!(
                "o:{:0off_hex_w$X} i:{:0idx_w$} t:{:0tag_w$X}",
                0,
                set_idx,
                line.tag,
                off_hex_w = off_hex_w,
                idx_w = idx_w,
                tag_w = tag_w
            );
            spans.push(Span::styled(breakdown, addr_style));
        }
    }

    // Data (first row: bytes [byte_offset .. byte_offset + bytes_per_row])
    if bytes_per_row > 0 && byte_offset < line.data.len() {
        spans.push(Span::raw("  "));
        let tint = if is_dirty { Some(DIRTY_COLOR) } else { None };
        spans.extend(render_data(
            &line.data[byte_offset..],
            bytes_per_row,
            fmt,
            group,
            tint,
        ));
    }

    // Policy metadata
    spans.push(Span::raw("  "));
    // Count valid lines in this set (used by LRU/MRU to detect trivial rank)
    let valid_ways = set.lines.iter().filter(|l| l.valid).count();
    match policy {
        ReplacementPolicy::Lru => {
            // r:? when only one way is valid — rank is trivially 0 with no competition
            if valid_ways < 2 {
                spans.push(Span::styled("r:?", style::idle()));
            } else {
                let rank = set.lru_order.iter().position(|&w| w == way).unwrap_or(0);
                let n = set.lru_order.len();
                let style = if rank == 0 {
                    Style::default().fg(theme::ACCENT) // MRU = safest
                } else if rank + 1 == n {
                    style::danger().bold() // LRU = evicted next
                } else {
                    style::label()
                };
                spans.push(Span::styled(format!("r:{rank}"), style));
            }
        }
        ReplacementPolicy::Mru => {
            let rank = set.lru_order.iter().position(|&w| w == way).unwrap_or(0);
            let n = set.lru_order.len();
            // MRU evicts the most recently used → rank 0 is the DANGER zone
            // r:? when only one way is valid — no competition yet
            if valid_ways < 2 {
                spans.push(Span::styled("r:?", style::idle()));
            } else {
                let style = if rank == 0 {
                    style::danger().bold() // MRU = evicted next!
                } else if rank + 1 == n {
                    Style::default().fg(theme::ACCENT) // LRU = safest for MRU
                } else {
                    style::label()
                };
                spans.push(Span::styled(format!("r:{rank}"), style));
            }
        }
        ReplacementPolicy::Fifo => {
            let pos = set.fifo_order.iter().position(|&w| w == way).unwrap_or(0);
            let n = set.fifo_order.len();
            let style = if pos == 0 {
                Style::default().fg(theme::ACCENT) // newest
            } else if pos + 1 == n {
                style::danger().bold() // oldest = evicted next
            } else {
                style::label()
            };
            spans.push(Span::styled(format!("r:{pos}"), style));
        }
        ReplacementPolicy::Lfu => {
            let freq = line.frequency;
            let min_freq = set
                .lines
                .iter()
                .filter(|l| l.valid)
                .map(|l| l.frequency)
                .min()
                .unwrap_or(0);
            let style = if freq == min_freq {
                style::danger().bold()
            } else {
                Style::default().fg(theme::DIRTY)
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
            let (icon, style) = match (is_hand, line.referenced) {
                (true, true) => (">R", Style::default().fg(theme::LABEL_Y).bold()),
                (true, false) => ("> ", style::danger().bold()),
                (false, true) => (" R", Style::default().fg(theme::LABEL_Y)),
                (false, false) => ("  ", style::label()),
            };
            spans.push(Span::styled(icon, style));
        }
        ReplacementPolicy::Random => {
            spans.push(Span::styled("??", style::label()));
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
            if budget == 0 {
                break;
            }
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
