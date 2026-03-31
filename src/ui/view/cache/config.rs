// ui/view/cache/config.rs — Cache configuration subtab
use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
};

use crate::falcon::cache::{
    CacheConfig, InclusionPolicy, ReplacementPolicy, WriteAllocPolicy, WritePolicy,
    extra_level_presets,
};
use crate::ui::app::{App, CacheHoverTarget, ConfigField};
use crate::ui::theme;
use crate::ui::view::components::{dense_action, dense_value};

pub(super) fn render_config(f: &mut Frame, area: Rect, app: &App) {
    app.cache.config_hitboxes_i.set([(0, 0, 0); 11]);
    app.cache.config_hitboxes_d.set([(0, 0, 0); 11]);
    app.cache.config_hitboxes_u.set([(0, 0, 0); 11]);
    app.cache.config_preset_btns_i.set([(0, 0, 0); 3]);
    app.cache.config_preset_btns_d.set([(0, 0, 0); 3]);
    app.cache.config_preset_btns_u.set([(0, 0, 0); 3]);
    app.cache.config_apply_btns.set([(0, 0, 0); 2]);
    if app.cache.selected_level == 0 {
        // L1: two-column layout (I-Cache | D-Cache)
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        render_cache_config_panel(f, cols[0], app, true);
        render_cache_config_panel(f, cols[1], app, false);
    } else {
        // L2+: single-column unified config
        render_unified_config(f, area, app, app.cache.selected_level - 1);
    }
}

fn render_cache_config_panel(f: &mut Frame, area: Rect, app: &App, icache: bool) {
    let cfg = if icache {
        &app.cache.pending_icache
    } else {
        &app.cache.pending_dcache
    };
    let label = if icache {
        "I-Cache Config"
    } else {
        "D-Cache Config"
    };
    let current = if icache {
        &app.run.mem.icache.config
    } else {
        &app.run.mem.dcache.config
    };

    // Determine which field (if any) is being edited in this panel
    let (active_field, edit_buf) = match app.cache.edit_field {
        Some((panel, field)) if panel == icache => (Some(field), app.cache.edit_buf.as_str()),
        _ => (None, ""),
    };
    let hovered_field = match app.cache.hover {
        Some(CacheHoverTarget::ConfigField(panel, field)) if panel == icache => Some(field),
        _ => None,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            label,
            Style::default().fg(theme::ACCENT).bold(),
        ));
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
    record_config_field_hitboxes(app, layout[0], Some(icache), is_last_level);
    render_fields(
        f,
        layout[0],
        cfg,
        current,
        active_field,
        hovered_field,
        edit_buf,
        is_last_level,
    );
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
    let hovered_field = match app.cache.hover {
        Some(CacheHoverTarget::ConfigField(_, f)) => Some(f),
        _ => None,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            label,
            Style::default().fg(theme::ACCENT).bold(),
        ));
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
    record_config_field_hitboxes(app, layout[0], None, is_last_level);
    render_fields(
        f,
        layout[0],
        pending,
        current,
        active_field,
        hovered_field,
        edit_buf,
        is_last_level,
    );
    render_unified_presets(f, layout[1], app, extra_idx);
    render_apply_row(f, layout[2], app);
}

fn record_config_field_hitboxes(app: &App, area: Rect, icache: Option<bool>, is_last_level: bool) {
    let mut hitboxes = [(0, 0, 0); 11];
    for &field in ConfigField::all_editable() {
        if field == ConfigField::Inclusion && is_last_level {
            continue;
        }
        let row_y = area.y.saturating_add(field.list_row() as u16);
        if row_y < area.y.saturating_add(area.height) {
            hitboxes[field.hitbox_index()] = (row_y, area.x, area.x.saturating_add(area.width));
        }
    }
    match icache {
        Some(true) => app.cache.config_hitboxes_i.set(hitboxes),
        Some(false) => app.cache.config_hitboxes_d.set(hitboxes),
        None => app.cache.config_hitboxes_u.set(hitboxes),
    }
}

fn render_fields(
    f: &mut Frame,
    area: Rect,
    pending: &CacheConfig,
    current: &CacheConfig,
    active: Option<ConfigField>,
    hovered: Option<ConfigField>,
    edit_buf: &str,
    is_last_level: bool,
) {
    let validation = pending.validate();

    // Per-field validity marks (✗ only for the directly responsible field)
    let line_ok = pending.line_size >= 4 && pending.line_size.is_power_of_two();
    let assoc_ok = pending.associativity >= 1;
    let size_ok = pending.size > 0 && validation.is_ok();

    let mark = |ok: bool| if ok { "" } else { " ✗" };
    let value_color = |same: bool| if same { theme::TEXT } else { theme::LABEL_Y };

    let field_item =
        |field: ConfigField, label: &'static str, value: String, same: bool| -> ListItem<'static> {
            let label_style = if active == Some(field) {
                Style::default().fg(theme::ACCENT).bold()
            } else if hovered == Some(field) {
                Style::default().fg(theme::TEXT).bold()
            } else {
                Style::default().fg(theme::LABEL)
            };
            let item = if active == Some(field) {
                if field.is_numeric() {
                    let display = format!("{edit_buf}█");
                    ListItem::new(Line::from(vec![
                        Span::styled(label, label_style),
                        dense_value(&display, false, true, theme::ACCENT),
                    ]))
                } else {
                    ListItem::new(Line::from(vec![
                        Span::styled(label, label_style),
                        dense_value(&format!("< {value} >"), false, true, theme::ACCENT),
                    ]))
                }
            } else {
                ListItem::new(Line::from(vec![
                    Span::styled(label, label_style),
                    dense_value(&value, hovered == Some(field), true, value_color(same)),
                ]))
            };
            item
        };

    // Sets row: show computed value or the specific validation error
    let sets_item = match &validation {
        Ok(()) => ListItem::new(Line::from(vec![
            Span::styled("  Sets:          ", Style::default().fg(theme::LABEL)),
            Span::styled(
                format!("{}", pending.num_sets()),
                Style::default().fg(theme::BORDER),
            ),
        ])),
        Err(msg) => ListItem::new(Line::from(vec![
            Span::styled("  Sets:          ", Style::default().fg(theme::LABEL)),
            Span::styled(format!("✗ {msg}"), Style::default().fg(theme::DANGER)),
        ])),
    };

    let items: Vec<ListItem> = vec![
        field_item(
            ConfigField::Size,
            "  Size:          ",
            format!("{} B{}", pending.size, mark(size_ok)),
            pending.size == current.size,
        ),
        field_item(
            ConfigField::LineSize,
            "  Line Size:     ",
            format!("{} B{}", pending.line_size, mark(line_ok)),
            pending.line_size == current.line_size,
        ),
        field_item(
            ConfigField::Associativity,
            "  Associativity: ",
            format!("{}-way{}", pending.associativity, mark(assoc_ok)),
            pending.associativity == current.associativity,
        ),
        sets_item,
        field_item(
            ConfigField::Replacement,
            "  Replacement:   ",
            replacement_label(pending.replacement).to_string(),
            pending.replacement == current.replacement,
        ),
        field_item(
            ConfigField::WritePolicy,
            "  Write Policy:  ",
            write_policy_label(pending.write_policy).to_string(),
            pending.write_policy == current.write_policy,
        ),
        field_item(
            ConfigField::WriteAlloc,
            "  Write Alloc:   ",
            write_alloc_label(pending.write_alloc).to_string(),
            pending.write_alloc == current.write_alloc,
        ),
        field_item(
            ConfigField::HitLatency,
            "  Hit Latency:   ",
            format!("{} cyc", pending.hit_latency),
            pending.hit_latency == current.hit_latency,
        ),
        field_item(
            ConfigField::MissPenalty,
            "  Miss Penalty:  ",
            format!("{} cyc", pending.miss_penalty),
            pending.miss_penalty == current.miss_penalty,
        ),
        field_item(
            ConfigField::AssocPenalty,
            "  Assoc Penalty: ",
            format!("{} cyc/way", pending.assoc_penalty),
            pending.assoc_penalty == current.assoc_penalty,
        ),
        field_item(
            ConfigField::TransferWidth,
            "  Transfer Width:",
            format!("{} B", pending.transfer_width),
            pending.transfer_width == current.transfer_width,
        ),
        if is_last_level {
            ListItem::new(Line::from(vec![
                Span::styled("  Inclusion:      ", Style::default().fg(theme::BORDER)),
                Span::styled("N/A (last level)", Style::default().fg(theme::BORDER)),
            ]))
        } else {
            field_item(
                ConfigField::Inclusion,
                "  Inclusion:      ",
                inclusion_label(pending.inclusion).to_string(),
                pending.inclusion == current.inclusion,
            )
        },
        ListItem::new(Line::raw("")),
        ListItem::new(Line::from(Span::styled(
            if active.is_some() {
                "  Enter=confirm  Esc=cancel  <- ->=cycle  Tab/↑↓=move"
            } else {
                "  Click/edit  ◄►=cycle  Ctrl+E=export  Ctrl+L=import"
            },
            Style::default().fg(theme::LABEL),
        ))),
    ];

    f.render_widget(List::new(items), area);
}

fn render_presets(f: &mut Frame, area: Rect, app: &App, icache: bool) {
    let hovered = match app.cache.hover {
        Some(CacheHoverTarget::PresetI(i)) if icache => Some(i),
        Some(CacheHoverTarget::PresetD(i)) if !icache => Some(i),
        _ => None,
    };

    let small_s = preset_btn_style(hovered == Some(0));
    let med_s = preset_btn_style(hovered == Some(1));
    let large_s = preset_btn_style(hovered == Some(2));

    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("presets", Style::default().fg(theme::IDLE)),
        Span::raw(" "),
        Span::styled("small", small_s),
        Span::raw(" "),
        Span::styled("medium", med_s),
        Span::raw(" "),
        Span::styled("large", large_s),
    ]);
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::BORDER));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let btns = [
        (inner.y, inner.x + 9, inner.x + 14),
        (inner.y, inner.x + 15, inner.x + 21),
        (inner.y, inner.x + 22, inner.x + 27),
    ];
    if icache {
        app.cache.config_preset_btns_i.set(btns);
    } else {
        app.cache.config_preset_btns_d.set(btns);
    }
    f.render_widget(Paragraph::new(line), inner);
}

fn render_unified_presets(f: &mut Frame, area: Rect, app: &App, _extra_idx: usize) {
    // PresetD is reused for unified presets
    let hovered = match app.cache.hover {
        Some(CacheHoverTarget::PresetD(i)) => Some(i),
        _ => None,
    };
    let small_s = preset_btn_style(hovered == Some(0));
    let med_s = preset_btn_style(hovered == Some(1));
    let large_s = preset_btn_style(hovered == Some(2));

    let presets = extra_level_presets();
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("presets", Style::default().fg(theme::IDLE)),
        Span::raw(" "),
        Span::styled(format!("small {}kb", presets[0].size / 1024), small_s),
        Span::raw(" "),
        Span::styled(format!("med {}kb", presets[1].size / 1024), med_s),
        Span::raw(" "),
        Span::styled(format!("large {}kb", presets[2].size / 1024), large_s),
    ]);
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::BORDER));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let small_label = format!("small {}kb", presets[0].size / 1024);
    let med_label = format!("med {}kb", presets[1].size / 1024);
    let large_label = format!("large {}kb", presets[2].size / 1024);
    let x0 = inner.x + 9;
    app.cache.config_preset_btns_u.set([
        (inner.y, x0, x0 + small_label.len() as u16),
        (
            inner.y,
            x0 + small_label.len() as u16 + 1,
            x0 + small_label.len() as u16 + 1 + med_label.len() as u16,
        ),
        (
            inner.y,
            x0 + small_label.len() as u16 + med_label.len() as u16 + 2,
            x0 + small_label.len() as u16 + med_label.len() as u16 + 2 + large_label.len() as u16,
        ),
    ]);
    f.render_widget(Paragraph::new(line), inner);
}

fn render_apply_row(f: &mut Frame, area: Rect, app: &App) {
    let line = if let Some(ref err) = app.cache.config_error {
        Line::from(Span::styled(
            format!(" ✗ {err}"),
            Style::default().fg(theme::DANGER),
        ))
    } else if let Some(ref status) = app.cache.config_status {
        Line::from(Span::styled(
            format!(" ✓ {status}"),
            Style::default().fg(theme::RUNNING),
        ))
    } else {
        Line::from(vec![
            Span::raw(" "),
            dense_action(
                "apply + reset stats",
                theme::RUNNING,
                matches!(app.cache.hover, Some(CacheHoverTarget::Apply)),
            ),
            Span::raw("   "),
            dense_action(
                "apply keep history",
                theme::ACCENT,
                matches!(app.cache.hover, Some(CacheHoverTarget::ApplyKeep)),
            ),
        ])
    };

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::BORDER));
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.cache.config_apply_btns.set([
        (inner.y, inner.x + 1, inner.x + 20),
        (inner.y, inner.x + 23, inner.x + 41),
    ]);
    f.render_widget(Paragraph::new(line), inner);
}

fn preset_btn_style(hovered: bool) -> Style {
    if hovered {
        Style::default().fg(theme::TEXT).bold()
    } else {
        Style::default().fg(theme::ACCENT).bold()
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
        InclusionPolicy::Inclusive => "Inclusive",
        InclusionPolicy::Exclusive => "Exclusive",
    }
}
