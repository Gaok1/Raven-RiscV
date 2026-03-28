use crate::ui::theme;
use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};
use std::time::Instant;

const CHIP: &[&str] = &[
    "                    ┌─────────────────────────────────────┐                     ",
    "  VCC  ─────────────│                                     │────────────  GND  ",
    "  CLK  ─────────────┤  ╔═══════════════════════════════╗  ├────────────  nRST ",
    " XTAL  ─────────────┤  ║                               ║  ├────────────  IRQ  ",
    "  SDA  ─────────────┤  ║     R · A · V · E · N         ║  ├────────────  INT  ",
    "  SCL  ─────────────┤  ║     ───────────────────       ║  ├────────────  MOSI ",
    "   A0  ─────────────┤  ║         R I S C ─ V           ║  ├────────────  MISO ",
    "   A2  ─────────────┤  ║        R V 3 2 I M F          ║  ├────────────  SCK  ",
    "   D0  ─────────────┤  ║                               ║  ├────────────  CS   ",
    "   D2  ─────────────┤  ║    ┌─┐ ┌─┐ ┌─┐ ┌─┐ ┌─┐        ║  ├────────────  D1   ",
    "   D4  ─────────────┤  ║    │F│─│D│─│E│─│M│─│W│        ║  ├────────────  D3   ",
    "   D6  ─────────────┤  ║    └─┘ └─┘ └─┘ └─┘ └─┘        ║  ├────────────  D5   ",
    "    PC ─────────────┤  ║                               ║  ├────────────  SP   ",
    "    RA ─────────────┤  ║  ┌────┐  ┌───┐   ┌────┐┌────┐ ║  ├────────────  T0   ",
    "   ALU ─────────────┤  ║  │REG │──│ALU│───│ I$ ││ D$ │ ║  ├────────────  MEM  ",
    "   CSR ─────────────┤  ║  │ x32│  │   │   └─┬──┘└──┬─┘ ║  ├────────────  WB   ",
    " FETCH ─────────────┤  ║  └────┘  └───┘   ┌─┴──────┴─┐ ║  ├────────────  DBG  ",
    "   RAM ─────────────┤  ║                  │  CACHE   │ ║  ├────────────  BUS  ",
    "   GND ─────────────┤  ║                  └──────────┘ ║  ├────────────  VCC  ",
    "                    │  ╚═══════════════════════════════╝  │                    ",
    "                    └─────────────────────────────────────┘                     ",
];

struct AnimState {
    elapsed: f64,
    progress: f64,
    stage_gate: usize,
    stage_pulse: usize,
    wire_head: usize,
    logo_pulse: usize,
    clk_phase: bool,
    spinner: char,
}

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

    let elapsed = started.elapsed().as_secs_f64();
    let progress = (elapsed / duration_secs).clamp(0.0, 1.0);
    let anim = AnimState {
        elapsed,
        progress,
        stage_gate: ((progress * 5.0).floor() as usize).min(4),
        stage_pulse: ((elapsed * 7.5) as usize) % 5,
        wire_head: ((elapsed * 28.0) as usize) % 24,
        logo_pulse: ((elapsed * 14.0) as usize) % 31,
        clk_phase: ((elapsed * 6.0) as usize) % 2 == 0,
        spinner: ['|', '/', '-', '\\'][((elapsed * 8.0) as usize) % 4],
    };

    let chip_h = CHIP.len() as u16;
    let chip_w = CHIP[1].chars().count() as u16;
    let total_h = chip_h + 8;
    let y0 = area.height.saturating_sub(total_h) / 2;
    let x0 = area.width.saturating_sub(chip_w) / 2;

    for (row_idx, line) in CHIP.iter().enumerate() {
        let row = y0 + row_idx as u16;
        if row >= area.height {
            break;
        }
        let spans = colorize_line(line, row_idx, &anim);
        let w = chip_w.min(area.width.saturating_sub(x0));
        f.render_widget(Paragraph::new(Line::from(spans)), Rect::new(x0, row, w, 1));
    }

    let subtitle = format!(
        "{}  Raven boot console   ·   RV32IMF   ·   {}   ·   5-stage pipeline",
        anim.spinner,
        format_mem(mem_size)
    );
    let sub_y = y0 + chip_h + 1;
    if sub_y < area.height {
        let sub_x = area.width.saturating_sub(subtitle.len() as u16) / 2;
        f.render_widget(
            Paragraph::new(Span::styled(
                subtitle.clone(),
                Style::default().fg(theme::ACTIVE).add_modifier(Modifier::BOLD),
            )),
            Rect::new(sub_x, sub_y, subtitle.len() as u16, 1),
        );
    }

    let status = match ((elapsed * 1.8) as usize) % 5 {
        0 => "Power rails stable  ·  clock tree oscillating",
        1 => "Decode fabric online  ·  register banks biased",
        2 => "Pipeline synchronized  ·  IF -> ID -> EX -> MA -> WB",
        3 => "L1 I$/D$ cache online  ·  assembler front-end ready",
        _ => "Core standing by for operator input",
    };

    let status_y = sub_y + 2;
    if status_y < area.height {
        let status_x = area.width.saturating_sub(status.len() as u16) / 2;
        f.render_widget(
            Paragraph::new(Span::styled(status, Style::default().fg(theme::LABEL))),
            Rect::new(status_x, status_y, status.len() as u16, 1),
        );
    }

    let prompt_y = status_y + 2;
    let prompt_w = 30u16.min(area.width.saturating_sub(4));
    let prompt_x = area.width.saturating_sub(prompt_w) / 2;
    let pulse_on = ((elapsed * 2.5) as usize) % 2 == 0;
    let prompt_style = if pulse_on {
        Style::default()
            .fg(theme::BG)
            .bg(theme::ACTIVE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::ACTIVE)
            .bg(theme::BG_PANEL)
            .add_modifier(Modifier::BOLD)
    };
    let prompt_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if pulse_on {
            theme::ACTIVE
        } else {
            theme::BORDER_HOV
        }));
    if prompt_y + 2 < area.height {
        f.render_widget(
            Paragraph::new("   ENTER TO START   ")
                .style(prompt_style)
                .block(prompt_block)
                .alignment(Alignment::Center),
            Rect::new(prompt_x, prompt_y, prompt_w, 3),
        );
    }

    let hint = "Esc quits";
    let hint_y = prompt_y + 4;
    if hint_y < area.height {
        let hint_x = area.width.saturating_sub(hint.len() as u16) / 2;
        f.render_widget(
            Paragraph::new(Span::styled(hint, Style::default().fg(theme::IDLE))),
            Rect::new(hint_x, hint_y, hint.len() as u16, 1),
        );
    }
}

fn colorize_line(line: &str, row_idx: usize, anim: &AnimState) -> Vec<Span<'static>> {
    let chars: Vec<char> = line.chars().collect();
    let lb = chars
        .iter()
        .position(|&c| matches!(c, '┤' | '│'))
        .unwrap_or(0);
    let rb = chars
        .iter()
        .rposition(|&c| matches!(c, '├' | '│'))
        .unwrap_or(chars.len().saturating_sub(1));

    chars
        .iter()
        .enumerate()
        .map(|(col, &c)| {
            let style = if col == lb || col == rb {
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else if col > lb && col < rb {
                style_inside(c, row_idx, col - lb - 1, anim)
            } else {
                style_outside(c, row_idx, col, lb, rb, chars.len(), anim)
            };
            Span::styled(c.to_string(), style)
        })
        .collect()
}

fn style_inside(c: char, row: usize, inner_col: usize, anim: &AnimState) -> Style {
    let mut style = Style::default().fg(match c {
        '╔' | '╗' | '╚' | '╝' | '═' | '║' => theme::ACCENT,
        '┌' | '┐' | '└' | '┘' | '─' | '│' | '┬' | '┴' | '┤' | '├' => theme::BORDER_HOV,
        '·' => theme::IDLE,
        '0'..='9' => theme::PAUSED,
        ' ' => theme::BG,
        _ => theme::TEXT,
    });

    if c == ' ' {
        return style;
    }

    if matches!(c, 'R' | 'A' | 'V' | 'E' | 'N') && row == 4 {
        if anim.progress > 0.12 {
            let logo_col = inner_col.saturating_sub(3);
            if logo_col.abs_diff(anim.logo_pulse) <= 1 {
                style = style.fg(theme::ACTIVE).add_modifier(Modifier::BOLD);
            } else {
                style = style.fg(theme::METRIC_CPI).add_modifier(Modifier::BOLD);
            }
        }
        return style;
    }

    if matches!(c, 'R' | 'I' | 'S' | 'C' | 'V') && row == 6 {
        if anim.progress > 0.22 {
            style = style.fg(theme::METRIC_CYC).add_modifier(Modifier::BOLD);
        }
        return style;
    }

    if matches!(c, 'R' | 'V' | 'I' | 'M' | 'F' | '3' | '2') && row == 7 {
        if anim.progress > 0.35 {
            style = style.fg(theme::PAUSED).add_modifier(Modifier::BOLD);
        }
        return style;
    }

    if matches!(c, 'F' | 'D' | 'E' | 'M' | 'W') && row == 10 {
        let stage_idx = match c {
            'F' => 0,
            'D' => 1,
            'E' => 2,
            'M' => 3,
            'W' => 4,
            _ => 0,
        };
        let active = stage_idx <= anim.stage_gate
            && (stage_idx == anim.stage_pulse || anim.progress > 0.9);
        style = if active {
            Style::default()
                .fg(theme::RUNNING)
                .add_modifier(Modifier::BOLD)
        } else if stage_idx <= anim.stage_gate {
            Style::default().fg(theme::RUNNING)
        } else {
            Style::default().fg(theme::IDLE)
        };
        return style;
    }

    if c == 'I' && row == 15 && anim.progress > 0.55 {
        style = style.fg(theme::CACHE_I).add_modifier(Modifier::BOLD);
        return style;
    }

    if c == 'D' && row == 15 && anim.progress > 0.55 {
        style = style.fg(theme::CACHE_D).add_modifier(Modifier::BOLD);
        return style;
    }

    if c == '$' && row == 15 && anim.progress > 0.55 {
        let cache_color = if inner_col < 28 {
            theme::CACHE_I
        } else {
            theme::CACHE_D
        };
        style = style.fg(cache_color).add_modifier(Modifier::BOLD);
        return style;
    }

    if matches!(c, 'R' | 'G') && row >= 13 && anim.progress > 0.55 {
        style = style.fg(theme::METRIC_CYC).add_modifier(Modifier::BOLD);
        return style;
    }

    if matches!(c, 'L' | 'C' | 'A' | 'H' | 'E' | '1' | 'B' | 'N' | 'K' | 'S') && row == 17 && anim.progress > 0.55 {
        style = style.fg(theme::CACHE_L2).add_modifier(Modifier::BOLD);
        return style;
    }

    if matches!(c, 'x' | '3' | '2') && row == 15 && anim.progress > 0.18 {
        style = style.fg(theme::ACTIVE).add_modifier(Modifier::BOLD);
        return style;
    }

    if row == 5 && c == '─' && anim.progress > 0.1 {
        let center = 15usize;
        let left_target = center.saturating_sub(anim.logo_pulse.min(center));
        let right_target = center + anim.logo_pulse.min(30usize.saturating_sub(center));
        if inner_col.abs_diff(left_target) <= 1 || inner_col.abs_diff(right_target) <= 1 {
            style = style.fg(theme::PAUSED).add_modifier(Modifier::BOLD);
        }
    }

    if matches!(
        c,
        '┌' | '┐' | '└' | '┘' | '─' | '│' | '┬' | '┴' | '┤' | '├' | '╔' | '╗' | '╚' | '╝' | '═' | '║'
    ) && anim.progress > 0.1
    {
        let pulse_col = ((anim.elapsed * 12.0) as usize + row) % 31;
        if inner_col.abs_diff(pulse_col) == 0 {
            style = style.fg(theme::ACTIVE).add_modifier(Modifier::BOLD);
        } else if inner_col.abs_diff(pulse_col) == 1 {
            style = style.fg(theme::METRIC_CPI);
        }
    }

    style
}

fn style_outside(
    c: char,
    row: usize,
    col: usize,
    lb: usize,
    rb: usize,
    width: usize,
    anim: &AnimState,
) -> Style {
    let mut style = Style::default().fg(match c {
        '─' => theme::LABEL,
        '┌' | '┐' | '└' | '┘' | '┬' | '┴' => theme::BORDER_HOV,
        ' ' => theme::BG,
        _ => theme::TEXT,
    });

    if c == ' ' {
        return style;
    }

    let left_pulse = row + anim.wire_head;
    let right_pulse = row * 2 + anim.wire_head;
    let left_match = col < lb && col.abs_diff(left_pulse) <= 1;
    let right_col = width.saturating_sub(1).saturating_sub(col);
    let right_span = width.saturating_sub(1).saturating_sub(rb);
    let right_match = col > rb && right_col.abs_diff((right_pulse % (right_span + 8)).min(right_span)) <= 1;

    let clk_rail = row == 2 && c == '─';

    if clk_rail {
        style = if anim.clk_phase {
            style.fg(theme::ACTIVE).add_modifier(Modifier::BOLD)
        } else {
            style.fg(theme::METRIC_CYC)
        };
    } else if left_match || right_match {
        style = style.fg(theme::PAUSED).add_modifier(Modifier::BOLD);
    }

    if row == 2 && matches!(c, 'C' | 'L' | 'K') {
        style = if anim.clk_phase {
            style.fg(theme::ACTIVE).add_modifier(Modifier::BOLD)
        } else {
            style.fg(theme::METRIC_CYC)
        };
    }

    if matches!(c, 'V' | 'C' | 'K' | 'R' | 'S' | 'D' | 'A' | 'L' | 'U' | 'M' | 'P' | 'T' | 'I' | 'O' | 'B' | 'G' | 'N' | 'X' | 'H')
        && anim.progress > 0.05
        && ((row + col + (anim.elapsed * 10.0) as usize) % 9 == 0)
    {
        style = style.fg(theme::ACTIVE);
    }

    style
}
