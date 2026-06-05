// ui/view/tlb/ — Top-level Virtual Memory tab renderer.
//
// Mirrors the Cache tab's nesting. The Virtual Memory header selects one of
// three subviews (VmSubtab): Status (live satp/priv/vm + "translation active?"
// banner), Tree (the live Sv32 page-table tree + map-config form in Auto mode)
// and TLB. The TLB subview opens its own nested world (TlbSubtab) with a second
// header: Stats (hits/misses/page-faults + hit-rate chart), Entries (installed
// translations) and Settings (edit TlbConfig + presets + apply). The TLB world
// can be disabled in Settings, which hides its sub-header and content.

use ratatui::{Frame, prelude::*, widgets::Paragraph};

use crate::ui::app::{App, TlbHoverTarget, TlbSubtab, VmSubtab};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{ControlState, Toolbar};
use crate::ui::view::style;

mod config;
mod entries;
mod page_tree;
mod stats;
mod status;
mod vm_settings;

pub(super) fn render_tlb_tab(f: &mut Frame, area: Rect, app: &App) {
    // Two header rows mirror the Cache tab's nesting: the Virtual Memory
    // selector (status / tree / TLB) always shows; the TLB sub-selector
    // (stats / entries / settings) only shows inside the TLB world.
    let show_tlb_subheader = matches!(app.tlb.vm_subtab, VmSubtab::Tlb) && app.run.tlb_enabled;
    let layout = if show_tlb_subheader {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Min(0)])
            .split(area)
    };

    render_vm_header(f, layout[0], app);

    // Clear the TLB sub-header origin when hidden so stale clicks can't fire.
    if !show_tlb_subheader {
        app.tlb.tlb_subheader_origin.set((0, 0));
    }

    let content = if show_tlb_subheader {
        render_tlb_subheader(f, layout[1], app);
        layout[2]
    } else {
        layout[1]
    };

    match app.tlb.vm_subtab {
        VmSubtab::Status => status::render_status(f, content, app),
        VmSubtab::Tree => page_tree::render_page_tree(f, content, app),
        VmSubtab::Settings => vm_settings::render_vm_settings(f, content, app),
        VmSubtab::Tlb => {
            if !app.run.tlb_enabled {
                render_tlb_disabled_notice(f, content);
            } else {
                match app.tlb.subtab {
                    TlbSubtab::Stats => stats::render_stats(f, content, app),
                    TlbSubtab::Entries => entries::render_entries(f, content, app),
                    TlbSubtab::Settings => config::render_config(f, content, app),
                }
            }
        }
    }
}

/// The Virtual Memory header bar — `[status] [tree] [settings] [TLB]` — as a
/// [`Toolbar`]: the single source shared by the renderer and the mouse hit-test
/// (`mouse::update_tlb_hover` / `handle_tlb_click`). The cell id is the
/// [`VmSubtab`] the word selects.
pub(crate) fn build_vm_header_bar(app: &App) -> Toolbar<VmSubtab> {
    let st = |sub: VmSubtab, t: TlbHoverTarget| {
        ControlState::chip(app.tlb.vm_subtab == sub, app.tlb.hover == Some(t))
    };
    let tlb_label = if app.run.tlb_enabled { "TLB" } else { "TLB(off)" };
    let mut bar = Toolbar::new();
    bar.value(VmSubtab::Status, "status", st(VmSubtab::Status, TlbHoverTarget::VmStatus), theme::ACCENT)
        .value(VmSubtab::Tree, "tree", st(VmSubtab::Tree, TlbHoverTarget::VmTree), theme::ACCENT)
        .value(VmSubtab::Settings, "settings", st(VmSubtab::Settings, TlbHoverTarget::VmSettings), theme::ACCENT)
        .value(VmSubtab::Tlb, tlb_label, st(VmSubtab::Tlb, TlbHoverTarget::VmTlb), theme::ACCENT);
    bar
}

/// Top-level Virtual Memory header: [status] [tree] [settings] [TLB] + activity chip.
fn render_vm_header(f: &mut Frame, area: Rect, app: &App) {
    let block = panel::panel("Virtual Memory", PanelKind::Accent);
    let inner = block.inner(area);
    let origin_x = inner.x + 1;
    app.tlb.vm_header_origin.set((inner.y, origin_x));

    use crate::falcon::mmu::VmMode;
    let active = translation_active(app);
    let chip_text = match (app.vm_mode(), active) {
        (VmMode::Off, _) => "vm=off (toggle in Settings)",
        (VmMode::Sv32, true) => "vm=sv32 · translating",
        (VmMode::Sv32, false) => "vm=sv32 · inactive (satp=Bare)",
        (VmMode::Custom, true) => "vm=custom · translating",
        (VmMode::Custom, false) => "vm=custom · inactive (satp=Bare)",
        (VmMode::Manual, true) => "vm=manual · translating",
        (VmMode::Manual, false) => "vm=manual · inactive (satp=Bare or priv=M)",
    };
    let chip_color = if active {
        theme::RUNNING
    } else if app.run.vm_enabled() {
        theme::ACCENT
    } else {
        theme::PAUSED
    };

    let mut spans = vec![Span::raw(" ")];
    spans.extend(build_vm_header_bar(app).spans());
    spans.push(Span::raw("   "));
    spans.push(Span::styled(chip_text, Style::default().fg(chip_color)));
    let line1 = Line::from(spans);
    let line2 = Line::from(vec![
        Span::raw(" "),
        Span::styled("Tab to switch", style::label()),
    ]);

    f.render_widget(block, area);
    f.render_widget(Paragraph::new(vec![line1, line2]), inner);
}

/// The nested TLB sub-header bar — `[stats] [entries] [settings]` — as a
/// [`Toolbar`] keyed by [`TlbSubtab`]. Shared by render and mouse, like
/// [`build_vm_header_bar`].
pub(crate) fn build_tlb_subheader_bar(app: &App) -> Toolbar<TlbSubtab> {
    let st = |sub: TlbSubtab, t: TlbHoverTarget| {
        ControlState::chip(app.tlb.subtab == sub, app.tlb.hover == Some(t))
    };
    let mut bar = Toolbar::new();
    bar.value(TlbSubtab::Stats, "stats", st(TlbSubtab::Stats, TlbHoverTarget::TlbStats), theme::ACCENT)
        .value(TlbSubtab::Entries, "entries", st(TlbSubtab::Entries, TlbHoverTarget::TlbEntries), theme::ACCENT)
        .value(TlbSubtab::Settings, "settings", st(TlbSubtab::Settings, TlbHoverTarget::TlbSettings), theme::ACCENT);
    bar
}

/// Nested TLB sub-header: [stats] [entries] [settings] (mirrors the Cache tab).
fn render_tlb_subheader(f: &mut Frame, area: Rect, app: &App) {
    let block = panel::panel("Translation Lookaside Buffer", PanelKind::Plain);
    let inner = block.inner(area);
    app.tlb.tlb_subheader_origin.set((inner.y, inner.x + 1));

    let mut spans = vec![Span::raw(" ")];
    spans.extend(build_tlb_subheader_bar(app).spans());

    f.render_widget(block, area);
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

/// Shown in the TLB world when the cache is disabled in Settings.
fn render_tlb_disabled_notice(f: &mut Frame, area: Rect) {
    let inner = render_panel(f, area, panel::panel_frame(PanelKind::Plain));
    let lines = vec![
        Line::raw(""),
        Line::from(Span::styled("  TLB is disabled.", style::warning().bold())),
        Line::raw(""),
        Line::from(Span::styled(
            "  Every translation walks the page table (miss + penalty, no hits).",
            style::label(),
        )),
        Line::from(Span::styled(
            "  Enable \"TLB Enabled\" in the Settings tab to cache translations.",
            style::label(),
        )),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

pub(super) fn translation_active(app: &App) -> bool {
    use crate::falcon::mmu::{PrivMode, SatpMode};
    let mmu = app.run.mem.mmu();
    // Mirror `Mmu::translate`: in Auto mode (force_translate) even M-mode
    // translates, so the priv-level gate only applies otherwise.
    let priv_ok = mmu.priv_mode != PrivMode::M || mmu.force_translate;
    app.run.vm_enabled() && mmu.satp.mode() == SatpMode::Sv32 && priv_ok
}

pub(super) fn replacement_label(r: crate::falcon::cache::ReplacementPolicy) -> &'static str {
    use crate::falcon::cache::ReplacementPolicy;
    match r {
        ReplacementPolicy::Lru => "LRU (Least Recently Used)",
        ReplacementPolicy::Mru => "MRU (Most Recently Used)",
        ReplacementPolicy::Fifo => "FIFO (First In First Out)",
        ReplacementPolicy::Random => "Random",
        ReplacementPolicy::Lfu => "LFU (Least Frequently Used)",
        ReplacementPolicy::Clock => "Clock (Second Chance)",
    }
}
