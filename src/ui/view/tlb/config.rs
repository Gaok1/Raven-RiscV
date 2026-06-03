// ui/view/tlb/config.rs — TLB configuration form (entries, associativity,
// replacement, latencies) + presets + apply/flush row.

use ratatui::{
    Frame,
    prelude::*,
    widgets::{List, ListItem},
};

use crate::ui::app::{App, TlbConfigField, TlbHoverTarget};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{dense_action, dense_value};

pub(super) fn render_config(f: &mut Frame, area: Rect, app: &App) {
    app.tlb.config_hitboxes.set([(0, 0, 0); 5]);
    app.tlb.preset_btns.set([(0, 0, 0); 3]);
    app.tlb.apply_btn.set((0, 0, 0));
    app.tlb.flush_btn.set((0, 0, 0));

    let col_w = area.width.min(60);
    let col_x = area.x + (area.width.saturating_sub(col_w)) / 2;
    let col_area = Rect::new(col_x, area.y, col_w, area.height);

    let block = panel::panel("TLB Settings", PanelKind::Plain);
    let inner = block.inner(col_area);
    f.render_widget(block, col_area);
    if inner.height == 0 {
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(3), // presets
            Constraint::Length(3), // apply + flush
        ])
        .split(inner);

    record_config_hitboxes(app, layout[0]);
    render_fields(f, layout[0], app);
    render_presets(f, layout[1], app);
    render_apply_row(f, layout[2], app);
}

fn record_config_hitboxes(app: &App, area: Rect) {
    let mut hits = [(0, 0, 0); 5];
    for &field in TlbConfigField::all_editable() {
        let row_y = area.y.saturating_add(field.list_row() as u16);
        if row_y < area.y.saturating_add(area.height) {
            hits[field.hitbox_index()] = (row_y, area.x, area.x.saturating_add(area.width));
        }
    }
    app.tlb.config_hitboxes.set(hits);
}

fn render_fields(f: &mut Frame, area: Rect, app: &App) {
    let pending = &app.tlb.pending;
    let active = app.tlb.edit_field;
    let edit_buf = app.tlb.edit_buf.as_str();
    let hovered = match &app.tlb.hover {
        Some(TlbHoverTarget::ConfigField(f)) => Some(*f),
        _ => None,
    };

    let current = &app.run.mem.mmu().tlb.config;
    let value_color = |same: bool| if same { theme::TEXT } else { theme::LABEL_Y };

    let entry_ok = pending.entry_count >= pending.associativity as u16;
    let assoc_ok = pending.associativity >= 1;
    let mark = |ok: bool| if ok { "" } else { " ✗" };

    let field_item = |field: TlbConfigField,
                      label: &'static str,
                      value: String,
                      same: bool|
     -> ListItem<'static> {
        let label_style = if active == Some(field) {
            Style::default().fg(theme::ACCENT).bold()
        } else if hovered == Some(field) {
            Style::default().fg(theme::TEXT).bold()
        } else {
            Style::default().fg(theme::LABEL)
        };
        if active == Some(field) {
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
        }
    };

    let assoc = pending.associativity.max(1) as usize;
    let raw = (pending.entry_count.max(1) as usize)
        .next_power_of_two()
        .max(assoc);
    let n_entries = ((raw + assoc - 1) / assoc) * assoc;
    let n_sets = (n_entries / assoc).max(1);

    let items: Vec<ListItem> = vec![
        field_item(
            TlbConfigField::EntryCount,
            "  Entries:       ",
            format!("{}{}", pending.entry_count, mark(entry_ok)),
            pending.entry_count == current.entry_count,
        ),
        field_item(
            TlbConfigField::Associativity,
            "  Associativity: ",
            format!("{}-way{}", pending.associativity, mark(assoc_ok)),
            pending.associativity == current.associativity,
        ),
        ListItem::new(Line::from(vec![
            Span::styled("  Sets:          ", Style::default().fg(theme::LABEL)),
            Span::styled(format!("{}", n_sets), Style::default().fg(theme::BORDER)),
        ])),
        field_item(
            TlbConfigField::Replacement,
            "  Replacement:   ",
            super::replacement_label(pending.replacement).to_string(),
            pending.replacement == current.replacement,
        ),
        field_item(
            TlbConfigField::HitLatency,
            "  Hit Latency:   ",
            format!("{} cyc", pending.hit_latency),
            pending.hit_latency == current.hit_latency,
        ),
        field_item(
            TlbConfigField::MissPenalty,
            "  Miss Penalty:  ",
            format!("{} cyc", pending.miss_penalty),
            pending.miss_penalty == current.miss_penalty,
        ),
        ListItem::new(Line::raw("")),
        ListItem::new(Line::from(Span::styled(
            if active.is_some() {
                "  Enter=confirm  Esc=cancel  ←→=cycle  Tab/↑↓=move"
            } else {
                "  Click to edit  ←→=cycle"
            },
            Style::default().fg(theme::LABEL),
        ))),
    ];
    f.render_widget(List::new(items), area);
}

fn render_presets(f: &mut Frame, area: Rect, app: &App) {
    let hovered = match &app.tlb.hover {
        Some(TlbHoverTarget::Preset(i)) => Some(*i),
        _ => None,
    };
    let style = |on: bool| {
        if on {
            Style::default().fg(theme::TEXT).bold()
        } else {
            Style::default().fg(theme::ACCENT).bold()
        }
    };
    let labels = ["small 16", "med 32", "large 64"];
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("presets", Style::default().fg(theme::IDLE)),
        Span::raw(" "),
        Span::styled(labels[0], style(hovered == Some(0))),
        Span::raw(" "),
        Span::styled(labels[1], style(hovered == Some(1))),
        Span::raw(" "),
        Span::styled(labels[2], style(hovered == Some(2))),
    ]);
    let inner = render_panel(f, area, panel::handle_bar(theme::BORDER));
    let x0 = inner.x + 9;
    let mut x = x0;
    let mut btns = [(0u16, 0u16, 0u16); 3];
    for (i, lbl) in labels.iter().enumerate() {
        btns[i] = (inner.y, x, x + lbl.len() as u16);
        x += lbl.len() as u16 + 1;
    }
    app.tlb.preset_btns.set(btns);
    f.render_widget(ratatui::widgets::Paragraph::new(line), inner);
}

fn render_apply_row(f: &mut Frame, area: Rect, app: &App) {
    let show_buttons = app.tlb.config_error.is_none() && app.tlb.config_status.is_none();
    let line = if let Some(ref err) = app.tlb.config_error {
        Line::from(Span::styled(
            format!(" ✗ {err}"),
            Style::default().fg(theme::DANGER),
        ))
    } else if let Some(ref status) = app.tlb.config_status {
        Line::from(Span::styled(
            format!(" ✓ {status}"),
            Style::default().fg(theme::RUNNING),
        ))
    } else {
        Line::from(vec![
            Span::raw(" "),
            dense_action(
                "apply",
                theme::RUNNING,
                matches!(app.tlb.hover, Some(TlbHoverTarget::Apply)),
            ),
            Span::raw("   "),
            dense_action(
                "flush tlb",
                theme::DANGER,
                matches!(app.tlb.hover, Some(TlbHoverTarget::Flush)),
            ),
        ])
    };
    let inner = render_panel(f, area, panel::handle_bar(theme::BORDER));
    if show_buttons {
        app.tlb
            .apply_btn
            .set((inner.y, inner.x + 1, inner.x + 1 + "apply".len() as u16));
        let flush_x0 = inner.x + 1 + "apply".len() as u16 + 3;
        app.tlb.flush_btn.set((
            inner.y,
            flush_x0,
            flush_x0 + "flush tlb".len() as u16,
        ));
    }
    f.render_widget(ratatui::widgets::Paragraph::new(line), inner);
}
