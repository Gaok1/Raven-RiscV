// ui/view/tlb/overview.rs — the Virtual Memory landing subtab.
//
// Two jobs: give a beginner one-click controls (VM mode + TLB toggle) so the
// MMU can be enabled without touching the Settings panel, and explain why the
// TLB looks idle after toggling Virtual Memory on: translation only runs when
// satp.mode=Sv32 and the hart is not in M-mode (see falcon/mmu/mod.rs).
// Without this view the mode toggle appears to do nothing.

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Paragraph, Wrap},
};

use crate::falcon::mmu::{PrivMode, SatpMode};
use crate::ui::app::{App, TlbHoverTarget};
use crate::ui::theme;
use crate::ui::view::components::kv_styled;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::components::{ControlState, Toolbar};
use crate::ui::view::style;

/// The overview's two quick controls — VM mode and the TLB toggle.
#[derive(Clone, Copy, PartialEq, Eq)]
enum QuickBtn {
    Mode,
    Tlb,
}

pub(super) fn render_overview(f: &mut Frame, area: Rect, app: &App) {
    // The tab is already named Virtual Memory, so the panel says what it shows
    // rather than repeating where it lives.
    let inner = render_panel(f, area, panel::panel("Overview", PanelKind::Plain));
    if inner.height == 0 {
        return;
    }

    let Some(mmu) = app.rv32().map(|rv32| rv32.mem().mmu()) else {
        return;
    };
    let satp_mode = mmu.satp.mode();
    let priv_mode = mmu.priv_mode;
    let active = super::translation_active(app);

    // ── Quick controls (row 0) ───────────────────────────────────────────────
    //
    // Through a `Toolbar`, because the columns were previously counted by hand
    // as `x += mode_label.len()` while the renderer drew `dense_value`, which
    // pads a column either side: the mode box came out two columns short and
    // the TLB box beside it landed two columns to the left of the pill it was
    // supposed to cover.
    let mut bar = Toolbar::new();
    bar.toggle(
        QuickBtn::Mode,
        "mode",
        app.vm_mode().as_str(),
        ControlState::chip(true, matches!(app.tlb.hover, Some(TlbHoverTarget::QuickMode))),
        theme::TEXT,
    )
    .toggle(
        QuickBtn::Tlb,
        "tlb",
        if app.session.tlb_enabled { "on" } else { "off" },
        ControlState::chip(
            app.session.tlb_enabled,
            matches!(app.tlb.hover, Some(TlbHoverTarget::QuickTlb)),
        ),
        theme::RUNNING,
    );

    let origin = inner.x + 1;
    for (id, start, end) in bar.cells() {
        let cell = match id {
            QuickBtn::Mode => &app.tlb.quick_mode_btn,
            QuickBtn::Tlb => &app.tlb.quick_tlb_btn,
        };
        cell.set((inner.y, origin + start, origin + end));
    }

    // No "(click to change — …)" trailing the row: the controls wear a filled
    // surface now, and that *is* the affordance. Which mode to pick is what the
    // help overlay is for.
    let mut quick_spans = vec![Span::raw(" ")];
    quick_spans.extend(bar.spans());
    let quick_line = Line::from(quick_spans);

    // ── Live state ───────────────────────────────────────────────────────────
    let satp_color = match satp_mode {
        SatpMode::Sv32 => theme::RUNNING,
        SatpMode::Bare => theme::PAUSED,
    };
    let priv_color = match priv_mode {
        PrivMode::M => theme::LABEL_Y,
        PrivMode::S | PrivMode::U => theme::RUNNING,
    };
    let active_color = if active {
        theme::RUNNING
    } else {
        theme::DANGER
    };

    let satp_mode_label = match satp_mode {
        SatpMode::Bare => "Bare (translation off)",
        SatpMode::Sv32 => "Sv32",
    };
    let priv_label = match priv_mode {
        PrivMode::M => "M (machine — bypasses translation)",
        PrivMode::S => "S (supervisor)",
        PrivMode::U => "U (user)",
    };
    let root_pt = (mmu.satp.ppn() as u64) << 12;

    // Key/value readout via the toolkit's `kv_styled` (it owns the line and the
    // key–value separator; we own the span styling).
    //
    // The CSR field names are kept verbatim — `satp.mode` is what the spec calls
    // it, the same way `imm[11:0]` stays as written. The rest lose their colons
    // and Title Case: a padded label followed by a lit value is the one shape
    // this app uses for a readout, and the trailing `:` only made the column
    // ragged where the label ran long.
    const KEY_W: usize = 24;
    let key = |s: &str| Span::styled(format!(" {s:<KEY_W$}"), style::label());
    let val =
        |s: String, c: Color| Span::styled(s, Style::default().fg(c).add_modifier(Modifier::BOLD));

    let mut lines: Vec<Line<'static>> = vec![quick_line, Line::raw("")];
    lines.extend(kv_styled(vec![
        (
            key("satp.mode"),
            val(satp_mode_label.to_string(), satp_color),
        ),
        (
            key("satp.asid"),
            val(format!("{}", mmu.satp.asid()), theme::TEXT),
        ),
        (
            key("satp.ppn — root table"),
            val(format!("0x{root_pt:08x}"), theme::TEXT),
        ),
        (key("privilege"), val(priv_label.to_string(), priv_color)),
    ]));
    lines.push(Line::raw(""));
    lines.extend(kv_styled(vec![(
        key("translating"),
        val(if active { "yes" } else { "no" }.to_string(), active_color),
    )]));

    if !app.session.vm_enabled() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " VM is off — every access is identity-mapped.",
            Style::default().fg(theme::LABEL),
        )));
        lines.push(Line::from(Span::styled(
            " Set mode to sv32 above, then Assemble: the simulator installs",
            Style::default().fg(theme::LABEL),
        )));
        lines.push(Line::from(Span::styled(
            " a page map for you and every access goes through the MMU.",
            Style::default().fg(theme::LABEL),
        )));
    } else if !active {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " VM is ON but no translation is happening yet.",
            Style::default()
                .fg(theme::DANGER)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::raw(""));
        if priv_mode == PrivMode::M && !mmu.force_translate {
            lines.push(Line::from(Span::styled(
                "  • Privilege is M — machine mode bypasses translation by spec.",
                Style::default().fg(theme::LABEL),
            )));
        }
        if satp_mode == SatpMode::Bare {
            lines.push(Line::from(Span::styled(
                "  • satp.mode = Bare — no root page table is installed.",
                Style::default().fg(theme::LABEL),
            )));
        }
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " Translation only starts when a program writes satp to Sv32",
            Style::default().fg(theme::LABEL),
        )));
        lines.push(Line::from(Span::styled(
            " (csrw satp, <ppn|mode>) and switches privilege to S/U (mret/sret).",
            Style::default().fg(theme::LABEL),
        )));
        lines.push(Line::from(Span::styled(
            " Until then the TLB counters on stats stay at zero.",
            Style::default().fg(theme::LABEL),
        )));
    } else {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " Every fetch/load/store is being translated through the TLB.",
            Style::default().fg(theme::RUNNING),
        )));
        lines.push(Line::from(Span::styled(
            " Watch it work: map shows the page table, tlb the cached translations,",
            Style::default().fg(theme::LABEL),
        )));
        lines.push(Line::from(Span::styled(
            " stats the hit/miss counters (press s there to capture a snapshot).",
            Style::default().fg(theme::LABEL),
        )));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
