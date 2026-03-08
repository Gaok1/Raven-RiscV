use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, Gauge, Paragraph},
};
use std::time::Instant;
use crate::ui::theme;

const CHIP: &[&str] = &[
    "              ┌─┬─┬─┬─┬─┬─┬─┬─┐              ",
    "  VCC  ───────┤                 ├─────── GND   ",
    "  CLK  ───────┤  ╔═══════════╗  ├─────── RST   ",
    "   PC  ───────┤  ║  R I S C  ║  ├─────── IRQ   ",
    "   SP  ───────┤  ║  ─────── ─║  ├─────── A0    ",
    "   RA  ───────┤  ║  R V 3 2  ║  ├─────── A1    ",
    "   T0  ───────┤  ║  I · M · F║  ├─────── D0    ",
    "   T1  ───────┤  ║           ║  ├─────── D1    ",
    "   A2  ───────┤  ║  R A V E N║  ├─────── MEM   ",
    "  ALU  ───────┤  ╚═══════════╝  ├─────── WB    ",
    "  FETCH───────┤                 ├─────── EX    ",
    "              └─┴─┴─┴─┴─┴─┴─┴─┘              ",
];

const SUBTITLE: &str = "RISC-V Simulator & IDE  ·  RV32IMF";

pub fn render_splash(f: &mut Frame, started: Instant, duration_secs: f64) {
    let area = f.area();

    f.render_widget(
        Block::default().style(Style::default().bg(theme::BG)),
        area,
    );

    let chip_h = CHIP.len() as u16;
    let chip_w = CHIP[0].len() as u16;
    let total_h = chip_h + 5; // chip + blank + subtitle + blank + bar

    // Center vertically and horizontally
    let y = area.height.saturating_sub(total_h) / 2;
    let x = area.width.saturating_sub(chip_w) / 2;

    // Draw chip lines
    for (i, line) in CHIP.iter().enumerate() {
        let row = y + i as u16;
        if row >= area.height { break; }

        let spans: Vec<Span> = line.chars().map(|c| {
            let color = match c {
                '╔' | '╗' | '╚' | '╝' | '║' | '═' => theme::ACCENT,
                '┌' | '┐' | '└' | '┘' | '─' | '│' | '┬' | '┴' | '┤' | '├' => theme::BORDER_HOV,
                _ if c.is_ascii_uppercase() && !matches!(c, 'G'|'V'|'C'|'R'|'I'|'P'|'S'|'A'|'T'|'M'|'F'|'E') => theme::TEXT,
                _ => theme::TEXT,
            };
            let color = if matches!(c, '╔'|'╗'|'╚'|'╝'|'║'|'═') {
                theme::ACCENT
            } else if matches!(c, '┌'|'┐'|'└'|'┘'|'─'|'│'|'┬'|'┴'|'┤'|'├') {
                theme::BORDER_HOV
            } else if c == '·' {
                theme::IDLE
            } else {
                theme::TEXT
            };
            Span::styled(c.to_string(), Style::default().fg(color))
        }).collect();

        let para = Paragraph::new(Line::from(spans));
        let rect = Rect::new(x, row, chip_w.min(area.width.saturating_sub(x)), 1);
        f.render_widget(para, rect);
    }

    // Subtitle
    let sub_y = y + chip_h + 1;
    if sub_y < area.height {
        let sub_x = area.width.saturating_sub(SUBTITLE.len() as u16) / 2;
        let para = Paragraph::new(Span::styled(SUBTITLE, Style::default().fg(theme::IDLE)));
        let rect = Rect::new(sub_x, sub_y, SUBTITLE.len() as u16, 1);
        f.render_widget(para, rect);
    }

    // Progress bar
    let elapsed = started.elapsed().as_secs_f64();
    let progress = (elapsed / duration_secs).clamp(0.0, 1.0);
    let pct = (progress * 100.0) as u16;

    let bar_label = match pct {
        0..=19   => "  Initializing registers...",
        20..=39  => "  Loading instruction set...",
        40..=59  => "  Mapping memory regions...",
        60..=79  => "  Wiring cache hierarchy...",
        80..=99  => "  Booting chip...",
        _        => "  Ready.",
    };

    let bar_w = chip_w.min(area.width.saturating_sub(x));
    let bar_y = sub_y + 2;
    if bar_y < area.height {
        let gauge = Gauge::default()
            .block(Block::default())
            .gauge_style(Style::default().fg(theme::ACCENT).bg(theme::BG_PANEL))
            .label(Span::styled(bar_label, Style::default().fg(theme::TEXT)))
            .percent(pct);
        let rect = Rect::new(x, bar_y, bar_w, 1);
        f.render_widget(gauge, rect);
    }
}
