//! Boot splash — a cinematic, fixed-length power-on sequence for the Raven core.
//!
//! No key is required: the run loop clears `splash_start` after [`SPLASH_SECS`]
//! (any key skips early, Esc opens the exit popup). Every frame is a pure
//! function of the elapsed time, drawn onto a cell [`Canvas`] in layers:
//!
//! 1. power surge — two beams race in from the screen edges and meet;
//! 2. circuit fabric — pins and solder dots energize toward the die;
//! 3. die frame — the core's outline draws itself in;
//! 4. logo ignition — RAVEN materializes as a white-hot wave that cools
//!    into the theme violet, with sparks riding the wavefront;
//! 5. POST log + stage chips + power bar — the machine reports in;
//! 6. flash — one white-hot pulse, then hand-off to the console.

use crate::ui::theme;
use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, Paragraph},
};
use std::time::Instant;

/// Total splash duration; the run loop dismisses the splash after this long.
pub(crate) const SPLASH_SECS: f64 = 5.2;

// ── Phase timeline (seconds) ──────────────────────────────────────────────────
const SURGE: (f64, f64) = (0.05, 0.70);
const FABRIC: (f64, f64) = (0.45, 2.60);
const FRAME: (f64, f64) = (0.80, 1.80);
const LOGO: (f64, f64) = (1.70, 3.30);
const SUBTITLE: (f64, f64) = (3.10, 3.80);
const STAGES_T0: f64 = 3.20;
const LOG_T0: f64 = 1.90;
const BAR: (f64, f64) = (0.70, 4.45);
const FLASH: (f64, f64) = (4.55, 4.90);

// ── Entry points ──────────────────────────────────────────────────────────────

pub fn render_splash(f: &mut Frame, started: Instant, mem_size: usize) {
    render_splash_frame(f, started.elapsed().as_secs_f64(), mem_size);
}

fn render_splash_frame(f: &mut Frame, t: f64, mem_size: usize) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(theme::BG)), area);
    if area.width < 8 || area.height < 4 {
        return;
    }

    let mut cv = Canvas::new(area.width as usize, area.height as usize);
    if area.width >= 72 && area.height >= 23 {
        draw_full(&mut cv, t, mem_size);
    } else {
        draw_compact(&mut cv, t, mem_size);
    }
    cv.flush(f, area);
}

// ── Cell canvas ───────────────────────────────────────────────────────────────

struct Canvas {
    w: usize,
    h: usize,
    ch: Vec<char>,
    st: Vec<Style>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            ch: vec![' '; w * h],
            st: vec![Style::default().bg(theme::BG); w * h],
        }
    }

    fn put(&mut self, x: i64, y: i64, c: char, s: Style) {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 {
            return;
        }
        let i = y as usize * self.w + x as usize;
        self.ch[i] = c;
        self.st[i] = s.bg(theme::BG);
    }

    fn text(&mut self, x: i64, y: i64, s: &str, style: Style) {
        for (i, c) in s.chars().enumerate() {
            self.put(x + i as i64, y, c, style);
        }
    }

    fn flush(self, f: &mut Frame, area: Rect) {
        for row in 0..self.h {
            let mut spans: Vec<Span<'static>> = Vec::with_capacity(self.w / 4);
            let base = row * self.w;
            let mut run = String::new();
            let mut run_style = self.st[base];
            for col in 0..self.w {
                let (c, s) = (self.ch[base + col], self.st[base + col]);
                if s == run_style {
                    run.push(c);
                } else {
                    spans.push(Span::styled(std::mem::take(&mut run), run_style));
                    run.push(c);
                    run_style = s;
                }
            }
            spans.push(Span::styled(run, run_style));
            f.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect::new(area.x, area.y + row as u16, area.width, 1),
            );
        }
    }
}

// ── Small math helpers ────────────────────────────────────────────────────────

fn clamp01(p: f64) -> f64 {
    p.clamp(0.0, 1.0)
}

/// 0→1 progress through a phase window, eased (fast start, soft landing).
fn phase(t: f64, (a, b): (f64, f64)) -> f64 {
    let p = clamp01((t - a) / (b - a));
    1.0 - (1.0 - p).powi(3)
}

fn mix(a: (u8, u8, u8), b: (u8, u8, u8), p: f64) -> Color {
    let p = clamp01(p);
    let l = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * p).round() as u8;
    Color::Rgb(l(a.0, b.0), l(a.1, b.1), l(a.2, b.2))
}

/// The ignition ramp: dark indigo → electric violet → lavender → white-hot.
fn ramp(p: f64) -> Color {
    let p = clamp01(p);
    const STOPS: [(f64, (u8, u8, u8)); 5] = [
        (0.00, (24, 18, 42)),
        (0.35, (72, 44, 132)),
        (0.65, (145, 95, 250)), // theme violet
        (0.85, (196, 164, 255)),
        (1.00, (246, 240, 255)),
    ];
    for w in STOPS.windows(2) {
        let ((p0, c0), (p1, c1)) = (w[0], w[1]);
        if p <= p1 {
            return mix(c0, c1, (p - p0) / (p1 - p0));
        }
    }
    Color::Rgb(246, 240, 255)
}

/// Deterministic per-cell hash (splitmix-ish) for jitter and sparkles.
fn hash(x: u64, y: u64) -> u64 {
    let mut z = x
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(y.wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add(0x94D0_49BB_1331_11EB);
    z ^= z >> 30;
    z = z.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z ^= z >> 27;
    z
}

fn format_mem(bytes: usize) -> String {
    let kb = bytes / 1024;
    if kb >= 1024 && kb % 1024 == 0 {
        format!("{} MB", kb / 1024)
    } else {
        format!("{} KB", kb)
    }
}

// ── Block font (5×5 cells, each cell two columns wide) ────────────────────────

const LETTER_W: usize = 10; // 5 cells × 2 columns
const LETTER_GAP: usize = 2;
const LOGO_ROWS: usize = 5;
const LOGO_W: usize = 5 * LETTER_W + 4 * LETTER_GAP; // 58

fn glyph(c: char) -> [&'static str; LOGO_ROWS] {
    match c {
        'R' => ["####.", "#...#", "####.", "#..#.", "#...#"],
        'A' => [".###.", "#...#", "#####", "#...#", "#...#"],
        'V' => ["#...#", "#...#", "#...#", ".#.#.", "..#.."],
        'E' => ["#####", "#....", "####.", "#....", "#####"],
        'N' => ["#...#", "##..#", "#.#.#", "#..##", "#...#"],
        _ => [".....", ".....", ".....", ".....", "....."],
    }
}

// ── Full composition ──────────────────────────────────────────────────────────
//
// Vertically: die (12 rows) · gap · POST log (6) · gap · power bar (1) = 21.

const DIE_W: usize = 64;
const DIE_H: usize = 12;
const LOG_LINES: usize = 6;
const TOTAL_H: usize = DIE_H + 1 + LOG_LINES + 1 + 1;

fn draw_full(cv: &mut Canvas, t: f64, mem_size: usize) {
    let (w, h) = (cv.w, cv.h);
    let die_x = (w.saturating_sub(DIE_W) / 2) as i64;
    let die_y = (h.saturating_sub(TOTAL_H) / 2) as i64;

    draw_fabric(cv, t, die_x, die_y);
    draw_pins(cv, t, die_x, die_y);
    draw_die(cv, t, die_x, die_y);
    draw_logo(cv, t, die_x + 3, die_y + 2);
    draw_subtitle(cv, t, die_x, die_y + 8, mem_size);
    draw_stages(cv, t, die_x, die_y + 9);
    draw_log(cv, t, die_x, die_y + DIE_H as i64 + 1);
    draw_bar(cv, t, die_x, die_y + DIE_H as i64 + 1 + LOG_LINES as i64 + 1);
    draw_surge(cv, t, die_y + DIE_H as i64 / 2);
}

/// Phase 0 — two power beams race in from the edges and meet at the die.
fn draw_surge(cv: &mut Canvas, t: f64, y: i64) {
    if t >= SURGE.1 + 0.25 {
        return;
    }
    let p = phase(t, SURGE);
    let fade = clamp01((SURGE.1 + 0.25 - t) / 0.25);
    let cx = cv.w as i64 / 2;
    let head = (p * cx as f64) as i64;
    for x in 0..head {
        let tail = clamp01(x as f64 / head.max(1) as f64);
        let c = ramp(0.25 + 0.75 * tail * fade);
        cv.put(x, y, '━', Style::default().fg(c));
        cv.put(cv.w as i64 - 1 - x, y, '━', Style::default().fg(c));
    }
    if p < 1.0 {
        let hot = Style::default().fg(ramp(1.0)).add_modifier(Modifier::BOLD);
        cv.put(head, y, '█', hot);
        cv.put(cv.w as i64 - 1 - head, y, '█', hot);
    }
}

/// Sparse solder dots waking up radially around the die.
fn draw_fabric(cv: &mut Canvas, t: f64, die_x: i64, die_y: i64) {
    let p = phase(t, FABRIC);
    if p <= 0.0 {
        return;
    }
    let flash = if t >= FLASH.0 && t <= FLASH.1 { 0.35 } else { 0.0 };
    let (cx, cy) = (die_x + DIE_W as i64 / 2, die_y + DIE_H as i64 / 2);
    let max_r = (cv.w as f64 / 2.0).hypot(cv.h as f64 / 2.0);
    let r = p * max_r;
    for y in 0..cv.h as i64 {
        for x in 0..cv.w as i64 {
            let hv = hash(x as u64, y as u64);
            if hv % 41 != 0 {
                continue;
            }
            // Keep the die's neighbourhood clean.
            if x >= die_x - 2
                && x < die_x + DIE_W as i64 + 2
                && y >= die_y - 1
                && y < die_y + TOTAL_H as i64 + 1
            {
                continue;
            }
            // Terminal cells are ~2× taller than wide.
            let d = ((x - cx) as f64).hypot(((y - cy) * 2) as f64);
            if d > r {
                continue;
            }
            let twinkle = (hv >> 8) % 97 == (t * 30.0) as u64 % 97;
            let birth = clamp01((r - d) / 14.0);
            let lvl = (0.10 + 0.25 * birth + flash + if twinkle { 0.45 } else { 0.0 }).min(1.0);
            cv.put(x, y, '·', Style::default().fg(ramp(lvl * 0.55)));
        }
    }
}

/// Power pins feeding the die from the left, right and top screen edges.
fn draw_pins(cv: &mut Canvas, t: f64, die_x: i64, die_y: i64) {
    let steel = (52, 62, 82);
    let hot = (110, 175, 220); // theme steel-blue
    let pin = |cvv: &mut Canvas, path: Vec<(i64, i64)>, c: char, seed: u64| {
        let len = path.len();
        if len == 0 {
            return;
        }
        let start = 0.55 + (seed % 7) as f64 * 0.11;
        let p = phase(t, (start, start + 0.6));
        if p <= 0.0 {
            return;
        }
        let lit = (p * len as f64) as usize;
        // A recurring pulse rides edge→die once the pin is energized.
        let pulse = (((t * 1.5 + (seed % 11) as f64 * 0.17) % 1.35) * len as f64) as usize;
        for (i, &(x, y)) in path.iter().take(lit.max(1)).enumerate() {
            let is_head = p < 1.0 && i + 1 == lit;
            let is_pulse = p >= 1.0 && i.abs_diff(pulse) <= 1;
            let style = if is_head || is_pulse {
                Style::default()
                    .fg(mix(hot, (240, 250, 255), 0.4))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(mix(steel, hot, 0.18))
            };
            cvv.put(x, y, c, style);
        }
        // Landing node on the die's rim.
        if p >= 1.0 {
            if let Some(&(x, y)) = path.last() {
                cvv.put(x, y, '•', Style::default().fg(theme::ACCENT));
            }
        }
    };

    // Left + right pins, every other die row.
    for (k, row) in (1..DIE_H as i64 - 1).step_by(2).enumerate() {
        let y = die_y + row;
        let left: Vec<_> = (0..die_x).map(|x| (x, y)).collect();
        let right: Vec<_> = (die_x + DIE_W as i64..cv.w as i64).rev().map(|x| (x, y)).collect();
        pin(cv, left, '─', k as u64);
        pin(cv, right, '─', k as u64 + 3);
    }
    // Top pins, sparse columns.
    for (k, col) in (6..DIE_W as i64 - 6).step_by(9).enumerate() {
        let x = die_x + col;
        let down: Vec<_> = (0..die_y).map(|y| (x, y)).collect();
        pin(cv, down, '│', k as u64 + 7);
    }
}

/// The core's outline draws itself clockwise from the top-left corner.
fn draw_die(cv: &mut Canvas, t: f64, die_x: i64, die_y: i64) {
    // Interior is always kept dark so fabric/pins never bleed through.
    for y in 0..DIE_H as i64 {
        for x in 0..DIE_W as i64 {
            cv.put(die_x + x, die_y + y, ' ', Style::default());
        }
    }

    let p = phase(t, FRAME);
    if p <= 0.0 {
        return;
    }
    let (wi, hi) = (DIE_W as i64, DIE_H as i64);
    // Perimeter as an ordered path, clockwise from the top-left corner.
    let mut path: Vec<(i64, i64, char)> = Vec::with_capacity((2 * (wi + hi)) as usize);
    path.push((0, 0, '╭'));
    path.extend((1..wi - 1).map(|x| (x, 0, '─')));
    path.push((wi - 1, 0, '╮'));
    path.extend((1..hi - 1).map(|y| (wi - 1, y, '│')));
    path.push((wi - 1, hi - 1, '╯'));
    path.extend((1..wi - 1).rev().map(|x| (x, hi - 1, '─')));
    path.push((0, hi - 1, '╰'));
    path.extend((1..hi - 1).rev().map(|y| (0, y, '│')));

    let lit = ((p * path.len() as f64) as usize).max(1);
    for (i, &(x, y, c)) in path.iter().take(lit).enumerate() {
        let head = p < 1.0 && i + 1 >= lit.saturating_sub(1);
        let style = if head {
            Style::default().fg(ramp(1.0)).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(mix((92, 66, 158), (145, 95, 250), 0.45))
        };
        cv.put(die_x + x, die_y + y, c, style);
    }

    if p >= 1.0 {
        let title = " RAVEN CORE ";
        cv.text(
            die_x + 3,
            die_y,
            title,
            Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
        );
        let tag = " RISC-V ";
        cv.text(
            die_x + wi - 3 - tag.len() as i64,
            die_y,
            tag,
            Style::default().fg(theme::METRIC_CYC),
        );
        let status = if t >= BAR.1 {
            " CORE ONLINE "
        } else if t >= LOGO.1 {
            " CLOCK LOCKED "
        } else {
            " APPLYING POWER "
        };
        let color = if t >= BAR.1 { theme::RUNNING } else { theme::PAUSED };
        cv.text(
            die_x + (wi - status.len() as i64) / 2,
            die_y + hi - 1,
            status,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        );
    }
}

/// RAVEN ignites: a white-hot wave sweeps the letters left→right, each cell
/// heating through ░▒▓█ and cooling into the theme violet.
fn draw_logo(cv: &mut Canvas, t: f64, x0: i64, y0: i64) {
    let sweep = LOGO.1 - LOGO.0 - 0.45; // wave travel time; 0.45s = per-cell heat-up
    let flash = t >= FLASH.0 && t < FLASH.1;
    let settled = t >= FLASH.1;

    for (li, lc) in "RAVEN".chars().enumerate() {
        let rows = glyph(lc);
        let lx = x0 + (li * (LETTER_W + LETTER_GAP)) as i64;
        for (ry, row) in rows.iter().enumerate() {
            for (cx, cc) in row.chars().enumerate() {
                if cc != '#' {
                    continue;
                }
                for half in 0..2i64 {
                    let gx = (li * (LETTER_W + LETTER_GAP) + cx * 2) as i64 + half;
                    let jitter =
                        (hash(gx as u64, ry as u64) % 100) as f64 / 100.0 * 0.10;
                    let ignite = LOGO.0 + (gx as f64 / LOGO_W as f64) * sweep + jitter;
                    let age = t - ignite;
                    if age < 0.0 {
                        continue;
                    }
                    let (ch, heat) = if flash {
                        ('█', 1.0)
                    } else if settled {
                        ('█', 0.62)
                    } else if age < 0.10 {
                        ('░', 1.0)
                    } else if age < 0.20 {
                        ('▒', 0.95)
                    } else if age < 0.32 {
                        ('▓', 0.85)
                    } else {
                        // Cool from white-hot down to the resting violet.
                        ('█', (0.92 - (age - 0.32) * 0.55).max(0.62))
                    };
                    let style = Style::default().fg(ramp(heat));
                    cv.put(lx + (cx as i64 * 2) + half, y0 + ry as i64, '█', style);
                    // The freshly-ignited edge keeps its texture character.
                    if ch != '█' {
                        cv.put(
                            lx + (cx as i64 * 2) + half,
                            y0 + ry as i64,
                            ch,
                            Style::default().fg(ramp(heat)),
                        );
                    }
                }
            }
        }
    }

    // Sparks riding the wavefront.
    if t >= LOGO.0 && t < LOGO.1 + 0.2 {
        let wave_x = ((t - LOGO.0) / sweep * LOGO_W as f64) as i64;
        for k in 0..5u64 {
            let hv = hash(k, (t * 24.0) as u64);
            // Clamp to the die interior so sparks never land on the border.
            let sx = (x0 + wave_x + (hv % 9) as i64 - 4)
                .clamp(x0 - 2, x0 + LOGO_W as i64 + 1);
            let sy = y0 - 1 + (hv >> 4) as i64 % (LOGO_ROWS as i64 + 2);
            let c = ['·', '✦', '✧', '*'][(hv >> 9) as usize % 4];
            cv.put(sx, sy, c, Style::default().fg(ramp(0.9 + 0.1 * (k as f64 / 5.0))));
        }
    }
}

fn draw_subtitle(cv: &mut Canvas, t: f64, die_x: i64, y: i64, mem_size: usize) {
    let text = format!(
        "R I S C - V   ·   R V 3 2 I M F   ·   {}",
        format_mem(mem_size)
    );
    let p = phase(t, SUBTITLE);
    if p <= 0.0 {
        return;
    }
    let shown = (p * text.chars().count() as f64) as usize;
    let x = die_x + (DIE_W as i64 - text.chars().count() as i64) / 2;
    for (i, c) in text.chars().take(shown).enumerate() {
        let style = if i + 1 == shown && p < 1.0 {
            Style::default().fg(ramp(1.0)).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::ACTIVE)
        };
        cv.put(x + i as i64, y, c, style);
    }
}

/// The five pipeline stage chips pop online one by one, then a pulse laps them.
fn draw_stages(cv: &mut Canvas, t: f64, die_x: i64, y: i64) {
    // The chips live inside the die — nothing to show before its frame closes.
    if t < FRAME.1 {
        return;
    }
    const CHIPS: [&str; 5] = ["[IF]", "[ID]", "[EX]", "[MEM]", "[WB]"];
    let total_w: usize = CHIPS.iter().map(|c| c.len()).sum::<usize>() + 4; // '─' joints
    let mut x = die_x + (DIE_W as i64 - total_w as i64) / 2;
    let lap = ((t - STAGES_T0) * 4.0) as usize % 5;
    for (i, chip) in CHIPS.iter().enumerate() {
        let on = t >= STAGES_T0 + i as f64 * 0.16;
        let fresh = on && t < STAGES_T0 + i as f64 * 0.16 + 0.14;
        let all_on = t >= STAGES_T0 + 5.0 * 0.16;
        let style = if fresh || (all_on && lap == i) {
            Style::default().fg(ramp(1.0)).add_modifier(Modifier::BOLD)
        } else if on {
            Style::default().fg(theme::RUNNING).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::BORDER)
        };
        cv.text(x, y, chip, style);
        x += chip.len() as i64;
        if i < 4 {
            let joint_on = t >= STAGES_T0 + (i + 1) as f64 * 0.16;
            cv.put(
                x,
                y,
                '─',
                Style::default().fg(if joint_on { theme::RUNNING } else { theme::BORDER }),
            );
            x += 1;
        }
    }
}

/// POST log: each line types itself, then its status word stamps in colour.
fn draw_log(cv: &mut Canvas, t: f64, die_x: i64, y0: i64) {
    const LOG: [(&str, &str, &str); LOG_LINES] = [
        ("[0.000s] ", "power rails ............ ", "1.2V OK"),
        ("[0.412s] ", "clock tree ............. ", "LOCKED"),
        ("[0.973s] ", "register file x32 ...... ", "ONLINE"),
        ("[1.508s] ", "L1 i-cache / d-cache ... ", "WARM"),
        ("[2.144s] ", "MMU / Sv32 / TLB ....... ", "READY"),
        ("[2.700s] ", "pipeline IF -> WB ...... ", "PRIMED"),
    ];
    let width = LOG
        .iter()
        .map(|(a, b, c)| a.len() + b.len() + c.len())
        .max()
        .unwrap_or(0);
    let x0 = die_x + (DIE_W as i64 - width as i64) / 2;

    for (i, (ts, label, status)) in LOG.iter().enumerate() {
        let start = LOG_T0 + i as f64 * 0.34;
        if t < start {
            break;
        }
        let body: String = format!("{ts}{label}");
        let typed = (((t - start) * 140.0) as usize).min(body.len());
        let y = y0 + i as i64;
        for (j, c) in body.chars().take(typed).enumerate() {
            let style = if j < ts.len() {
                Style::default().fg(theme::METRIC_CYC)
            } else if c == '.' {
                Style::default().fg(theme::BORDER)
            } else {
                Style::default().fg(theme::LABEL)
            };
            cv.put(x0 + j as i64, y, c, style);
        }
        if typed == body.len() && t >= start + 0.30 {
            let color = match *status {
                "1.2V OK" | "ONLINE" | "PRIMED" => theme::RUNNING,
                "LOCKED" => theme::METRIC_CYC,
                "WARM" => theme::CACHE_I,
                "READY" => theme::ACCENT,
                _ => theme::TEXT,
            };
            cv.text(
                x0 + body.len() as i64,
                y,
                status,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            );
        }
    }
}

/// Power bar: a violet gradient charges to 100 %, then reads CORE ONLINE.
fn draw_bar(cv: &mut Canvas, t: f64, die_x: i64, y: i64) {
    let p = phase(t, BAR);
    if p <= 0.0 {
        return;
    }
    let inner = DIE_W - 8;
    let filled = (p * inner as f64) as usize;
    cv.put(die_x, y, '▐', Style::default().fg(theme::BORDER));
    for i in 0..inner {
        let (c, s) = if i < filled {
            ('█', Style::default().fg(ramp(0.30 + 0.62 * (i as f64 / inner as f64))))
        } else {
            ('·', Style::default().fg(theme::BORDER))
        };
        cv.put(die_x + 1 + i as i64, y, c, s);
    }
    cv.put(die_x + 1 + inner as i64, y, '▌', Style::default().fg(theme::BORDER));

    if p >= 1.0 {
        let label = " CORE ONLINE ";
        let style = Style::default()
            .fg(if t >= FLASH.0 && t < FLASH.1 { ramp(1.0) } else { theme::RUNNING })
            .add_modifier(Modifier::BOLD);
        cv.text(die_x + (DIE_W as i64 - label.len() as i64) / 2, y, label, style);
    } else {
        let pct = format!(" {:>3}% ", (p * 100.0).floor() as u32);
        cv.text(
            die_x + DIE_W as i64 - 2 - pct.len() as i64,
            y,
            &pct,
            Style::default().fg(theme::ACTIVE),
        );
    }
}

// ── Compact fallback for small terminals ─────────────────────────────────────

fn draw_compact(cv: &mut Canvas, t: f64, mem_size: usize) {
    let word = "R A V E N";
    let cy = cv.h as i64 / 2 - 1;
    let x0 = (cv.w as i64 - word.len() as i64) / 2;
    let sweep = 1.6;
    for (i, c) in word.chars().enumerate() {
        let ignite = 0.3 + (i as f64 / word.len() as f64) * sweep;
        if t < ignite {
            continue;
        }
        let heat = (1.0 - (t - ignite) * 0.4).max(0.65);
        cv.put(
            x0 + i as i64,
            cy,
            c,
            Style::default().fg(ramp(heat)).add_modifier(Modifier::BOLD),
        );
    }
    let sub = format!("RISC-V · RV32IMF · {}", format_mem(mem_size));
    if t > 2.2 {
        cv.text(
            (cv.w as i64 - sub.chars().count() as i64) / 2,
            cy + 2,
            &sub,
            Style::default().fg(theme::ACTIVE),
        );
    }
    // Mini power bar.
    let p = phase(t, BAR);
    let bw = (cv.w / 2).max(10);
    let bx = (cv.w as i64 - bw as i64) / 2;
    let filled = (p * bw as f64) as usize;
    for i in 0..bw {
        let (c, s) = if i < filled {
            ('█', Style::default().fg(ramp(0.3 + 0.6 * (i as f64 / bw as f64))))
        } else {
            ('·', Style::default().fg(theme::BORDER))
        };
        cv.put(bx + i as i64, cy + 4, c, s);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn render_at(t: f64, w: u16, h: u16) -> ratatui::buffer::Buffer {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render_splash_frame(f, t, 16 * 1024 * 1024))
            .unwrap();
        term.backend().buffer().clone()
    }

    fn buffer_text(buf: &ratatui::buffer::Buffer) -> String {
        let area = *buf.area();
        let mut s = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                s.push_str(buf.cell((x, y)).unwrap().symbol());
            }
            s.push('\n');
        }
        s
    }

    #[test]
    #[ignore]
    fn splash_dump_frames() {
        for &t in &[0.4, 1.0, 1.6, 2.3, 3.0, 3.7, 4.3, 5.0] {
            println!("
======== t = {t:.1}s ========");
            println!("{}", buffer_text(&render_at(t, 96, 26)));
        }
    }

    #[test]
    fn splash_renders_every_phase_without_panicking() {
        for &t in &[0.0, 0.3, 0.9, 1.5, 2.1, 2.8, 3.4, 4.0, 4.7, 5.1, 9.0] {
            render_at(t, 100, 30);
            render_at(t, 60, 18); // compact fallback
            render_at(t, 7, 3); // degenerate
        }
    }

    #[test]
    fn splash_finale_reports_core_online() {
        let text = buffer_text(&render_at(5.0, 100, 30));
        assert!(text.contains("CORE ONLINE"), "finale should read CORE ONLINE");
        assert!(text.contains("RAVEN CORE"), "die should be titled RAVEN CORE");
        assert!(text.contains("[IF]"), "stage chips should be online");
        assert!(text.contains("PRIMED"), "POST log should be complete");
    }

    #[test]
    fn splash_logo_is_fully_lit_after_the_sweep() {
        let buf = render_at(4.2, 100, 30);
        let text = buffer_text(&buf);
        // The block letters render as solid █ runs once ignited.
        assert!(text.contains("██"), "logo cells should be lit");
    }
}
