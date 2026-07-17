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
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};

use crate::falcon::mmu::{PrivMode, SatpMode};
use crate::ui::app::{App, TlbHoverTarget};
use crate::ui::theme;
use crate::ui::view::components::dense_value;
use crate::ui::view::components::kv_styled;
use crate::ui::view::style;

pub(super) fn render_overview(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            "Virtual Memory Overview",
            Style::default().fg(theme::LABEL),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let mmu = app.run.mem().mmu();
    let satp_mode = mmu.satp.mode();
    let priv_mode = mmu.priv_mode;
    // Mirror Mmu::translate: M-mode bypass is skipped when force_translate is
    // on (didactic auto modes), so the panel must consult it too or it will
    // lie after Assemble.
    let active = app.run.vm_enabled()
        && satp_mode == SatpMode::Sv32
        && (priv_mode != PrivMode::M || mmu.force_translate);

    // ── Quick controls (row 0) ───────────────────────────────────────────────
    let mode_label = format!("< {} >", app.vm_mode().as_str());
    let tlb_label = if app.run.tlb_enabled { "[on]" } else { "[off]" };
    let row_y = inner.y;
    let mut x = inner.x + 1;
    x += "Mode ".len() as u16;
    app.tlb
        .quick_mode_btn
        .set((row_y, x, x + mode_label.len() as u16));
    x += mode_label.len() as u16;
    x += "     TLB ".len() as u16;
    app.tlb
        .quick_tlb_btn
        .set((row_y, x, x + tlb_label.len() as u16));

    let quick_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("Mode ", Style::default().fg(theme::LABEL)),
        dense_value(
            &mode_label,
            matches!(app.tlb.hover, Some(TlbHoverTarget::QuickMode)),
            true,
            theme::TEXT,
        ),
        Span::raw("     "),
        Span::styled("TLB ", Style::default().fg(theme::LABEL)),
        dense_value(
            tlb_label,
            matches!(app.tlb.hover, Some(TlbHoverTarget::QuickTlb)),
            app.run.tlb_enabled,
            theme::RUNNING,
        ),
        Span::styled(
            "     (click to change — sv32 is the easy didactic default)",
            Style::default().fg(theme::IDLE),
        ),
    ]);

    // ── Live state ───────────────────────────────────────────────────────────
    let satp_color = match satp_mode {
        SatpMode::Sv32 => theme::RUNNING,
        SatpMode::Bare => theme::PAUSED,
    };
    let priv_color = match priv_mode {
        PrivMode::M => theme::LABEL_Y,
        PrivMode::S | PrivMode::U => theme::RUNNING,
    };
    let active_color = if active { theme::RUNNING } else { theme::DANGER };

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
    let key = |s: &'static str| Span::styled(s, style::label());
    let val = |s: String, c: Color| {
        Span::styled(s, Style::default().fg(c).add_modifier(Modifier::BOLD))
    };

    let mut lines: Vec<Line<'static>> = vec![quick_line, Line::raw("")];
    lines.extend(kv_styled(vec![
        (
            key(" satp.mode:                   "),
            val(satp_mode_label.to_string(), satp_color),
        ),
        (
            key(" satp.asid:                   "),
            val(format!("{}", mmu.satp.asid()), theme::TEXT),
        ),
        (
            key(" satp.ppn (root PT @):        "),
            val(format!("0x{:08x}", root_pt), theme::TEXT),
        ),
        (
            key(" Privilege mode:              "),
            val(priv_label.to_string(), priv_color),
        ),
    ]));
    lines.push(Line::raw(""));
    lines.extend(kv_styled(vec![(
        key(" Translation active?          "),
        val(if active { "YES" } else { "no" }.to_string(), active_color),
    )]));

    if !app.run.vm_enabled() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " VM is off — every access is identity-mapped.",
            Style::default().fg(theme::LABEL),
        )));
        lines.push(Line::from(Span::styled(
            " Click Mode above and pick sv32, then Assemble: the simulator installs",
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
            Style::default().fg(theme::DANGER).add_modifier(Modifier::BOLD),
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

    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        inner,
    );
}
