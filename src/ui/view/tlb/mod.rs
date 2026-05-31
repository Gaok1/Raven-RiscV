// ui/view/tlb/ — Top-level TLB / Virtual Memory tab renderer.
//
// Four subviews: Stats (hits/misses/page-faults + hit-rate chart), Settings
// (edit pending TlbConfig + presets + apply), Entries (table of installed
// translations), Status (live satp/priv/vm_enabled + "translation active?"
// banner — added to explain why the TLB looks idle when satp=Bare or priv=M).

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::ui::app::{App, TlbHoverTarget, TlbSubtab};
use crate::ui::theme;

mod config;
mod entries;
mod stats;
mod status;

pub(super) fn render_tlb_tab(f: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);

    render_subtab_header(f, layout[0], app);
    match app.tlb.subtab {
        TlbSubtab::Stats => stats::render_stats(f, layout[1], app),
        TlbSubtab::Config => config::render_config(f, layout[1], app),
        TlbSubtab::Entries => entries::render_entries(f, layout[1], app),
        TlbSubtab::Status => status::render_status(f, layout[1], app),
    }
}

fn render_subtab_header(f: &mut Frame, area: Rect, app: &App) {
    let stats_style = btn_style(
        matches!(app.tlb.subtab, TlbSubtab::Stats),
        matches!(app.tlb.hover, Some(TlbHoverTarget::SubtabStats)),
    );
    let config_style = btn_style(
        matches!(app.tlb.subtab, TlbSubtab::Config),
        matches!(app.tlb.hover, Some(TlbHoverTarget::SubtabConfig)),
    );
    let entries_style = btn_style(
        matches!(app.tlb.subtab, TlbSubtab::Entries),
        matches!(app.tlb.hover, Some(TlbHoverTarget::SubtabEntries)),
    );
    let status_style = btn_style(
        matches!(app.tlb.subtab, TlbSubtab::Status),
        matches!(app.tlb.hover, Some(TlbHoverTarget::SubtabStatus)),
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            "TLB / Virtual Memory",
            Style::default().fg(theme::ACCENT).bold(),
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
    let status_x0 = x;
    x += "vm".len() as u16;
    let status_x1 = x;
    x += 3;
    let config_x0 = x;
    x += "settings".len() as u16;
    let config_x1 = x;
    app.tlb.subtab_stats_btn.set((row_y, stats_x0, stats_x1));
    app.tlb.subtab_config_btn.set((row_y, config_x0, config_x1));
    app.tlb
        .subtab_entries_btn
        .set((row_y, entries_x0, entries_x1));
    app.tlb
        .subtab_status_btn
        .set((row_y, status_x0, status_x1));

    let active = translation_active(app);
    let chip_text = if !app.run.vm_enabled {
        "vm=off (toggle in Settings)"
    } else if active {
        "vm=on · translating"
    } else {
        "vm=on · inactive (satp=Bare or priv=M)"
    };
    let chip_color = if active {
        theme::RUNNING
    } else if app.run.vm_enabled {
        theme::ACCENT
    } else {
        theme::PAUSED
    };
    let line1 = Line::from(vec![
        Span::raw(" "),
        Span::styled("stats", stats_style),
        Span::raw("   "),
        Span::styled("entries", entries_style),
        Span::raw("   "),
        Span::styled("vm", status_style),
        Span::raw("   "),
        Span::styled("settings", config_style),
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

pub(super) fn translation_active(app: &App) -> bool {
    use crate::falcon::mmu::{PrivMode, SatpMode};
    let mmu = app.run.mem.mmu();
    app.run.vm_enabled && mmu.satp.mode() == SatpMode::Sv32 && mmu.priv_mode != PrivMode::M
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
