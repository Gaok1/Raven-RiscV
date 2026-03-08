use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, Gauge, Paragraph},
};
use std::time::Instant;
use crate::ui::theme;

//  Format per data line  (total = 86 chars):
//  [9 label] [─×19] [┤] [28 inner] [├] [─×19] [9 label]
//   9 + 19 + 1 + 28 + 1 + 19 + 9  =  86
//
//  Inner chip box:
//   "  ╔════════════════════════╗  "  →  2+1+22+1+2 = 28  ✓
//   "  ║  [20-char content]  ║  "  →  2+1+2+20+2+1+2... = 30 — too wide
//   Use:  " ╔════════════════════════╗ "  →  1+1+24+1+1 = 28  ✓
//          " ║  [22-char content]  ║ "  →  1+1+2+22+2+1 = 29 — 1 off
//   Use: inner=28, box=" ╔══════════════════════════╗" = 1+1+24+1+1 = 28 ✓
//        content line: " ║  [20 chars]   ║ " = 1+1+2+20+3+1 = 28 ✓  (when content=20)
//
//  Pipeline boxes fit in 20 chars:
//   ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐  →  3+1+3+1+3+1+3+1+3 = 19  ✓
//   │IF│─│ID│─│EX│─│MA│  →  same                    ✓

const CHIP: &[&str] = &[
    //         [9]        [───19───] ┌─top connector 30─┐ [───19───]        [9]
    "                              ┌─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┐                              ",
    //         [9 label]  [───19──] ┤ [────── 28 inner ──────] ├ [───19──] [9 label]
    "   VCC    ───────────────────┤                            ├─────────────────    GND    ",
    "   CLK    ───────────────────┤ ╔══════════════════════════╗├─────────────────   nRST   ",
    "  XTAL    ───────────────────┤ ║  ┌──────────────────────┐║├─────────────────    IRQ   ",
    "   nRST   ───────────────────┤ ║  │  R  ·  A  ·  V  ·  E·N│║├─────────────────    INT   ",
    "   SDA    ───────────────────┤ ║  │  ─────────────────── │║├─────────────────   MOSI   ",
    "   SCL    ───────────────────┤ ║  │      R  I  S  C─V    │║├─────────────────   MISO   ",
    "    A0    ───────────────────┤ ║  │      R  V  3  2  I   │║├─────────────────    SCK   ",
    "    A2    ───────────────────┤ ║  │      M  ·  ·  ·  F   │║├─────────────────     CS   ",
    "    D0    ───────────────────┤ ║  └──────────────────────┘║├─────────────────     D1   ",
    "    D2    ───────────────────┤ ║                          ║├─────────────────     D3   ",
    "    D4    ───────────────────┤ ║  ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐   ║├─────────────────     D5   ",
    "    D6    ───────────────────┤ ║  │F│─│D│─│E│─│M│─│W│   ║├─────────────────     D7   ",
    "    PC    ───────────────────┤ ║  │E│ │E│ │X│ │A│ │B│   ║├─────────────────     SP   ",
    "    RA    ───────────────────┤ ║  │T│ │C│ │E│ │  │ │ │   ║├─────────────────     T0   ",
    "   ALU    ───────────────────┤ ║  └─┘ └─┘ └─┘ └─┘ └─┘   ║├─────────────────    MEM   ",
    "   CSR    ───────────────────┤ ║                          ║├─────────────────     WB   ",
    "  FETCH   ───────────────────┤ ║  ┌─────┐ ┌────┐ ┌──┐   ║├─────────────────    DBG   ",
    "  EXEC    ───────────────────┤ ║  │REG  │ │ ALU│ │$I│   ║├─────────────────   CTRL   ",
    "   MEM    ───────────────────┤ ║  │ x32 │ │    │ │$D│   ║├─────────────────   HALT   ",
    "   GND    ───────────────────┤ ╚══════════════════════════╝├─────────────────    VCC   ",
    "                              └─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┘                              ",
];

const SUBTITLE: &str =
    "RISC-V Simulator & IDE   ·   RV32IMF   ·   128 KB RAM   ·   5-stage pipeline";

pub fn render_splash(f: &mut Frame, started: Instant, duration_secs: f64) {
    let area = f.area();

    f.render_widget(
        Block::default().style(Style::default().bg(theme::BG)),
        area,
    );

    let chip_h = CHIP.len() as u16;
    let chip_w = CHIP.iter().map(|l| l.chars().count() as u16).max().unwrap_or(88);
    let total_h = chip_h + 4; // chip + blank + subtitle + blank + bar

    let y0 = area.height.saturating_sub(total_h) / 2;
    let x0 = area.width.saturating_sub(chip_w) / 2;

    // ── Chip ──────────────────────────────────────────────────────────────
    for (i, line) in CHIP.iter().enumerate() {
        let row = y0 + i as u16;
        if row >= area.height { break; }
        let spans = colorize_line(line);
        let w = chip_w.min(area.width.saturating_sub(x0));
        f.render_widget(Paragraph::new(Line::from(spans)), Rect::new(x0, row, w, 1));
    }

    // ── Subtitle ──────────────────────────────────────────────────────────
    let sub_y = y0 + chip_h + 1;
    if sub_y < area.height {
        let sub_x = area.width.saturating_sub(SUBTITLE.len() as u16) / 2;
        f.render_widget(
            Paragraph::new(Span::styled(SUBTITLE, Style::default().fg(theme::IDLE))),
            Rect::new(sub_x, sub_y, SUBTITLE.len() as u16, 1),
        );
    }

    // ── Progress bar ──────────────────────────────────────────────────────
    let elapsed  = started.elapsed().as_secs_f64();
    let progress = (elapsed / duration_secs).clamp(0.0, 1.0);
    let pct      = (progress * 100.0) as u16;

    let label = match pct {
        0..=9   => "  Powering on...",
        10..=19 => "  Initializing register file  (x0–x31  ·  f0–f31  ·  pc  ·  fcsr)...",
        20..=29 => "  Loading base ISA  (RV32I — 37 integer instructions)...",
        30..=39 => "  Loading M extension  (multiply · divide · remainder)...",
        40..=49 => "  Loading F extension  (26 single-precision float instructions)...",
        50..=59 => "  Mapping address space  (0x00000000 – 0x0001FFFF  ·  128 KB  ·  no MMU)...",
        60..=69 => "  Configuring cache hierarchy  (L1-I  ·  L1-D  ·  set-associative)...",
        70..=79 => "  Wiring pipeline  (IF → ID → EX → MA → WB)...",
        80..=89 => "  Initializing assembler  (pseudo-instructions · directives · labels)...",
        90..=98 => "  Booting RISC-V core...",
        _       => "  Ready.",
    };

    let bar_w = chip_w.min(area.width.saturating_sub(x0));
    let bar_y = sub_y + 2;
    if bar_y < area.height {
        f.render_widget(
            Gauge::default()
                .block(Block::default())
                .gauge_style(Style::default().fg(theme::ACCENT).bg(theme::BG_PANEL))
                .label(Span::styled(label, Style::default().fg(theme::TEXT)))
                .percent(pct),
            Rect::new(x0, bar_y, bar_w, 1),
        );
    }
}

/// Colorize a single chip line using the chip body boundaries.
fn colorize_line(line: &str) -> Vec<Span<'static>> {
    let chars: Vec<char> = line.chars().collect();
    let lb = chars.iter().position(|&c| c == '┤').unwrap_or(0);
    let rb = chars.iter().rposition(|&c| c == '├').unwrap_or(chars.len().saturating_sub(1));

    chars.iter().enumerate().map(|(i, &c)| {
        let color = if i == lb || i == rb {
            theme::BORDER_HOV
        } else if i > lb && i < rb {
            color_inside(c)
        } else {
            color_outside(c)
        };
        Span::styled(c.to_string(), Style::default().fg(color))
    }).collect()
}

fn color_inside(c: char) -> Color {
    match c {
        // Double-line chip border
        '╔' | '╗' | '╚' | '╝' | '═' | '║' => theme::ACCENT,
        // Single-line inner boxes
        '┌' | '┐' | '└' | '┘' | '─' | '│' | '┬' | '┴' | '┤' | '├' => theme::BORDER_HOV,
        // Separator dots
        '·' => theme::IDLE,
        // Spaces keep default background
        ' ' => theme::BG,
        // Pipeline stage abbreviations — green
        'F' | 'E' | 'T' | 'D' | 'C' | 'X' | 'M' | 'A' | 'W' | 'B' => theme::RUNNING,
        // Component box labels — blue
        'R' | 'G' | '$' | 'I' => theme::METRIC_CYC,
        // Numbers in ISA names — amber
        '0'..='9' => theme::PAUSED,
        _ => theme::TEXT,
    }
}

fn color_outside(c: char) -> Color {
    match c {
        '─' => theme::LABEL,
        '┌' | '┐' | '└' | '┘' | '┬' | '┴' => theme::BORDER_HOV,
        ' ' => theme::BG,
        _ => theme::TEXT,
    }
}
