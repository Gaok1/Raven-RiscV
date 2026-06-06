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
use crate::ui::view::components::{ControlState, Toolbar, field_row};
use crate::ui::view::style;

/// A button in the TLB config preset / apply rows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TlbConfigBtn {
    Preset(usize),
    Apply,
    Flush,
}

/// The `presets` row — `small 16  med 32  large 64` (after a dim `presets `
/// label). Keyed by [`TlbConfigBtn::Preset`].
pub(crate) fn build_tlb_preset_bar(app: &App) -> Toolbar<TlbConfigBtn> {
    let hovered = match &app.tlb.hover {
        Some(TlbHoverTarget::Preset(i)) => Some(*i),
        _ => None,
    };
    let labels = ["small 16", "med 32", "large 64"];
    let mut bar = Toolbar::with_gap(1);
    for (i, lbl) in labels.iter().enumerate() {
        bar.action(
            TlbConfigBtn::Preset(i),
            lbl,
            ControlState::chip(false, hovered == Some(i)),
            theme::ACCENT,
        );
    }
    bar
}

/// The `apply   flush tlb` row. Keyed by [`TlbConfigBtn`].
pub(crate) fn build_tlb_apply_bar(app: &App) -> Toolbar<TlbConfigBtn> {
    let mut bar = Toolbar::new();
    bar.action(
        TlbConfigBtn::Apply,
        "apply",
        ControlState::chip(false, matches!(app.tlb.hover, Some(TlbHoverTarget::Apply))),
        theme::RUNNING,
    )
    .action(
        TlbConfigBtn::Flush,
        "flush tlb",
        ControlState::chip(false, matches!(app.tlb.hover, Some(TlbHoverTarget::Flush))),
        theme::DANGER,
    );
    bar
}

pub(super) fn render_config(f: &mut Frame, area: Rect, app: &App) {
    app.tlb.config_hitboxes.set([(0, 0, 0); 5]);
    app.tlb.preset_origin.set((0, 0));
    app.tlb.apply_origin.set((0, 0));

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

    let entry_ok = pending.entry_count >= pending.associativity as u16;
    let assoc_ok = pending.associativity >= 1;
    let mark = |ok: bool| if ok { "" } else { " ✗" };

    let field_item =
        |field: TlbConfigField, label: &'static str, value: String, same: bool| -> ListItem<'static> {
            field_row(
                label,
                &value,
                active == Some(field),
                field.is_numeric(),
                edit_buf,
                hovered == Some(field),
                !same,
            )
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
            Span::styled("  Sets:          ", style::label()),
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
            style::label(),
        ))),
    ];
    f.render_widget(List::new(items), area);
}

fn render_presets(f: &mut Frame, area: Rect, app: &App) {
    let inner = render_panel(f, area, panel::handle_bar(theme::BORDER));
    // Bar starts right after the dim `presets ` label.
    app.tlb.preset_origin.set((inner.y, inner.x + 9));
    let mut spans = vec![
        Span::raw(" "),
        Span::styled("presets", style::idle()),
        Span::raw(" "),
    ];
    spans.extend(build_tlb_preset_bar(app).spans());
    f.render_widget(ratatui::widgets::Paragraph::new(Line::from(spans)), inner);
}

fn render_apply_row(f: &mut Frame, area: Rect, app: &App) {
    let inner = render_panel(f, area, panel::handle_bar(theme::BORDER));
    let line = if let Some(ref err) = app.tlb.config_error {
        app.tlb.apply_origin.set((0, 0));
        Line::from(Span::styled(format!(" ✗ {err}"), style::danger()))
    } else if let Some(ref status) = app.tlb.config_status {
        app.tlb.apply_origin.set((0, 0));
        Line::from(Span::styled(format!(" ✓ {status}"), style::success()))
    } else {
        app.tlb.apply_origin.set((inner.y, inner.x + 1));
        let mut spans = vec![Span::raw(" ")];
        spans.extend(build_tlb_apply_bar(app).spans());
        Line::from(spans)
    };
    f.render_widget(ratatui::widgets::Paragraph::new(line), inner);
}
