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

use crate::ui::app::{App, RunButton, TlbHoverTarget, VmSubtab};
use crate::ui::theme;
use crate::ui::view::components::{ControlState, SpanRow, Toolbar};
use crate::ui::view::style;

mod entries;
mod overview;
mod page_tree;
mod stats;
mod vm_settings;

pub(super) fn render_tlb_tab(f: &mut Frame, area: Rect, app: &App) {
    // Cleared every frame; the Entries renderer re-registers its scrollbar
    // track when (and only when) the bar is actually drawn.
    app.tlb.entries_sb.set(None);

    // Layout mirrors the Cache tab: a two-line borderless header, then content.
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(VM_HEADER_H), Constraint::Min(0)])
        .split(area);

    render_vm_header(f, layout[0], app);

    let body = layout[1];
    match app.tlb.vm_subtab {
        VmSubtab::Overview => overview::render_overview(f, body, app),
        VmSubtab::Map => page_tree::render_page_tree(f, body, app),
        VmSubtab::Tlb => {
            if !app.session.tlb_enabled {
                render_tlb_disabled_notice(f, body);
            } else {
                entries::render_entries(f, body, app);
            }
        }
        VmSubtab::Stats => stats::render_stats(f, body, app),
        VmSubtab::Settings => vm_settings::render_vm_settings(f, body, app),
    }

    // The session snapshots are shared with the Cache tab; viewing one from
    // the Stats subtab opens the same popup.
    if matches!(app.tlb.vm_subtab, VmSubtab::Stats) && app.cache.viewing_snapshot.is_some() {
        crate::ui::view::cache::render_snapshot_popup(f, area, app);
    }
}

/// Rows the borderless Virtual Memory header occupies — two rows of controls
/// with a blank between them, shared with mouse hit-testing so the two cannot
/// drift. See [`crate::ui::view::cache::CACHE_HEADER_H`] on why the gap is
/// worth its row.
pub(crate) const VM_HEADER_H: u16 = 3;

/// The Virtual Memory subtab bar, as a [`Toolbar`] keyed by [`VmSubtab`].
pub(crate) fn build_vm_subtab_bar(app: &App) -> Toolbar<VmSubtab> {
    let mut bar = Toolbar::new();
    for sub in VmSubtab::ALL {
        // A disabled TLB is state, so it is said in the label rather than left
        // for the reader to discover by clicking through to an empty subtab.
        let label = if sub == VmSubtab::Tlb && !app.session.tlb_enabled {
            "tlb (off)"
        } else {
            sub.label()
        };
        bar.value(
            sub,
            label,
            ControlState::chip(
                app.tlb.vm_subtab == sub,
                matches!(&app.tlb.hover, Some(TlbHoverTarget::Subtab(s)) if *s == sub),
            ),
            theme::ACCENT,
        );
    }
    bar
}

/// The VM action group — `results`, `flush tlb`, and the config import/export
/// pair while the Settings subtab is showing.
pub(crate) fn build_vm_ctrl_bar(app: &App) -> Toolbar<TlbCtrlBtn> {
    let hov = |t: TlbHoverTarget| app.tlb.hover == Some(t);
    let mut bar = Toolbar::new();
    bar.action(
        TlbCtrlBtn::Results,
        "results",
        ControlState::chip(false, hov(TlbHoverTarget::ExportResults)),
        theme::ACCENT,
    );
    if matches!(app.tlb.vm_subtab, VmSubtab::Settings) {
        bar.action(
            TlbCtrlBtn::ImportCfg,
            "import cfg",
            ControlState::chip(false, hov(TlbHoverTarget::ImportCfg)),
            theme::METRIC_CYC,
        )
        .action(
            TlbCtrlBtn::ExportCfg,
            "export cfg",
            ControlState::chip(false, hov(TlbHoverTarget::ExportCfg)),
            theme::METRIC_CYC,
        );
    }
    bar.action(
        TlbCtrlBtn::FlushTlb,
        "flush tlb",
        ControlState::chip(false, hov(TlbHoverTarget::FlushTlb)),
        theme::DANGER,
    );
    bar
}

/// The VM transport controls — the same `speed / state / reset` group the Cache
/// tab carries, keyed by the shared [`RunButton`] ids.
pub(crate) fn build_vm_exec_bar(app: &App) -> Toolbar<RunButton> {
    let hov = |b: RunButton| app.hover_run_button == Some(b);
    let (state_text, state_color) = if app.session.is_running {
        ("run", theme::RUNNING)
    } else {
        ("pause", theme::PAUSED)
    };
    let mut bar = Toolbar::new();
    bar.toggle(
        RunButton::Speed,
        "speed",
        app.session.speed.label(),
        ControlState::chip(true, hov(RunButton::Speed)),
        theme::TEXT,
    )
    .toggle(
        RunButton::State,
        "state",
        state_text,
        ControlState::chip(true, hov(RunButton::State)),
        state_color,
    )
    .action(
        RunButton::Reset,
        "reset",
        ControlState::chip(false, hov(RunButton::Reset)),
        theme::DANGER,
    );
    bar
}

/// A button in the Virtual Memory action group.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TlbCtrlBtn {
    Results,
    ImportCfg,
    ExportCfg,
    FlushTlb,
}

/// The Virtual Memory tab's chrome, in two borderless lines.
///
/// ```text
///  Virtual Memory   overview  map  tlb  stats  settings  │  vm=sv32 · translating
///  speed 1x   state pause   reset  │  results  flush tlb        cyc 0  CPI 0.00  instr 0
/// ```
///
/// This was three nested boxes — a header panel, an `Execution` panel, and an
/// untitled panel holding the action group — spending eleven rows on borders
/// around three lines of controls, plus a `r=reset  f=speed  …` legend and a
/// `Tab to switch` line, both of which the footer and help overlay already say.
///
/// The columns each control claims come from its [`Toolbar`] now. They used to
/// be counted by hand as `x += "results".len()`, which was **wrong**: the pill
/// renders a padding column either side, so every recorded hitbox was two
/// columns narrower than the button drawn, and offset further left with each
/// control down the row.
fn render_vm_header(f: &mut Frame, area: Rect, app: &App) {
    use crate::falcon::mmu::VmMode;

    let sep = || Span::styled("│", Style::default().fg(theme::BORDER));

    // ── Line 1: identity · subtabs · translation state ──
    let mut row = SpanRow::new(area.x, area.y);
    row.push(Span::styled(
        " Virtual Memory ",
        Style::default().fg(theme::ACCENT).bold(),
    ));
    row.gap(2);

    let subtabs = build_vm_subtab_bar(app);
    let origin = row.cursor();
    let mut btns = [(0u16, 0u16, 0u16); 5];
    for (sub, start, end) in subtabs.cells() {
        if let Some(i) = VmSubtab::ALL.iter().position(|s| *s == sub) {
            btns[i] = (area.y, origin + start, origin + end);
        }
    }
    app.tlb.subtab_btns.set(btns);
    for span in subtabs.spans() {
        row.push(span);
    }

    let active = translation_active(app);
    let chip_text = match (app.vm_mode(), active) {
        // No "(enable in overview)" here: telling the reader where to go is what
        // the help overlay is for, and the line has to stay scannable as state.
        (VmMode::Off, _) => "vm=off",
        (VmMode::Sv32, true) => "vm=sv32 · translating",
        (VmMode::Sv32, false) => "vm=sv32 · inactive (satp=Bare)",
        (VmMode::Custom, true) => "vm=custom · translating",
        (VmMode::Custom, false) => "vm=custom · inactive (satp=Bare)",
        (VmMode::Manual, true) => "vm=manual · translating",
        (VmMode::Manual, false) => "vm=manual · inactive (satp=Bare or priv=M)",
    };
    let chip_color = if active {
        theme::RUNNING
    } else if app.session.vm_enabled() {
        theme::ACCENT
    } else {
        theme::PAUSED
    };
    row.gap(3);
    row.push(sep());
    row.gap(3);
    row.push(Span::styled(chip_text, Style::default().fg(chip_color)));
    let line1 = row.into_line();

    // ── Line 2: transport · actions · metrics ──
    let y2 = area.y + 2;
    let mut row = SpanRow::new(area.x, y2);
    row.gap(1);

    let exec = build_vm_exec_bar(app);
    let origin = row.cursor();
    for (id, start, end) in exec.cells() {
        let cell = match id {
            RunButton::Speed => &app.tlb.exec_speed_btn,
            RunButton::State => &app.tlb.exec_state_btn,
            RunButton::Reset => &app.tlb.exec_reset_btn,
            _ => continue,
        };
        cell.set((y2, origin + start, origin + end));
    }
    for span in exec.spans() {
        row.push(span);
    }

    row.gap(3);
    row.push(sep());
    row.gap(3);
    let actions = build_vm_ctrl_bar(app);
    let origin = row.cursor();
    app.tlb.ctrl_import_btn.set((0, 0, 0));
    app.tlb.ctrl_export_btn.set((0, 0, 0));
    for (id, start, end) in actions.cells() {
        let cell = match id {
            TlbCtrlBtn::Results => &app.tlb.ctrl_results_btn,
            TlbCtrlBtn::ImportCfg => &app.tlb.ctrl_import_btn,
            TlbCtrlBtn::ExportCfg => &app.tlb.ctrl_export_btn,
            TlbCtrlBtn::FlushTlb => &app.tlb.ctrl_flush_btn,
        };
        cell.set((y2, origin + start, origin + end));
    }
    for span in actions.spans() {
        row.push(span);
    }

    let totals = app.execution_totals();
    let metrics = vec![
        style::metric_span("cyc ", totals.cycles, style::Metric::Cycles),
        Span::raw("   "),
        style::metric_span("CPI ", format!("{:.2}", totals.cpi()), style::Metric::Cpi),
        Span::raw("   "),
        Span::styled("instr", style::idle()),
        Span::styled(format!(" {}", totals.instructions), style::value()),
        Span::raw(" "),
    ];
    let metrics_w: u16 = metrics.iter().map(|s| s.width() as u16).sum();
    row.gap(
        (area.x + area.width)
            .saturating_sub(metrics_w)
            .saturating_sub(row.cursor())
            .max(3),
    );
    for span in metrics {
        row.push(span);
    }

    f.render_widget(
        Paragraph::new(vec![line1, Line::raw(""), row.into_line()]),
        area,
    );
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

/// Whether the machine is actually translating addresses right now.
///
/// The backend answers, rather than the panel re-deriving it from `satp`,
/// privilege and a force flag — that duplication is how the panel could
/// disagree with the machine it was describing.
pub(super) fn translation_active(app: &App) -> bool {
    app.translation()
        .is_some_and(|translation| translation.enabled())
}

fn btn_style(active: bool, hovered: bool) -> Style {
    if active || hovered {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD)
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
        assert!(
            app.cache.session_history[0].tlb.is_some(),
            "vm on ⇒ tlb snapshot"
        );
        app.cache.viewing_snapshot = Some(0);
        render_all_subtabs(&mut app, "vm on + snapshot");

        // Custom mode exposes the editable paging-scheme rows; TLB off shows
        // the disabled notice on the tlb subtab.
        app.set_vm_mode(crate::falcon::mmu::VmMode::Custom);
        app.set_tlb_enabled(false);
        render_all_subtabs(&mut app, "custom + tlb off");
    }
}
