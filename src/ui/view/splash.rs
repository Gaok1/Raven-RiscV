use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, Gauge, Paragraph},
};
use std::time::Instant;
use crate::ui::theme;

// All lines verified at exactly 77 chars.
// Format: [8 label][──×14][┤][31 inner][├][──×14][8 label]
// Inner chip box: "  ╔═════════════════════════╗  " = 2+1+25+1+2 = 31
// Content lines:  "  ║<───────── 25 ─────────>║  " = 31
const CHIP: &[&str] = &[
    "                      ┌─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┬─┐                      ",
    "   VCC  ──────────────┤                               ├──────────────  GND   ",
    "   CLK  ──────────────┤  ╔═════════════════════════╗  ├──────────────  nRST  ",
    "  XTAL  ──────────────┤  ║                         ║  ├──────────────  IRQ   ",
    "   SDA  ──────────────┤  ║   R · A · V · E · N     ║  ├──────────────  INT   ",
    "   SCL  ──────────────┤  ║   ───────────────────   ║  ├──────────────  MOSI  ",
    "    A0  ──────────────┤  ║       R I S C ─ V       ║  ├──────────────  MISO  ",
    "    A2  ──────────────┤  ║      R V 3 2 I M F      ║  ├──────────────  SCK   ",
    "    D0  ──────────────┤  ║                         ║  ├──────────────  CS    ",
    "    D2  ──────────────┤  ║   ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐   ║  ├──────────────  D1    ",
    "    D4  ──────────────┤  ║   │F│─│D│─│E│─│M│─│W│   ║  ├──────────────  D3    ",
    "    D6  ──────────────┤  ║   └─┘ └─┘ └─┘ └─┘ └─┘   ║  ├──────────────  D5    ",
    "     PC ──────────────┤  ║                         ║  ├──────────────  SP    ",
    "     RA ──────────────┤  ║   ┌────┐  ┌───┐  ┌──┐   ║  ├──────────────  T0    ",
    "    ALU ──────────────┤  ║   │REG │  │ALU│  │$I│   ║  ├──────────────  MEM   ",
    "    CSR ──────────────┤  ║   │ x32│  │   │  │$D│   ║  ├──────────────  WB    ",
    "  FETCH ──────────────┤  ║   └────┘  └───┘  └──┘   ║  ├──────────────  DBG   ",
    "   GND  ──────────────┤  ╚═════════════════════════╝  ├──────────────  VCC   ",
    "                      └─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┴─┘                      ",
];

fn format_mem(bytes: usize) -> String {
    let kb = bytes / 1024;
    if kb >= 1024 && kb % 1024 == 0 {
        format!("{} MB", kb / 1024)
    } else {
        format!("{} KB", kb)
    }
}

pub fn render_splash(f: &mut Frame, started: Instant, duration_secs: f64, mem_size: usize) {
    let area = f.area();

    f.render_widget(
        Block::default().style(Style::default().bg(theme::BG)),
        area,
    );

    let chip_h = CHIP.len() as u16;
    let chip_w = CHIP[1].chars().count() as u16; // all lines same width
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
    let subtitle = format!(
        "RISC-V Simulator & IDE   ·   RV32IMF   ·   {}   ·   5-stage pipeline",
        format_mem(mem_size)
    );
    let sub_y = y0 + chip_h + 1;
    if sub_y < area.height {
        let sub_x = area.width.saturating_sub(subtitle.len() as u16) / 2;
        f.render_widget(
            Paragraph::new(Span::styled(subtitle.clone(), Style::default().fg(theme::IDLE))),
            Rect::new(sub_x, sub_y, subtitle.len() as u16, 1),
        );
    }

    // ── Progress bar ──────────────────────────────────────────────────────
    let elapsed  = started.elapsed().as_secs_f64();
    let progress = (elapsed / duration_secs).clamp(0.0, 1.0);
    let pct      = (progress * 100.0) as u16;

    let label: String = match pct {
        0..=9   => "  Powering on...".into(),
        10..=19 => "  Initializing register file  (x0–x31  ·  f0–f31  ·  pc  ·  fcsr)...".into(),
        20..=29 => "  Loading base ISA  (RV32I — 37 integer instructions)...".into(),
        30..=39 => "  Loading M extension  (multiply · divide · remainder)...".into(),
        40..=49 => "  Loading F extension  (26 single-precision float instructions)...".into(),
        50..=59 => format!(
            "  Mapping address space  (0x00000000 – 0x{:08X}  ·  {}  ·  no MMU)...",
            mem_size.saturating_sub(1),
            format_mem(mem_size),
        ),
        60..=69 => "  Configuring cache hierarchy  (L1-I  ·  L1-D  ·  set-associative)...".into(),
        70..=79 => "  Wiring pipeline  (IF → ID → EX → MA → WB)...".into(),
        80..=89 => "  Initializing assembler  (pseudo-instructions · directives · labels)...".into(),
        90..=98 => "  Booting RISC-V core...".into(),
        _       => "  Ready.".into(),
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

/// Colorize a chip line by splitting on the outermost ┤ and ├ boundaries.
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
        // Chip outer border (double lines) — accent violet
        '╔' | '╗' | '╚' | '╝' | '═' | '║' => theme::ACCENT,
        // Inner boxes (single lines) — border violet
        '┌' | '┐' | '└' | '┘' | '─' | '│' | '┬' | '┴' | '┤' | '├' => theme::BORDER_HOV,
        // Separator dots — dim
        '·' => theme::IDLE,
        // Pipeline stages — green
        'F' | 'D' | 'E' | 'M' | 'W' => theme::RUNNING,
        // Component labels — blue
        'R' | 'G' | '$' => theme::METRIC_CYC,
        // Digits in ISA names — amber
        '0'..='9' => theme::PAUSED,
        // Spaces — keep background clean
        ' ' => theme::BG,
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
