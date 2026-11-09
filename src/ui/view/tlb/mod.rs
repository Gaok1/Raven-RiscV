// ui/view/tlb/ — the Virtual Memory tab.
//
// A two-line header, like the Cache tab's, over one of five subtabs:
//
//   walk    what happened to one access, step by step, and the recent history
//   tlb     the TLB as a sets × ways matrix
//   map     the page table, one column per level
//   stats   what translation cost
//   config  how it is set up
//
// Every clickable thing is recorded in `app.tlb.hits` with the rectangle it was
// drawn in, and the mouse handler only looks there — so a control can never be
// clickable anywhere but where it is drawn.

use ratatui::{Frame, prelude::*, widgets::Paragraph};
use raven_engine::capability::{
    AddressTranslation, TranslationOutcome, TranslationPermissions, TranslationRecordView, TranslationScheme,
};

use crate::falcon::mmu::VmMode;
use crate::ui::app::{App, RunButton, VmHit, VmSubtab};
use crate::ui::theme;
use crate::ui::view::components::{ControlState, SpanRow, Toolbar};
use crate::ui::view::style;

mod config;
mod map;
mod matrix;
mod stats;
mod walk;

/// Rows the header occupies: two rows of controls with a blank between them,
/// as in the Cache tab.
pub(crate) const VM_HEADER_H: u16 = 3;

pub(super) fn render_tlb_tab(f: &mut Frame, area: Rect, app: &App) {
    app.tlb.hits.borrow_mut().clear();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(VM_HEADER_H), Constraint::Min(0)])
        .split(area);

    render_header(f, layout[0], app);

    let body = layout[1];
    match app.tlb.vm_subtab {
        VmSubtab::Walk => walk::render(f, body, app),
        VmSubtab::Tlb => matrix::render(f, body, app),
        VmSubtab::Map => map::render(f, body, app),
        VmSubtab::Stats => stats::render(f, body, app),
        VmSubtab::Config => config::render(f, body, app),
    }

    // The session snapshots are shared with the Cache tab; viewing one from
    // the Stats subtab opens the same popup.
    if matches!(app.tlb.vm_subtab, VmSubtab::Stats) && app.cache.viewing_snapshot.is_some() {
        crate::ui::view::cache::render_snapshot_popup(f, area, app);
    }
}

// ── Shared helpers ───────────────────────────────────────────────────────────

/// Lay `bar` out at `(x, y)`, record each control as a hit, and return its
/// spans. `to_hit` names what a cell does.
pub(super) fn place<Id: Copy>(
    app: &App,
    bar: &Toolbar<Id>,
    x: u16,
    y: u16,
    to_hit: impl Fn(Id) -> VmHit,
) -> Vec<Span<'static>> {
    for (id, start, end) in bar.cells() {
        app.tlb.hit(to_hit(id), Rect::new(x + start, y, end - start, 1));
    }
    bar.spans()
}

/// A chip's state: lit when `active`, bright under the pointer.
pub(super) fn chip(app: &App, hit: VmHit, active: bool) -> ControlState {
    ControlState::chip(active, app.tlb.is_hovered(hit))
}

/// `0x` and a fixed number of hex digits.
pub(super) fn hex(value: u64, digits: usize) -> String {
    format!("0x{value:0digits$x}")
}

/// `4 KiB`, `4 MiB`.
pub(super) fn size_long(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{} GiB", bytes >> 30)
    } else if bytes >= 1 << 20 {
        format!("{} MiB", bytes >> 20)
    } else if bytes >= 1 << 10 {
        format!("{} KiB", bytes >> 10)
    } else {
        format!("{bytes} B")
    }
}

/// `4K`, `4M` — for a column.
pub(super) fn size_short(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{}G", bytes >> 30)
    } else if bytes >= 1 << 20 {
        format!("{}M", bytes >> 20)
    } else if bytes >= 1 << 10 {
        format!("{}K", bytes >> 10)
    } else {
        format!("{bytes}B")
    }
}

/// `RW-U`: one column per permission, a dash where it is missing.
pub(super) fn perms_text(p: TranslationPermissions) -> String {
    [(p.read, 'R'), (p.write, 'W'), (p.execute, 'X'), (p.user, 'U')]
        .iter()
        .map(|&(on, c)| if on { c } else { '-' })
        .collect()
}

/// The word and colour every view uses for an outcome.
pub(super) fn outcome_badge(outcome: TranslationOutcome) -> (&'static str, Color) {
    match outcome {
        TranslationOutcome::TlbHit => ("hit", theme::RUNNING),
        TranslationOutcome::Walked => ("miss", theme::PAUSED),
        TranslationOutcome::Fault => ("fault", theme::DANGER),
        TranslationOutcome::Identity => ("—", theme::IDLE),
    }
}

/// Hex digits of a virtual page number.
pub(super) fn vpn_digits(scheme: &TranslationScheme) -> usize {
    let index_bits: u32 = scheme
        .levels
        .iter()
        .map(|l| u32::from(l.index_bits))
        .sum::<u32>()
        .max(1);
    (index_bits as usize).div_ceil(4)
}

/// The colour of walk level `level` (0 is the root), shared by the split
/// address and every step that indexes a table with that field.
pub(super) fn level_color(level: usize) -> Color {
    match level {
        0 => theme::VA_VPN1,
        _ => theme::VA_VPN0,
    }
}

/// Why nothing is being translated, and the one action that fixes it here.
pub(super) fn idle_notice(app: &App) -> Option<(String, Option<(VmHit, &'static str)>)> {
    let translation = app.translation()?;
    if translation.enabled() {
        return None;
    }
    match app.vm_mode() {
        VmMode::Off => Some((
            "virtual memory is off: every address is physical".into(),
            Some((VmHit::TurnOn, "turn on")),
        )),
        VmMode::Auto => Some((
            translation
                .idle_reason()
                .unwrap_or_else(|| "not translating".into()),
            Some((VmHit::InstallMap, "install map")),
        )),
        VmMode::Program => Some((
            format!(
                "{} — the program writes satp and drops to S/U itself",
                translation
                    .idle_reason()
                    .unwrap_or_else(|| "not translating".into())
            ),
            None,
        )),
    }
}

/// A view with nothing to show yet: one sentence, and the action that would
/// change that when there is one.
pub(super) fn render_empty(f: &mut Frame, inner: Rect, app: &App, fallback: &str) {
    if inner.height == 0 {
        return;
    }
    if app.vm_mode() == VmMode::Off && inner.height >= INTRO_MIN_H && inner.width >= 50 {
        render_intro(f, inner, app);
        return;
    }
    let (text, action) = match idle_notice(app) {
        Some((reason, action)) => (format!("Not translating: {reason}."), action),
        None => (fallback.to_string(), None),
    };
    let y = inner.y + inner.height.saturating_sub(2) / 2;
    let mut lines = vec![Line::from(Span::styled(format!(" {text}"), style::label()))];
    if let Some((hit, label)) = action {
        let mut bar = Toolbar::new();
        bar.action(hit, label, chip(app, hit, false), theme::ACCENT);
        let mut spans = vec![Span::raw(" ")];
        spans.extend(place(app, &bar, inner.x + 1, y + 2, |id| id));
        lines.push(Line::raw(""));
        lines.push(Line::from(spans));
    }
    let height = (lines.len() as u16).min(inner.height.saturating_sub(y - inner.y));
    f.render_widget(
        Paragraph::new(lines),
        Rect::new(inner.x, y, inner.width, height),
    );
}

/// Rows the welcome needs without its list of subtabs; a shorter panel says
/// it in one line.
const INTRO_MIN_H: u16 = 11;

/// What the tab shows before virtual memory is on: what it is for, the button
/// that starts it, what each subtab holds, and where the guide is. Acronyms
/// are spelled out, since this is the first thing a newcomer reads.
fn render_intro(f: &mut Frame, inner: Rect, app: &App) {
    let width = usize::from(inner.width.saturating_sub(2)).min(96);
    let mut lines = Vec::new();
    if inner.height >= INTRO_MIN_H + 2 {
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(Span::styled(
        " Virtual memory is off: every address goes straight to memory.",
        Style::default().fg(theme::TEXT).bold(),
    )));
    lines.push(Line::raw(""));
    lines.extend(crate::ui::view::guide::paragraph(
        "Turn it on to watch the **MMU** (Memory Management Unit) translate each **VA** (virtual address) the program uses into a **PA** (physical address): first through the **TLB** (Translation Lookaside Buffer), a cache of recent translations, and on a miss through the page table in memory.",
        width,
        1,
    ));
    lines.push(Line::raw(""));

    // The button, placed where its line lands so a click on it registers.
    let button_y = inner.y + lines.len() as u16;
    let mut bar = Toolbar::new();
    bar.action(VmHit::TurnOn, "turn on", chip(app, VmHit::TurnOn, false), theme::ACCENT);
    let mut row = vec![Span::raw(" ")];
    row.extend(place(app, &bar, inner.x + 1, button_y, |id| id));
    row.push(Span::styled("   then ", style::label()));
    row.push(Span::styled("s", style::key()));
    row.push(Span::styled(" steps to the next translation, ", style::label()));
    row.push(Span::styled("p", style::key()));
    row.push(Span::styled(" runs", style::label()));
    lines.push(Line::from(row));
    lines.push(Line::raw(""));

    // The list of subtabs goes first when the panel is short: the header
    // already names them, and the guide line below says where to read more.
    let subtabs_fit = lines.len() + 5 + 2 + 1 <= usize::from(inner.height);
    for (name, what) in [
        ("walk", "one translation, step by step, and the ones before it"),
        ("tlb", "the TLB, set by set and way by way"),
        ("map", "the page table the translations are read from"),
        ("stats", "hit rate, what the misses cost, and why they happened"),
        ("config", "the mode, the TLB's shape, what the pages allow"),
    ]
    .into_iter()
    .filter(|_| subtabs_fit)
    {
        lines.push(Line::from(vec![
            Span::styled(format!("   {name:<8}"), Style::default().fg(theme::ACCENT).bold()),
            Span::styled(what, style::label()),
        ]));
    }
    if subtabs_fit {
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled("?", style::key()),
        Span::styled(
            " opens the guide: how translation works, what every panel shows, every key.",
            style::label(),
        ),
    ]));
    f.render_widget(Paragraph::new(lines), inner);
}

/// `title`, naming the core it belongs to when each core keeps a TLB of its
/// own — otherwise two cores' panels would look alike.
pub(super) fn core_title(app: &App, title: &str) -> String {
    match app.translation() {
        Some(t) if t.tlb_count() > 1 => format!("{title} · core {}", t.core),
        _ => title.to_string(),
    }
}

/// The newest record first, filtered — what the history lists.
pub(super) fn history<'a>(
    app: &App,
    records: &'a [TranslationRecordView],
) -> Vec<&'a TranslationRecordView> {
    records
        .iter()
        .rev()
        .filter(|r| app.tlb.filter.admits(r))
        .collect()
}

// ── Header ───────────────────────────────────────────────────────────────────

/// ```text
///  Virtual Memory   walk  tlb  map  stats  config   │   Sv32 · auto · translating
///  speed 1x   state pause   step  back   reset   │   flush tlb   results      cyc 0  CPI 0.00  instr 0
/// ```
fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let sep = || Span::styled("│", Style::default().fg(theme::BORDER));

    // ── Line 1: identity · subtabs · translation state ──
    let mut row = SpanRow::new(area.x, area.y);
    row.push(Span::styled(
        " Virtual Memory ",
        Style::default().fg(theme::ACCENT).bold(),
    ));
    row.gap(2);
    let mut tabs = Toolbar::new();
    for sub in VmSubtab::ALL {
        let hit = VmHit::Subtab(sub);
        tabs.value(
            sub,
            sub.label(),
            chip(app, hit, app.tlb.vm_subtab == sub),
            theme::ACCENT,
        );
    }
    for span in place(app, &tabs, row.cursor(), area.y, VmHit::Subtab) {
        row.push(span);
    }
    row.gap(3);
    row.push(sep());
    row.gap(3);
    match idle_notice(app) {
        None => {
            let owner = match app.vm_mode() {
                VmMode::Program => "program",
                _ => "auto",
            };
            let scheme = app
                .translation()
                .map(|t| t.scheme().name)
                .unwrap_or_default();
            let room = (area.x + area.width).saturating_sub(row.cursor());
            let word = if room >= 30 { "translating" } else { "on" };
            row.push(Span::styled(
                format!("{scheme} · {owner} · {word}"),
                Style::default().fg(theme::RUNNING),
            ));
        }
        Some((reason, action)) => {
            let color = if app.vm_mode() == VmMode::Off {
                theme::IDLE
            } else {
                theme::PAUSED
            };
            let mut bar = Toolbar::new();
            if let Some((hit, label)) = action {
                bar.action(hit, label, chip(app, hit, false), theme::ACCENT);
            }
            // The reason in full when it fits beside its action; the subtabs'
            // own empty states say it in full when it does not.
            let room = (area.x + area.width).saturating_sub(row.cursor());
            let needed = reason.chars().count() as u16 + 2 + bar.width();
            let text = if needed <= room {
                reason
            } else if app.vm_mode() == VmMode::Off {
                "off".to_string()
            } else {
                "not translating".to_string()
            };
            row.push(Span::styled(text, Style::default().fg(color)));
            if action.is_some() {
                row.gap(2);
                for span in place(app, &bar, row.cursor(), area.y, |id| id) {
                    row.push(span);
                }
            }
        }
    }
    let line1 = row.into_line();

    // ── Line 2: transport · actions · metrics ──
    let y2 = area.y + 2;
    // A narrow screen tightens the gaps and leaves step-back to its key (b),
    // so the controls still fit on one line.
    let narrow = area.width < 110;
    let spacing = if narrow { 1 } else { 3 };
    let mut row = SpanRow::new(area.x, y2);
    row.gap(1);
    let (state_text, state_color) = if app.session.is_running {
        ("run", theme::RUNNING)
    } else {
        ("pause", theme::PAUSED)
    };
    let mut exec = Toolbar::with_gap(if narrow { 1 } else { 3 });
    exec.toggle(
        RunButton::Speed,
        "speed",
        app.session.speed.label(),
        chip(app, VmHit::Run(RunButton::Speed), true),
        theme::TEXT,
    )
    .toggle(
        RunButton::State,
        "state",
        state_text,
        chip(app, VmHit::Run(RunButton::State), true),
        state_color,
    )
    .action(
        RunButton::Step,
        "step",
        chip(app, VmHit::Run(RunButton::Step), false),
        theme::ACCENT,
    );
    if !narrow {
        exec.action(
            RunButton::Stepback,
            "back",
            chip(app, VmHit::Run(RunButton::Stepback), false),
            theme::TEXT,
        );
    }
    exec.action(
        RunButton::Reset,
        "reset",
        chip(app, VmHit::Run(RunButton::Reset), false),
        theme::DANGER,
    );
    for span in place(app, &exec, row.cursor(), y2, VmHit::Run) {
        row.push(span);
    }
    row.gap(spacing);
    row.push(sep());
    row.gap(spacing);

    let mut actions = Toolbar::with_gap(if narrow { 1 } else { 3 });
    actions.action(
        VmHit::FlushTlb,
        "flush tlb",
        chip(app, VmHit::FlushTlb, false),
        theme::DANGER,
    );
    actions.action(
        VmHit::Results,
        "results",
        chip(app, VmHit::Results, false),
        theme::ACCENT,
    );
    if matches!(app.tlb.vm_subtab, VmSubtab::Config) {
        actions
            .action(
                VmHit::ImportCfg,
                "import cfg",
                chip(app, VmHit::ImportCfg, false),
                theme::METRIC_CYC,
            )
            .action(
                VmHit::ExportCfg,
                "export cfg",
                chip(app, VmHit::ExportCfg, false),
                theme::METRIC_CYC,
            );
    }
    for span in place(app, &actions, row.cursor(), y2, |id| id) {
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
    // Metrics only when they fit whole; a clipped `CPI` reads as a glitch.
    let metrics_w: u16 = metrics.iter().map(|s| s.width() as u16).sum();
    let room = (area.x + area.width).saturating_sub(row.cursor());
    if room >= metrics_w + 3 {
        row.gap(room - metrics_w);
        for span in metrics {
            row.push(span);
        }
    }

    f.render_widget(
        Paragraph::new(vec![line1, Line::raw(""), row.into_line()]),
        area,
    );
}

// One spelling for a policy, wherever it is shown: the Cache Config pane owns
// it, and the VM config must not drift onto its own wording.
pub(super) use crate::ui::view::cache::config::replacement_label;

#[cfg(test)]
mod tests {
    use crate::ui::app::{Tab, VmSubtab};
    use crossterm::event::{KeyCode, KeyEvent};
    use ratatui::{Terminal, backend::TestBackend};

    fn render_all_subtabs(app: &mut crate::ui::app::App, label: &str) {
        for sub in VmSubtab::ALL {
            app.tlb.vm_subtab = sub;
            for (w, h) in [(160u16, 48u16), (120, 40), (80, 24), (60, 16), (30, 8)] {
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

        // VM on (auto map) + a real session snapshot captured through the
        // keyboard path + the popup over Stats.
        app.set_vm_mode(crate::falcon::mmu::VmMode::Auto);
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

        // Program mode, TLB off.
        app.set_vm_mode(crate::falcon::mmu::VmMode::Program);
        app.set_tlb_enabled(false);
        render_all_subtabs(&mut app, "program + tlb off");
    }

    fn draw(app: &crate::ui::app::App, w: u16, h: u16) {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).expect("terminal");
        terminal
            .draw(|f| crate::ui::view::ui(f, app))
            .expect("draw");
    }

    fn key(app: &mut crate::ui::app::App, code: KeyCode) {
        crate::ui::input::handle_key(app, KeyEvent::from(code)).expect("key");
    }

    /// `?` opens the guide on this tab — not the tour — and the guide
    /// scrolls, jumps between sections, switches language in place, hands
    /// over to the tour and closes.
    #[test]
    fn question_mark_opens_a_guide_that_scrolls_and_hands_over_to_the_tour() {
        let mut app = crate::ui::app::App::new(None);
        app.tab = Tab::Tlb;
        app.mode = crate::ui::app::EditorMode::Command;

        key(&mut app, KeyCode::Char('?'));
        assert!(app.guide.open && !app.tutorial.active);
        draw(&app, 120, 40);
        assert!(app.guide.lines.get() > app.guide.viewport.get(), "long enough to scroll");

        key(&mut app, KeyCode::Down);
        assert_eq!(app.guide.scroll, 1);
        key(&mut app, KeyCode::Right);
        assert_eq!(app.guide.section(), 1);
        draw(&app, 120, 40);

        // The other language keeps the reader on the same section.
        key(&mut app, KeyCode::Char('l'));
        draw(&app, 120, 40);
        assert_eq!(app.guide.section(), 1);

        // Every size renders, the contents list included or not.
        for (w, h) in [(160u16, 48u16), (80, 24), (40, 12)] {
            draw(&app, w, h);
        }

        key(&mut app, KeyCode::Char('t'));
        assert!(!app.guide.open && app.tutorial.active);
        key(&mut app, KeyCode::Esc);
        key(&mut app, KeyCode::Char('?'));
        key(&mut app, KeyCode::Esc);
        assert!(!app.guide.open);
    }

    /// `s` on the VM tab runs to the next translation and shows it.
    #[test]
    fn s_steps_and_follows_the_new_translation() {
        let mut app = crate::ui::app::App::new(None);
        app.tab = Tab::Tlb;
        app.mode = crate::ui::app::EditorMode::Command;
        app.set_vm_mode(crate::falcon::mmu::VmMode::Auto);
        app.tlb.pinned = Some(raven_engine::capability::TranslationRecordView {
            seq: 99,
            hart: 0,
            pc: 0,
            access: raven_engine::capability::TranslationAccess::Load,
            privilege: "M",
            virtual_address: 0,
            address_space: None,
            probed_sets: Vec::new(),
            hit_entry: None,
            rewalk_for_dirty: false,
            steps: Vec::new(),
            outcome: raven_engine::capability::TranslationOutcome::Identity,
            fault: None,
            fault_code: None,
            physical: Some(0),
            page_size: 4096,
            cycles: 0,
            installed_entry: None,
            evicted: None,
            set_accessed: false,
            set_dirty: false,
        });

        key(&mut app, KeyCode::Char('s'));
        assert!(app.tlb.pinned.is_none(), "a step follows what it caused");
        assert!(!app.vm_records().is_empty(), "the step translated its fetch");

        // Each press shows one more translation, even across the cycles a
        // miss stalls the pipeline for (the sample program halts soon after).
        for _ in 0..2 {
            let before = app.vm_records().last().map(|rec| rec.seq);
            key(&mut app, KeyCode::Char('s'));
            assert_ne!(app.vm_records().last().map(|rec| rec.seq), before);
        }
    }
}
