// ui/view/tlb/ — Top-level Virtual Memory tab renderer.
//
// Mirrors the Cache tab's nesting. The Virtual Memory header selects one of
// three subviews (VmSubtab): Status (live satp/priv/vm + "translation active?"
// banner), Tree (the live Sv32 page-table tree + map-config form in Auto mode)
// and TLB. The TLB subview opens its own nested world (TlbSubtab) with a second
// header: Stats (hits/misses/page-faults + hit-rate chart), Entries (installed
// translations) and Settings (edit TlbConfig + presets + apply). The TLB world
// can be disabled in Settings, which hides its sub-header and content.

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::ui::app::{App, TlbHoverTarget, TlbSubtab, VmSubtab};
use crate::ui::theme;

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

    // Clear TLB sub-header hitboxes when hidden so stale clicks can't fire.
    if !show_tlb_subheader {
        app.tlb.tlb_stats_btn.set((0, 0, 0));
        app.tlb.tlb_entries_btn.set((0, 0, 0));
        app.tlb.tlb_settings_btn.set((0, 0, 0));
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

/// Top-level Virtual Memory header: [status] [tree] [TLB] + activity chip.
fn render_vm_header(f: &mut Frame, area: Rect, app: &App) {
    let status_style = btn_style(
        matches!(app.tlb.vm_subtab, VmSubtab::Status),
        matches!(app.tlb.hover, Some(TlbHoverTarget::VmStatus)),
    );
    let tree_style = btn_style(
        matches!(app.tlb.vm_subtab, VmSubtab::Tree),
        matches!(app.tlb.hover, Some(TlbHoverTarget::VmTree)),
    );
    let settings_style = btn_style(
        matches!(app.tlb.vm_subtab, VmSubtab::Settings),
        matches!(app.tlb.hover, Some(TlbHoverTarget::VmSettings)),
    );
    let tlb_style = btn_style(
        matches!(app.tlb.vm_subtab, VmSubtab::Tlb),
        matches!(app.tlb.hover, Some(TlbHoverTarget::VmTlb)),
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            "Virtual Memory",
            Style::default().fg(theme::ACCENT).bold(),
        ));
    let inner = block.inner(area);
    let row_y = inner.y;
    let mut x = inner.x + 1;
    let status_x0 = x;
    x += "status".len() as u16;
    let status_x1 = x;
    x += 3;
    let tree_x0 = x;
    x += "tree".len() as u16;
    let tree_x1 = x;
    x += 3;
    let settings_x0 = x;
    x += "settings".len() as u16;
    let settings_x1 = x;
    x += 3;
    let tlb_x0 = x;
    x += "TLB".len() as u16;
    let tlb_x1 = x;
    app.tlb.vm_status_btn.set((row_y, status_x0, status_x1));
    app.tlb.vm_tree_btn.set((row_y, tree_x0, tree_x1));
    app.tlb.vm_settings_btn.set((row_y, settings_x0, settings_x1));
    app.tlb.vm_tlb_btn.set((row_y, tlb_x0, tlb_x1));

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
    let tlb_label = if app.run.tlb_enabled { "TLB" } else { "TLB(off)" };
    let line1 = Line::from(vec![
        Span::raw(" "),
        Span::styled("status", status_style),
        Span::raw("   "),
        Span::styled("tree", tree_style),
        Span::raw("   "),
        Span::styled("settings", settings_style),
        Span::raw("   "),
        Span::styled(tlb_label, tlb_style),
        Span::raw("   "),
        Span::styled(chip_text, Style::default().fg(chip_color)),
    ]);
    let line2 = Line::from(vec![
        Span::raw(" "),
        Span::styled("Tab to switch", Style::default().fg(theme::LABEL)),
    ]);

    f.render_widget(block, area);
    f.render_widget(Paragraph::new(vec![line1, line2]), inner);
}

/// Nested TLB sub-header: [stats] [entries] [settings] (mirrors the Cache tab).
fn render_tlb_subheader(f: &mut Frame, area: Rect, app: &App) {
    let stats_style = btn_style(
        matches!(app.tlb.subtab, TlbSubtab::Stats),
        matches!(app.tlb.hover, Some(TlbHoverTarget::TlbStats)),
    );
    let entries_style = btn_style(
        matches!(app.tlb.subtab, TlbSubtab::Entries),
        matches!(app.tlb.hover, Some(TlbHoverTarget::TlbEntries)),
    );
    let settings_style = btn_style(
        matches!(app.tlb.subtab, TlbSubtab::Settings),
        matches!(app.tlb.hover, Some(TlbHoverTarget::TlbSettings)),
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            "Translation Lookaside Buffer",
            Style::default().fg(theme::LABEL),
        ));
    let inner = block.inner(area);
    let row_y = inner.y;
    let mut x = inner.x + 1;
    let stats_x0 = x;
    x += "stats".len() as u16;
    let stats_x1 = x;
    x += 3;
    let entries_x0 = x;
    x += "entries".len() as u16;
    let entries_x1 = x;
    x += 3;
    let settings_x0 = x;
    x += "settings".len() as u16;
    let settings_x1 = x;
    app.tlb.tlb_stats_btn.set((row_y, stats_x0, stats_x1));
    app.tlb.tlb_entries_btn.set((row_y, entries_x0, entries_x1));
    app.tlb
        .tlb_settings_btn
        .set((row_y, settings_x0, settings_x1));

    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("stats", stats_style),
        Span::raw("   "),
        Span::styled("entries", entries_style),
        Span::raw("   "),
        Span::styled("settings", settings_style),
    ]);

    f.render_widget(block, area);
    f.render_widget(Paragraph::new(line), inner);
}

/// Shown in the TLB world when the cache is disabled in Settings.
fn render_tlb_disabled_notice(f: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let lines = vec![
        Line::raw(""),
        Line::from(Span::styled(
            "  TLB is disabled.",
            Style::default().fg(theme::PAUSED).bold(),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "  Every translation walks the page table (miss + penalty, no hits).",
            Style::default().fg(theme::LABEL),
        )),
        Line::from(Span::styled(
            "  Enable \"TLB Enabled\" in the Settings tab to cache translations.",
            Style::default().fg(theme::LABEL),
        )),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

pub(super) fn translation_active(app: &App) -> bool {
    use crate::falcon::mmu::{PrivMode, SatpMode};
    let mmu = app.run.mem().mmu();
    // Mirror `Mmu::translate`: in Auto mode (force_translate) even M-mode
    // translates, so the priv-level gate only applies otherwise.
    let priv_ok = mmu.priv_mode != PrivMode::M || mmu.force_translate;
    app.run.vm_enabled() && mmu.satp.mode() == SatpMode::Sv32 && priv_ok
}

fn btn_style(active: bool, hovered: bool) -> Style {
    if active {
        Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD)
    } else if hovered {
        Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::IDLE)
    }
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
