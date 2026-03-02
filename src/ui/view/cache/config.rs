// ui/view/cache/config.rs — Cache configuration subtab
use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
};

use crate::falcon::cache::{CacheConfig, InclusionPolicy, ReplacementPolicy, WriteAllocPolicy, WritePolicy, extra_level_presets};
use crate::ui::app::{App, ConfigField, CpiConfig};

pub(super) fn render_config(f: &mut Frame, area: Rect, app: &App) {
    if app.cache.selected_level == 0 {
        // L1: three-column layout (I-Cache | D-Cache | CPI Config)
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(38), Constraint::Percentage(38), Constraint::Percentage(24)])
            .split(area);
        render_cache_config_panel(f, cols[0], app, true);
        render_cache_config_panel(f, cols[1], app, false);
        render_cpi_panel(f, cols[2], app);
    } else {
        // L2+: single-column unified config
        render_unified_config(f, area, app, app.cache.selected_level - 1);
    }
}

// ── CPI Config panel ─────────────────────────────────────────────────────────

fn render_cpi_panel(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled("CPI Config", Style::default().fg(Color::Rgb(100, 220, 180)).bold()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height == 0 { return; }

    let names = CpiConfig::field_names();
    let descs = CpiConfig::descriptions();
    let selected = app.cache.cpi_selected;
    let editing = app.cache.cpi_editing;

    let hover = app.cache.hover_cpi_field;
    let items: Vec<ListItem> = names.iter().enumerate().map(|(i, &name)| {
        let val = if editing && i == selected {
            format!("{}_", app.cache.cpi_edit_buf)
        } else {
            format!("{}", app.run.cpi_config.get(i))
        };
        let desc = descs.get(i).copied().unwrap_or("");
        let is_sel = i == selected;
        let is_hov = hover == Some(i);

        let name_style = if is_sel {
            Style::default().fg(Color::Black).bg(Color::Rgb(100, 220, 180)).bold()
        } else if is_hov {
            Style::default().fg(Color::Rgb(100, 220, 180)).bg(Color::Rgb(30, 50, 40))
        } else {
            Style::default().fg(Color::Rgb(100, 220, 180))
        };
        let val_style = if is_sel && editing {
            Style::default().fg(Color::Yellow).bold()
        } else if is_sel || is_hov {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::White)
        };
        let desc_style = if is_hov {
            Style::default().fg(Color::Gray)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let line = Line::from(vec![
            Span::styled(format!("{name:<10}"), name_style),
            Span::styled(format!("{val:>4}  "), val_style),
            Span::styled(desc.to_string(), desc_style),
        ]);
        ListItem::new(line)
    }).collect();

    f.render_widget(List::new(items), inner);
}

fn render_cache_config_panel(f: &mut Frame, area: Rect, app: &App, icache: bool) {
    let cfg = if icache { &app.cache.pending_icache } else { &app.cache.pending_dcache };
    let label = if icache { "I-Cache Config" } else { "D-Cache Config" };
    let current = if icache { &app.run.mem.icache.config } else { &app.run.mem.dcache.config };

    // Determine which field (if any) is being edited in this panel
    let (active_field, edit_buf) = match app.cache.edit_field {
        Some((panel, field)) if panel == icache => (Some(field), app.cache.edit_buf.as_str()),
        _ => (None, ""),
    };
    let hovered_field = match app.cache.hover_config_field {
        Some((panel, field)) if panel == icache => Some(field),
        _ => None,
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

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // config fields
            Constraint::Length(3), // presets
            Constraint::Length(3), // apply + error
        ])
        .split(inner);

    let is_last_level = app.cache.extra_pending.is_empty();
    render_fields(f, layout[0], cfg, current, active_field, hovered_field, edit_buf, is_last_level);
    render_presets(f, layout[1], app, icache);
    // Apply is global (applies both I-cache and D-cache), so render it only once to avoid duplication.
    if icache {
        render_apply_row(f, layout[2], app);
    }
}

fn render_unified_config(f: &mut Frame, area: Rect, app: &App, extra_idx: usize) {
    let level_name = crate::falcon::cache::CacheController::extra_level_name(extra_idx);
    let label = format!("{level_name} Config (Unified)");

    let pending = if extra_idx < app.cache.extra_pending.len() {
        &app.cache.extra_pending[extra_idx]
    } else {
        return;
    };
    let current = if extra_idx < app.run.mem.extra_levels.len() {
        &app.run.mem.extra_levels[extra_idx].config
    } else {
        pending
    };

    // For L2+, edit_field is_icache is ignored (always "false" column)
    let (active_field, edit_buf) = match app.cache.edit_field {
        Some((_, field)) => (Some(field), app.cache.edit_buf.as_str()),
        _ => (None, ""),
    };
    let hovered_field = app.cache.hover_config_field.map(|(_, f)| f);

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

    // Centered single column (max 60 wide)
    let col_w = inner.width.min(60);
    let col_x = inner.x + (inner.width.saturating_sub(col_w)) / 2;
    let col_area = Rect::new(col_x, inner.y, col_w, inner.height);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(3), // presets
            Constraint::Length(3), // apply + error
        ])
        .split(col_area);

    let is_last_level = extra_idx == app.cache.extra_pending.len().saturating_sub(1);
    render_fields(f, layout[0], pending, current, active_field, hovered_field, edit_buf, is_last_level);
    render_unified_presets(f, layout[1], app, extra_idx);
    render_apply_row(f, layout[2], app);
}

fn render_fields(
    f: &mut Frame, area: Rect,
    pending: &CacheConfig, current: &CacheConfig,
    active: Option<ConfigField>, hovered: Option<ConfigField>, edit_buf: &str,
    is_last_level: bool,
) {
    let validation = pending.validate();

    // Per-field validity marks (✗ only for the directly responsible field)
    let line_ok = pending.line_size >= 4 && pending.line_size.is_power_of_two();
    let assoc_ok = pending.associativity >= 1;
    let size_ok = pending.size > 0 && validation.is_ok();

    let mark = |ok: bool| if ok { "" } else { " ✗" };
    // Yellow = pending change from active config, White = same
    let cs = |same: bool| -> Style {
        if !same { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::White) }
    };
    // Style for the active/selected field
    let active_style = Style::default().fg(Color::Black).bg(Color::Cyan);
    let label_active = Style::default().fg(Color::Black).bg(Color::Cyan);
    let hover_style = Style::default().bg(Color::DarkGray);

    let field_item = |field: ConfigField, label: &'static str, value: String, val_style: Style| -> ListItem<'static> {
        let mut item = if active == Some(field) {
            if field.is_numeric() {
                // Show edit buffer with cursor
                let display = format!("{edit_buf}█");
                ListItem::new(Line::from(vec![
                    Span::styled(label, label_active),
                    Span::styled(display, active_style),
                ]))
            } else {
                // Show enum value with ◄ ► arrows to indicate clickable
                ListItem::new(Line::from(vec![
                    Span::styled(label, label_active),
                    Span::styled(format!("◄ {value} ►"), active_style),
                ]))
            }
        } else {
            ListItem::new(Line::from(vec![
                Span::styled(label, Style::default().fg(Color::Gray)),
                Span::styled(value, val_style),
            ]))
        };

        if active != Some(field) && hovered == Some(field) {
            item = item.style(hover_style);
        }
        item
    };

    // Sets row: show computed value or the specific validation error
    let sets_item = match &validation {
        Ok(()) => ListItem::new(Line::from(vec![
            Span::styled("  Sets:          ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}", pending.num_sets()),
                Style::default().fg(Color::DarkGray),
            ),
        ])),
        Err(msg) => ListItem::new(Line::from(vec![
            Span::styled("  Sets:          ", Style::default().fg(Color::Gray)),
            Span::styled(format!("✗ {msg}"), Style::default().fg(Color::Red)),
        ])),
    };

    let items: Vec<ListItem> = vec![
        field_item(ConfigField::Size, "  Size:          ",
            format!("{} B{}", pending.size, mark(size_ok)),
            cs(pending.size == current.size)),
        field_item(ConfigField::LineSize, "  Line Size:     ",
            format!("{} B{}", pending.line_size, mark(line_ok)),
            cs(pending.line_size == current.line_size)),
        field_item(ConfigField::Associativity, "  Associativity: ",
            format!("{}-way{}", pending.associativity, mark(assoc_ok)),
            cs(pending.associativity == current.associativity)),
        sets_item,
        field_item(ConfigField::Replacement, "  Replacement:   ",
            replacement_label(pending.replacement).to_string(),
            cs(pending.replacement == current.replacement)),
        field_item(ConfigField::WritePolicy, "  Write Policy:  ",
            write_policy_label(pending.write_policy).to_string(),
            cs(pending.write_policy == current.write_policy)),
        field_item(ConfigField::WriteAlloc, "  Write Alloc:   ",
            write_alloc_label(pending.write_alloc).to_string(),
            cs(pending.write_alloc == current.write_alloc)),
        field_item(ConfigField::HitLatency, "  Hit Latency:   ",
            format!("{} cyc", pending.hit_latency),
            cs(pending.hit_latency == current.hit_latency)),
        field_item(ConfigField::MissPenalty, "  Miss Penalty:  ",
            format!("{} cyc", pending.miss_penalty),
            cs(pending.miss_penalty == current.miss_penalty)),
        field_item(ConfigField::AssocPenalty, "  Assoc Penalty: ",
            format!("{} cyc/way", pending.assoc_penalty),
            cs(pending.assoc_penalty == current.assoc_penalty)),
        field_item(ConfigField::TransferWidth, "  Transfer Width:",
            format!("{} B", pending.transfer_width),
            cs(pending.transfer_width == current.transfer_width)),
        if is_last_level {
            ListItem::new(Line::from(vec![
                Span::styled("  Inclusion:      ", Style::default().fg(Color::DarkGray)),
                Span::styled("N/A (last level)", Style::default().fg(Color::DarkGray)),
            ]))
        } else {
            field_item(ConfigField::Inclusion, "  Inclusion:      ",
                inclusion_label(pending.inclusion).to_string(),
                cs(pending.inclusion == current.inclusion))
        },
        ListItem::new(Line::raw("")),
        ListItem::new(Line::from(Span::styled(
            if active.is_some() {
                "  Enter=confirm  Esc=cancel  ◄►=cycle  Tab/↑↓=move"
            } else {
                "  Click/edit  ◄►=cycle  Ctrl+E=export  Ctrl+L=import"
            },
            Style::default().fg(Color::DarkGray),
        ))),
    ];

    f.render_widget(List::new(items), area);
}

fn render_presets(f: &mut Frame, area: Rect, app: &App, icache: bool) {
    let preset_i = app.cache.hover_preset_i;
    let preset_d = app.cache.hover_preset_d;
    let hovered = if icache { preset_i } else { preset_d };

    let small_s = preset_btn_style(hovered == Some(0));
    let med_s = preset_btn_style(hovered == Some(1));
    let large_s = preset_btn_style(hovered == Some(2));

    let line = Line::from(vec![
        Span::raw(" Presets: "),
        Span::styled("[Small]", small_s),
        Span::raw(" "),
        Span::styled("[Medium]", med_s),
        Span::raw(" "),
        Span::styled("[Large]", large_s),
    ]);
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(line), inner);
}

fn render_unified_presets(f: &mut Frame, area: Rect, app: &App, _extra_idx: usize) {
    // hover_preset_d is reused for unified presets
    let hovered = app.cache.hover_preset_d;
    let small_s = preset_btn_style(hovered == Some(0));
    let med_s   = preset_btn_style(hovered == Some(1));
    let large_s = preset_btn_style(hovered == Some(2));

    let presets = extra_level_presets();
    let line = Line::from(vec![
        Span::raw(" Presets: "),
        Span::styled(format!("[Small {}KB]", presets[0].size / 1024), small_s),
        Span::raw(" "),
        Span::styled(format!("[Med {}KB]",   presets[1].size / 1024), med_s),
        Span::raw(" "),
        Span::styled(format!("[Large {}KB]", presets[2].size / 1024), large_s),
    ]);
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(line), inner);
}

fn render_apply_row(f: &mut Frame, area: Rect, app: &App) {
    let apply_s = if app.cache.hover_apply {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    } else {
        Style::default().fg(Color::Black).bg(Color::Green)
    };
    let keep_s = if app.cache.hover_apply_keep {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    } else {
        Style::default().fg(Color::Black).bg(Color::Blue)
    };

    let line = if let Some(ref err) = app.cache.config_error {
        Line::from(Span::styled(format!(" ✗ {err}"), Style::default().fg(Color::Red)))
    } else if let Some(ref status) = app.cache.config_status {
        Line::from(Span::styled(format!(" ✓ {status}"), Style::default().fg(Color::Green)))
    } else {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("[Apply + Reset Stats]", apply_s),
            Span::raw("  "),
            Span::styled("[Apply Keep History]", keep_s),
        ])
    };

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(line), inner);
}

fn preset_btn_style(hovered: bool) -> Style {
    if hovered {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    } else {
        Style::default().fg(Color::Cyan)
    }
}

pub fn replacement_label(r: ReplacementPolicy) -> &'static str {
    match r {
        ReplacementPolicy::Lru => "LRU (Least Recently Used)",
        ReplacementPolicy::Mru => "MRU (Most Recently Used)",
        ReplacementPolicy::Fifo => "FIFO (First In First Out)",
        ReplacementPolicy::Random => "Random",
        ReplacementPolicy::Lfu => "LFU (Least Frequently Used)",
        ReplacementPolicy::Clock => "Clock (Second Chance)",
    }
}

pub fn write_policy_label(w: WritePolicy) -> &'static str {
    match w {
        WritePolicy::WriteThrough => "Write-Through",
        WritePolicy::WriteBack => "Write-Back",
    }
}

pub fn write_alloc_label(w: WriteAllocPolicy) -> &'static str {
    match w {
        WriteAllocPolicy::WriteAllocate => "Write-Allocate",
        WriteAllocPolicy::NoWriteAllocate => "No-Write-Allocate",
    }
}

pub fn inclusion_label(p: InclusionPolicy) -> &'static str {
    match p {
        InclusionPolicy::NonInclusive => "Non-Inclusive (NINE)",
        InclusionPolicy::Inclusive    => "Inclusive",
        InclusionPolicy::Exclusive    => "Exclusive",
    }
}
