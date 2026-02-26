// ui/view/cache/config.rs — Cache configuration subtab
use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
};

use crate::falcon::cache::{CacheConfig, ReplacementPolicy, WriteAllocPolicy, WritePolicy};
use crate::ui::app::{App, ConfigField};

pub(super) fn render_config(f: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    render_cache_config_panel(f, cols[0], app, true);
    render_cache_config_panel(f, cols[1], app, false);
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

    render_fields(f, layout[0], cfg, current, active_field, hovered_field, edit_buf);
    render_presets(f, layout[1], app, icache);
    // Apply is global (applies both I-cache and D-cache), so render it only once to avoid duplication.
    if icache {
        render_apply_row(f, layout[2], app);
    }
}

fn render_fields(
    f: &mut Frame, area: Rect,
    pending: &CacheConfig, current: &CacheConfig,
    active: Option<ConfigField>, hovered: Option<ConfigField>, edit_buf: &str,
) {
    let num_sets = if pending.is_valid_config() { pending.num_sets() } else { 0 };

    let valid_mark = |ok: bool| if ok { "" } else { " ✗" };
    let bytes_per_set = pending.line_size.checked_mul(pending.associativity).unwrap_or(0);
    let size_ok = pending.size > 0
        && bytes_per_set > 0
        && pending.size >= bytes_per_set
        && pending.size % bytes_per_set == 0;
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

    let items: Vec<ListItem> = vec![
        field_item(ConfigField::Size, "  Size:          ",
            format!("{} B{}", pending.size, valid_mark(size_ok)),
            cs(pending.size == current.size)),
        field_item(ConfigField::LineSize, "  Line Size:     ",
            format!("{} B{}", pending.line_size, valid_mark(pending.line_size >= 4 && pending.line_size.is_power_of_two())),
            cs(pending.line_size == current.line_size)),
        field_item(ConfigField::Associativity, "  Associativity: ",
            format!("{}-way", pending.associativity),
            cs(pending.associativity == current.associativity)),
        // Sets is read-only
        ListItem::new(Line::from(vec![
            Span::styled("  Sets:          ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{} {}", num_sets, if pending.is_valid_config() { "" } else { "(invalid)" }),
                Style::default().fg(if pending.is_valid_config() { Color::DarkGray } else { Color::Red }),
            ),
        ])),
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
        ListItem::new(Line::raw("")),
        ListItem::new(Line::from(Span::styled(
            if active.is_some() {
                "  Enter=confirm  Esc=cancel  ◄►=cycle  Tab/↑↓=move"
            } else {
                "  Click field to edit  ◄►=cycle enum  (yellow=pending)"
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
        Line::from(vec![
            Span::styled(format!(" Error: {err}"), Style::default().fg(Color::Red)),
        ])
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
