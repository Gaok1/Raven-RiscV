// ui/view/tlb/status.rs — Live Sv32 / privilege / VM-active surface.
//
// Exists specifically to explain why the TLB looks idle after toggling
// Virtual Memory on: translation only runs when satp.mode=Sv32 and the hart
// is not in M-mode (see falcon/mmu/mod.rs:80-83). Without this view the
// Settings toggle appears to do nothing.

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Paragraph, Wrap},
};

use crate::falcon::mmu::{PrivMode, SatpMode};
use crate::ui::app::App;
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::style;

pub(super) fn render_status(f: &mut Frame, area: Rect, app: &App) {
    let inner = render_panel(
        f,
        area,
        panel::panel("Virtual Memory State", PanelKind::Plain),
    );
    if inner.height == 0 {
        return;
    }

    let mmu = app.run.mem.mmu();
    let satp_mode = mmu.satp.mode();
    let priv_mode = mmu.priv_mode;
    // Mirror Mmu::translate (falcon/mmu/mod.rs:97-104): M-mode bypass is
    // skipped when force_translate is on (didactic standard mode), so the
    // panel must consult it too or it will lie after Assemble.
    let active = app.run.vm_enabled()
        && satp_mode == SatpMode::Sv32
        && (priv_mode != PrivMode::M || mmu.force_translate);

    let vm_color = if app.run.vm_enabled() {
        theme::RUNNING
    } else {
        theme::PAUSED
    };
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

    let mut lines: Vec<Line<'static>> = vec![
        kv(
            " VM mode (Settings):           ",
            app.vm_mode().as_str().to_string(),
            vm_color,
        ),
        kv(
            " satp.mode:                    ",
            satp_mode_label.to_string(),
            satp_color,
        ),
        kv(
            " satp.asid:                    ",
            format!("{}", mmu.satp.asid()),
            theme::TEXT,
        ),
        kv(
            " satp.ppn (root PT @):         ",
            format!("0x{:08x}", root_pt),
            theme::TEXT,
        ),
        kv(
            " Privilege mode:               ",
            priv_label.to_string(),
            priv_color,
        ),
        Line::raw(""),
        kv(
            " Translation active?           ",
            if active { "YES" } else { "no" }.to_string(),
            active_color,
        ),
    ];

    if !app.run.vm_enabled() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " VM is disabled — every access is identity-mapped.",
            style::label(),
        )));
        lines.push(Line::from(Span::styled(
            " Toggle \"Virtual Memory\" in the Settings tab to enable the MMU.",
            style::label(),
        )));
    } else if !active {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " VM toggle is ON but no translation is happening yet.",
            style::danger().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::raw(""));
        if priv_mode == PrivMode::M && !mmu.force_translate {
            lines.push(Line::from(Span::styled(
                "  • Privilege is M — machine mode bypasses translation by spec.",
                style::label(),
            )));
        }
        if satp_mode == SatpMode::Bare {
            lines.push(Line::from(Span::styled(
                "  • satp.mode = Bare — no root page table is installed.",
                style::label(),
            )));
        }
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " Translation only starts when a program writes satp to Sv32",
            style::label(),
        )));
        lines.push(Line::from(Span::styled(
            " (csrw satp, <ppn|mode>) and switches privilege to S/U (mret/sret).",
            style::label(),
        )));
        lines.push(Line::from(Span::styled(
            " Until then the TLB counters on Stats stay at zero.",
            style::label(),
        )));
    } else {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " Every fetch/load/store is being translated through the TLB.",
            style::success(),
        )));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn kv(label: &'static str, value: String, value_color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(label, style::label()),
        Span::styled(
            value,
            Style::default()
                .fg(value_color)
                .add_modifier(Modifier::BOLD),
        ),
    ])
}
