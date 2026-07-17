// ui/view/tlb/ — Top-level Virtual Memory tab renderer.
//
// Mirrors the Cache tab's frame: a single flat header selects one of five
// subviews (VmSubtab) — Overview (live satp/priv/vm banner + quick mode/TLB
// controls), Map (the live page-table tree), Tlb (installed translations),
// Stats (counters + hit-rate chart + shared session snapshots) and Settings
// (the single comprehensive VM control panel). An Execution box (speed /
// state / reset + cycles) sits above the content and a shared controls bar
// (results / import / export / flush tlb) below it, so the whole simulation
// can be driven without leaving the tab.

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::ui::app::{App, TlbHoverTarget, VmSubtab};
use crate::ui::theme;
use crate::ui::view::components::{dense_action, render_exec_controls};

mod entries;
mod overview;
mod page_tree;
mod stats;
mod vm_settings;

pub(super) fn render_tlb_tab(f: &mut Frame, area: Rect, app: &App) {
    // Layout mirrors the Cache tab: header | exec controls | content | bar.
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // subtab header
            Constraint::Length(4), // exec controls (Speed / State / Cycles)
            Constraint::Min(0),    // content
            Constraint::Length(3), // shared controls bar
        ])
        .split(area);

    render_vm_header(f, layout[0], app);

    let hint = if matches!(app.tlb.vm_subtab, VmSubtab::Stats) {
        "   r=reset  f=speed  p=pause  s=capture  ↑↓=history  D=del"
    } else {
        "   r=reset  f=speed  p=pause"
    };
    render_exec_controls(
        f,
        layout[1],
        app,
        &app.tlb.exec_speed_btn,
        &app.tlb.exec_state_btn,
        &app.tlb.exec_reset_btn,
        hint,
    );

    match app.tlb.vm_subtab {
        VmSubtab::Overview => overview::render_overview(f, layout[2], app),
        VmSubtab::Map => page_tree::render_page_tree(f, layout[2], app),
        VmSubtab::Tlb => {
            if !app.run.tlb_enabled {
                render_tlb_disabled_notice(f, layout[2]);
            } else {
                entries::render_entries(f, layout[2], app);
            }
        }
        VmSubtab::Stats => stats::render_stats(f, layout[2], app),
        VmSubtab::Settings => vm_settings::render_vm_settings(f, layout[2], app),
    }

    render_controls_bar(f, layout[3], app);

    // The session snapshots are shared with the Cache tab; viewing one from
    // the Stats subtab opens the same popup.
    if matches!(app.tlb.vm_subtab, VmSubtab::Stats) && app.cache.viewing_snapshot.is_some() {
        crate::ui::view::cache::render_snapshot_popup(f, area, app);
    }
}

/// Flat Virtual Memory header: [overview] [map] [tlb] [stats] [settings] + chip.
fn render_vm_header(f: &mut Frame, area: Rect, app: &App) {
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

    let mut btns = [(0u16, 0u16, 0u16); 5];
    let mut spans: Vec<Span> = Vec::new();
    let mut x = inner.x + 1;
    spans.push(Span::raw(" "));
    for (i, sub) in VmSubtab::ALL.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
            x += 3;
        }
        let label = if *sub == VmSubtab::Tlb && !app.run.tlb_enabled {
            "tlb(off)"
        } else {
            sub.label()
        };
        let style = btn_style(
            app.tlb.vm_subtab == *sub,
            matches!(&app.tlb.hover, Some(TlbHoverTarget::Subtab(s)) if s == sub),
        );
        btns[i] = (row_y, x, x + label.len() as u16);
        x += label.len() as u16;
        spans.push(Span::styled(label, style));
    }
    app.tlb.subtab_btns.set(btns);

    use crate::falcon::mmu::VmMode;
    let active = translation_active(app);
    let chip_text = match (app.vm_mode(), active) {
        (VmMode::Off, _) => "vm=off (enable in overview)",
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
    spans.push(Span::raw("   "));
    spans.push(Span::styled(chip_text, Style::default().fg(chip_color)));

    let line1 = Line::from(spans);
    let line2 = Line::from(vec![
        Span::raw(" "),
        Span::styled("Tab to switch", Style::default().fg(theme::LABEL)),
    ]);

    f.render_widget(block, area);
    f.render_widget(Paragraph::new(vec![line1, line2]), inner);
}

/// Shared controls bar — visible on every Virtual Memory subtab.
fn render_controls_bar(f: &mut Frame, area: Rect, app: &App) {
    let show_cfg_btns = matches!(app.tlb.vm_subtab, VmSubtab::Settings);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER));
    let inner = block.inner(area);
    let row_y = inner.y;
    let mut x = inner.x + 1; // leading space
    let results_x0 = x;
    x += "results".len() as u16;
    app.tlb.ctrl_results_btn.set((row_y, results_x0, x));
    app.tlb.ctrl_import_btn.set((0, 0, 0));
    app.tlb.ctrl_export_btn.set((0, 0, 0));

    let mut line_spans = vec![
        Span::raw(" "),
        dense_action(
            "results",
            theme::ACCENT,
            matches!(app.tlb.hover, Some(TlbHoverTarget::ExportResults)),
        ),
    ];

    if show_cfg_btns {
        x += 3;
        let import_x0 = x;
        x += "import cfg".len() as u16;
        app.tlb.ctrl_import_btn.set((row_y, import_x0, x));
        x += 3;
        let export_x0 = x;
        x += "export cfg".len() as u16;
        app.tlb.ctrl_export_btn.set((row_y, export_x0, x));

        line_spans.push(Span::raw("   "));
        line_spans.push(dense_action(
            "import cfg",
            theme::METRIC_CYC,
            matches!(app.tlb.hover, Some(TlbHoverTarget::ImportCfg)),
        ));
        line_spans.push(Span::raw("   "));
        line_spans.push(dense_action(
            "export cfg",
            theme::METRIC_CYC,
            matches!(app.tlb.hover, Some(TlbHoverTarget::ExportCfg)),
        ));
    }

    x += 3;
    let flush_x0 = x;
    x += "flush tlb".len() as u16;
    app.tlb.ctrl_flush_btn.set((row_y, flush_x0, x));
    line_spans.push(Span::raw("   "));
    line_spans.push(dense_action(
        "flush tlb",
        theme::DANGER,
        matches!(app.tlb.hover, Some(TlbHoverTarget::FlushTlb)),
    ));

    f.render_widget(block, area);
    f.render_widget(Paragraph::new(Line::from(line_spans)), inner);
}

/// Shown in the tlb subtab when the TLB is disabled in Settings.
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
            "  Toggle \"TLB\" in the overview (or settings) subtab to cache translations.",
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
    if active || hovered {
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

#[cfg(test)]
mod tests {
    use crate::ui::app::{Tab, VmSubtab};
    use crossterm::event::{KeyCode, KeyEvent};
    use ratatui::{Terminal, backend::TestBackend};

    fn render_all_subtabs(app: &mut crate::ui::app::App, label: &str) {
        for sub in VmSubtab::ALL {
            app.tlb.vm_subtab = sub;
            for (w, h) in [(120u16, 40u16), (80, 24), (60, 16), (30, 8)] {
                let backend = TestBackend::new(w, h);
                let mut terminal = Terminal::new(backend).expect("terminal");
                terminal
                    .draw(|f| super::render_tlb_tab(f, f.area(), app))
                    .unwrap_or_else(|e| panic!("{label}: render {sub:?} at {w}x{h} failed: {e}"));
            }
        }
    }

    #[test]
    fn vm_tab_renders_every_subtab_without_panicking() {
        let mut app = crate::ui::app::App::new(None);
        app.tab = Tab::Tlb;
        // Switching tabs in the real app drops back to command mode.
        app.mode = crate::ui::app::EditorMode::Command;

        // Plain state, VM off.
        render_all_subtabs(&mut app, "vm off");

        // VM on (didactic sv32) + a real session snapshot captured through the
        // keyboard path (exercises TlbSnapshot) + the popup over Stats.
        app.set_vm_mode(crate::falcon::mmu::VmMode::Sv32);
        app.tlb.vm_subtab = VmSubtab::Stats;
        crate::ui::input::handle_key(&mut app, KeyEvent::from(KeyCode::Char('s')))
            .expect("capture snapshot");
        assert_eq!(app.cache.session_history.len(), 1);
        assert!(app.cache.session_history[0].tlb.is_some(), "vm on ⇒ tlb snapshot");
        app.cache.viewing_snapshot = Some(0);
        render_all_subtabs(&mut app, "vm on + snapshot");

        // Custom mode exposes the editable paging-scheme rows; TLB off shows
        // the disabled notice on the tlb subtab.
        app.set_vm_mode(crate::falcon::mmu::VmMode::Custom);
        app.set_tlb_enabled(false);
        render_all_subtabs(&mut app, "custom + tlb off");
    }
}
