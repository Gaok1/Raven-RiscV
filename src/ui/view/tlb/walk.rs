// ui/view/tlb/walk.rs — one translation, step by step, over the recent ones.
//
//   ╭─ Access #918 ──────────────────────────────────── following data ─╮
//   │ ◀ older   newer ▶   follow  data  fetch  any                      │
//   │ lw a0, 8(sp)    load · priv M · pc 0x00400010                      │
//   │ VA 0x7fffeff8   VPN[1] 0x1ff    VPN[0] 0x3fe    offset 0xff8        │
//   │                 0111111111      1111111110      111111111000        │
//   │  1  TLB      set 6 (VPN 0x7fffe mod 8) · no way matches       miss  │
//   │  2  root     satp → table at 0x00ffc000                             │
//   │  3  VPN[1]   PTE[0x1ff] @ 0x00ffc7fc = 0x003fe801  pointer → …  RAM +12 │
//   │  4  VPN[0]   PTE[0x3fe] @ 0x00ffaff8 = 0x000100d7  leaf · 4 KiB …  L1 +1 │
//   │  5  PA       0x00040ff8 = frame 0x00040000 + offset 0xff8  +33 cyc  │
//   │  6  TLB ←    set 6 · way 1 · evicted VPN 0x00406                    │
//   ╰─────────────────────────────────────────────────────────────────────╯
//   ╭─ History ───────────────────────────────────────────── newest first ─╮
//
// Each field of the address keeps one colour throughout: the VPN[1] bits, the
// root PTE they index, and so on; the offset is the same teal in VA and PA.
// Each PTE read ends with where it was answered and what it cost: the walker
// reads through the D-cache like any load.

use ratatui::{Frame, prelude::*, widgets::Paragraph};
use raven_engine::capability::{
    AddressTranslation, TableEntryKind, TranslationOutcome, TranslationRecordView, TranslationScheme,
    WalkStepView,
};
use unicode_truncate::UnicodeTruncateStr;

use super::{
    chip, hex, history, level_color, outcome_badge, perms_text, place, size_long, size_short,
};
use crate::ui::app::{App, HistoryFilter, VmHit, WalkFollow};
use crate::ui::theme;
use crate::ui::view::components::Toolbar;
use crate::ui::view::components::panel::{self, PanelKind, render_panel};
use crate::ui::view::style;

/// Columns of the step label (`VPN[1]`, `TLB ←`).
const LABEL_W: usize = 9;
/// History rows worth showing below the access; fewer and it is dropped.
const MIN_HISTORY: u16 = 5;

pub(super) fn render(f: &mut Frame, area: Rect, app: &App) {
    let Some(translation) = app.translation() else {
        return;
    };
    let records = app.vm_records();
    let current = app.vm_current();
    let scheme = translation.scheme();

    // Narrow or short screens get one line per fact and no spacer rows.
    let inner_w = area.width.saturating_sub(2);
    let compact = inner_w < 100 || area.height < 24;
    let steps = current
        .as_ref()
        .map(|rec| step_lines(app, &scheme, rec, inner_w, compact))
        .unwrap_or_default();
    let head_rows: u16 = if compact { 4 } else { 7 };
    let wanted = if current.is_some() {
        head_rows + steps.len() as u16 + 2
    } else if records.is_empty() {
        // Nothing to list below: the empty state gets the room to explain.
        area.height
    } else {
        7
    };
    let access_h = wanted.min(area.height);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(access_h), Constraint::Min(0)])
        .split(area);

    render_access(f, layout[0], app, current.as_ref(), &scheme, steps, compact);
    if layout[1].height >= MIN_HISTORY {
        render_history(f, layout[1], app, &records, current.as_ref());
    }
}

// ── The access ───────────────────────────────────────────────────────────────

fn render_access(
    f: &mut Frame,
    area: Rect,
    app: &App,
    current: Option<&TranslationRecordView>,
    scheme: &TranslationScheme,
    steps: Vec<Line<'static>>,
    compact: bool,
) {
    let title = match current {
        Some(rec) if rec.seq > 0 => super::core_title(app, &format!("Access #{}", rec.seq)),
        _ => super::core_title(app, "Access"),
    };
    let state = if app.tlb.pinned.is_some() {
        "pinned · g follows again".to_string()
    } else if current.is_some() {
        format!("following {}", app.tlb.follow.label())
    } else {
        String::new()
    };
    let inner = render_panel(f, area, panel::panel_state(title, state, PanelKind::Plain));
    if inner.height == 0 || inner.width < 10 {
        return;
    }

    let Some(rec) = current else {
        super::render_empty(
            f,
            inner,
            app,
            "No translation yet — step or run the program to see one.",
        );
        return;
    };

    let mut lines: Vec<Line<'static>> = vec![controls_line(app, inner)];
    if !compact {
        lines.push(Line::raw(""));
    }
    lines.push(instruction_line(app, rec));
    if !compact {
        lines.push(Line::raw(""));
    }
    lines.extend(address_lines(scheme, rec.virtual_address, !compact));
    lines.push(Line::raw(""));
    lines.extend(steps);

    f.render_widget(Paragraph::new(lines), inner);
}

/// `◀ older   newer ▶   follow  data  fetch  any`
fn controls_line(app: &App, inner: Rect) -> Line<'static> {
    let mut bar = Toolbar::new();
    bar.action(
        VmHit::Older,
        "◀ older",
        chip(app, VmHit::Older, false),
        theme::TEXT,
    )
    .action(
        VmHit::Newer,
        "newer ▶",
        chip(app, VmHit::Newer, false),
        theme::TEXT,
    )
    .separator();
    bar.text(vec![Span::styled("follow", style::idle())]);
    for follow in WalkFollow::ALL {
        let hit = VmHit::Follow(follow);
        bar.value(
            hit,
            follow.label(),
            chip(app, hit, app.tlb.pinned.is_none() && app.tlb.follow == follow),
            theme::ACCENT,
        );
    }
    let mut spans = vec![Span::raw(" ")];
    spans.extend(place(app, &bar, inner.x + 1, inner.y, |id| id));
    Line::from(spans)
}

/// The instruction this access belongs to, with who asked and how.
fn instruction_line(app: &App, rec: &TranslationRecordView) -> Line<'static> {
    let text = instruction_at(app, rec.pc);
    let mut meta = format!(
        "{} · priv {} · pc {}",
        rec.access.label(),
        rec.privilege,
        hex(rec.pc, 8)
    );
    if app.max_cores > 1 {
        meta.push_str(&format!(" · hart {}", rec.hart));
    }
    if let Some(asid) = rec.address_space {
        meta.push_str(&format!(" · ASID {asid}"));
    }
    Line::from(vec![
        Span::raw(" "),
        Span::styled(text, Style::default().fg(theme::TEXT).bold()),
        Span::raw("    "),
        Span::styled(meta, style::label()),
    ])
}

/// The instruction at virtual `pc`, read where the MMU would fetch it from.
pub(super) fn instruction_at(app: &App, pc: u64) -> String {
    let physical = app
        .translation()
        .and_then(|t| t.probe(pc).physical)
        .unwrap_or(pc);
    app.disassemble_at(physical).unwrap_or_else(|| "?".into())
}

/// `VA 0x7fffeff8   VPN[1] 0x1ff   VPN[0] 0x3fe   offset 0xff8`, and the bits
/// of each field underneath it.
fn address_lines(scheme: &TranslationScheme, va: u64, wide: bool) -> Vec<Line<'static>> {
    let mut fields: Vec<(String, u64, u8, Color)> = scheme
        .levels
        .iter()
        .enumerate()
        .map(|(i, level)| {
            (
                level.name.clone(),
                scheme.index_at(va, i),
                level.index_bits,
                level_color(i),
            )
        })
        .collect();
    fields.push((
        "offset".into(),
        scheme.offset_of(va),
        scheme.offset_bits,
        theme::VA_OFFSET,
    ));

    let head = format!(" VA {}   ", hex(va, scheme.virtual_hex_width()));
    let mut top = vec![Span::styled(head.clone(), Style::default().fg(theme::TEXT).bold())];
    let mut bottom = vec![Span::raw(" ".repeat(head.chars().count()))];
    for (name, value, bits, color) in fields {
        let label = format!("{name} {}", hex(value, usize::from(bits).div_ceil(4)));
        let binary = format!("{value:0width$b}", width = usize::from(bits));
        let width = label.chars().count().max(binary.chars().count()) + 4;
        top.push(Span::styled(
            format!("{label:<width$}"),
            Style::default().fg(color).bold(),
        ));
        bottom.push(Span::styled(
            format!("{binary:<width$}"),
            Style::default().fg(color),
        ));
    }
    if wide {
        vec![Line::from(top), Line::from(bottom)]
    } else {
        vec![Line::from(top)]
    }
}

/// A numbered step: ` 3  VPN[1]   <content>`.
fn step(n: &mut usize, label: &str, label_color: Color, content: Vec<Span<'static>>) -> Line<'static> {
    *n += 1;
    let mut spans = vec![
        Span::styled(format!(" {:>2}  ", *n), Style::default().fg(theme::BORDER)),
        Span::styled(
            format!("{label:<LABEL_W$}"),
            Style::default().fg(label_color).bold(),
        ),
    ];
    spans.extend(content);
    Line::from(spans)
}

/// Where a PTE read was answered — a cache level or RAM — and, for a
/// translation that ran, what it cost. A probe charges nothing, so it only
/// says where the read would go; with the caches off a read has no cost of
/// its own, and a `+0` would read as a bug.
fn read_cost(step: &WalkStepView) -> String {
    let source = step
        .served_by
        .map_or_else(|| "RAM".to_string(), |level| format!("L{level}"));
    match step.cycles {
        Some(0) | None if step.served_by.is_none() => source,
        Some(cycles) => format!("{source} +{cycles}"),
        None => format!("in {source}"),
    }
}

/// Pad `spans` to `width` columns and put `tail` flush right.
fn with_tail(mut spans: Vec<Span<'static>>, tail: Span<'static>, width: u16) -> Vec<Span<'static>> {
    let used: usize = spans.iter().map(|s| s.width()).sum::<usize>() + LABEL_W + 5;
    let gap = usize::from(width).saturating_sub(used + tail.width() + 1).max(2);
    spans.push(Span::raw(" ".repeat(gap)));
    spans.push(tail);
    spans
}

fn step_lines(
    app: &App,
    scheme: &TranslationScheme,
    rec: &TranslationRecordView,
    width: u16,
    compact: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut n = 0usize;
    let translation = app.translation();
    let ways = translation
        .as_ref()
        .map(|t| (t.tlb_len() / t.tlb_sets().max(1)).max(1))
        .unwrap_or(1);
    let sets = translation.map_or(1, |t| t.tlb_sets().max(1));
    let vpn = rec.virtual_address >> scheme.offset_bits;
    let vpn_digits = super::vpn_digits(scheme);
    let dim = || Style::default().fg(theme::BORDER);

    // ── 1. The TLB probe ──
    if rec.probed_sets.is_empty() {
        lines.push(step(
            &mut n,
            "TLB",
            theme::TEXT,
            vec![Span::styled(
                if compact {
                    "off — every access walks"
                } else {
                    "off — every translation walks the page table"
                },
                style::label(),
            )],
        ));
    } else {
        let (small, _) = rec.probed_sets[0];
        let mut content = vec![Span::styled(
            format!("set {small}"),
            Style::default().fg(theme::TEXT).bold(),
        )];
        if !compact {
            content.push(Span::styled(
                format!(" ({} mod {sets})", hex(vpn, vpn_digits)),
                style::label(),
            ));
            if let Some(&(mega, size)) = rec.probed_sets.get(1)
                && mega != small
                && width >= 120
            {
                content.push(Span::styled(
                    format!(" · set {mega} for {} pages", size_long(size)),
                    style::label(),
                ));
            }
        }
        let result = match (rec.hit_entry, rec.rewalk_for_dirty) {
            (Some(slot), false) => format!(" · way {} matches", slot % ways),
            (Some(slot), true) => format!(
                " · way {} matches a clean page → walk again to set D",
                slot % ways
            ),
            (None, _) if compact => " · no way matches".to_string(),
            (None, _) => format!(" · none of its {ways} ways matches"),
        };
        content.push(Span::styled(result, style::label()));
        // The probe's own result. A hit can still fault on permissions; the
        // fault line below says so.
        let badge = if rec.hit_entry.is_some() && !rec.rewalk_for_dirty {
            outcome_badge(TranslationOutcome::TlbHit)
        } else {
            outcome_badge(TranslationOutcome::Walked)
        };
        let tail = Span::styled(
            badge.0.to_uppercase(),
            Style::default().fg(badge.1).bold(),
        );
        lines.push(step(&mut n, "TLB", theme::TEXT, with_tail(content, tail, width)));
    }

    // ── 2… The walk ──
    if let Some(first) = rec.steps.first() {
        let root = first.entry.address - first.entry.index * u64::from(scheme.entry_bits / 8);
        lines.push(step(
            &mut n,
            "root",
            theme::TEXT,
            vec![
                Span::styled("satp → table at ", style::label()),
                Span::styled(hex(root, 8), style::value()),
            ],
        ));
    }
    for walk_step in &rec.steps {
        let level = walk_step.level;
        let name = scheme
            .levels
            .get(level)
            .map_or_else(|| format!("L{level}"), |l| l.name.clone());
        let color = level_color(level);
        let entry = &walk_step.entry;
        let index_digits = scheme
            .levels
            .get(level)
            .map_or(3, |l| usize::from(l.index_bits).div_ceil(4));
        let raw = hex(entry.raw, usize::from(scheme.entry_bits).div_ceil(4));
        let mut content = if compact {
            vec![
                Span::styled("[", dim()),
                Span::styled(hex(entry.index, index_digits), Style::default().fg(color).bold()),
                Span::styled("] ", dim()),
                Span::styled(raw, style::value()),
                Span::raw("  "),
            ]
        } else {
            vec![
                Span::styled("PTE[", dim()),
                Span::styled(hex(entry.index, index_digits), Style::default().fg(color).bold()),
                Span::styled("] @ ", dim()),
                Span::styled(hex(entry.address, 8), style::value()),
                Span::styled(" = ", dim()),
                Span::styled(raw, style::value()),
                Span::raw("   "),
            ]
        };
        match entry.kind {
            TableEntryKind::Invalid => content.push(Span::styled(
                if compact {
                    "V=0: unmapped"
                } else {
                    "V=0 — nothing is mapped here"
                },
                Style::default().fg(theme::DANGER).bold(),
            )),
            TableEntryKind::Reserved => content.push(Span::styled(
                "reserved encoding (W without R)",
                Style::default().fg(theme::DANGER).bold(),
            )),
            TableEntryKind::Pointer { table } => {
                content.push(Span::styled(
                    if compact { "→ table " } else { "pointer → table " },
                    style::label(),
                ));
                content.push(Span::styled(hex(table, 8), style::value()));
            }
            TableEntryKind::Leaf {
                permissions,
                accessed,
                dirty,
                ..
            } => {
                let size = scheme
                    .levels
                    .get(level)
                    .map_or(scheme.page_size(), |l| l.coverage);
                content.push(Span::styled(
                    if compact {
                        format!("leaf {} ", size_short(size))
                    } else {
                        format!("leaf · {} page · ", size_long(size))
                    },
                    style::label(),
                ));
                content.push(Span::styled(perms_text(permissions), style::value()));
                let mut bits = Vec::new();
                if accessed {
                    bits.push("A");
                }
                if dirty {
                    bits.push("D");
                }
                if !bits.is_empty() {
                    content.push(Span::styled(format!(" · {}", bits.join(" ")), style::label()));
                }
            }
        }
        let tail = Span::styled(read_cost(walk_step), style::label());
        lines.push(step(&mut n, &name, color, with_tail(content, tail, width)));
    }

    // ── The result ──
    if rec.outcome == TranslationOutcome::Fault {
        let code = rec
            .fault_code
            .map(|c| format!(" (cause {c})"))
            .unwrap_or_default();
        lines.push(step(
            &mut n,
            "fault",
            theme::DANGER,
            vec![Span::styled(
                format!(
                    "✗ {} page fault{code}: {}",
                    rec.access.label(),
                    rec.fault.unwrap_or("no valid mapping")
                ),
                Style::default().fg(theme::DANGER).bold(),
            )],
        ));
        return lines;
    }

    if let Some(pa) = rec.physical {
        let page = rec.page_size.max(1);
        let frame = pa & !(page - 1);
        let low = pa & (page - 1);
        let mut content = vec![
            Span::styled(hex(pa, 8), Style::default().fg(theme::TEXT).bold()),
            Span::styled(" = frame ", style::label()),
            Span::styled(hex(frame, 8), style::value()),
            Span::styled(" + ", style::label()),
        ];
        if page > scheme.page_size() {
            content.push(Span::styled(
                hex(low, (page.trailing_zeros() as usize).div_ceil(4)),
                Style::default().fg(theme::VA_OFFSET).bold(),
            ));
            if !compact {
                content.push(Span::styled(
                    format!(" ({} page: VPN[0] passes through too)", size_long(page)),
                    style::label(),
                ));
            }
        } else {
            content.push(Span::styled("offset ", style::label()));
            content.push(Span::styled(
                hex(low, usize::from(scheme.offset_bits).div_ceil(4)),
                Style::default().fg(theme::VA_OFFSET).bold(),
            ));
        }
        let plural = if rec.cycles == 1 { "" } else { "s" };
        let tail = Span::styled(format!("+{} cycle{plural}", rec.cycles), style::label());
        lines.push(step(&mut n, "PA", theme::TEXT, with_tail(content, tail, width)));
    }

    // ── What the TLB kept ──
    if let Some(slot) = rec.installed_entry {
        let mut content = vec![Span::styled(
            format!("set {} · way {}", slot / ways, slot % ways),
            Style::default().fg(theme::ACCENT).bold(),
        )];
        match rec.evicted {
            Some(old) => content.push(Span::styled(
                format!(" · evicted VPN {}", hex(old.virtual_page, vpn_digits)),
                style::label(),
            )),
            None if rec.rewalk_for_dirty => {
                content.push(Span::styled(" · same entry, now dirty", style::label()))
            }
            None => content.push(Span::styled(" · the way was free", style::label())),
        }
        let written = match (rec.set_accessed, rec.set_dirty, compact) {
            (_, _, true) | (false, false, _) => "",
            (true, true, _) => " · A and D set in the PTE",
            (true, false, _) => " · A set in the PTE",
            (false, true, _) => " · D set in the PTE",
        };
        content.push(Span::styled(written, style::label()));
        lines.push(step(&mut n, "TLB ←", theme::ACCENT, content));
    } else if rec.outcome == TranslationOutcome::Walked && rec.probed_sets.is_empty() {
        lines.push(step(
            &mut n,
            "TLB",
            theme::TEXT,
            vec![Span::styled("off — nothing kept for next time", style::label())],
        ));
    }
    lines
}

// ── History ──────────────────────────────────────────────────────────────────

fn render_history(
    f: &mut Frame,
    area: Rect,
    app: &App,
    records: &[TranslationRecordView],
    current: Option<&TranslationRecordView>,
) {
    let inner = render_panel(
        f,
        area,
        panel::panel_state("History", "newest first · click to inspect", PanelKind::Plain),
    );
    if inner.height < 2 || inner.width < 20 {
        return;
    }

    // Filter chips.
    let rows = history(app, records);
    let mut bar = Toolbar::new();
    for filter in HistoryFilter::ALL {
        let hit = VmHit::Filter(filter);
        bar.value(
            hit,
            filter.label(),
            chip(app, hit, app.tlb.filter == filter),
            theme::ACCENT,
        );
    }
    bar.separator();
    bar.text(vec![Span::styled(
        format!("{} of the last {} translations", rows.len(), records.len()),
        style::idle(),
    )]);
    let mut first = vec![Span::raw(" ")];
    first.extend(place(app, &bar, inner.x + 1, inner.y, |id| id));
    f.render_widget(
        Paragraph::new(Line::from(first)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    let list = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        inner.height.saturating_sub(1),
    );
    if list.height < 2 {
        return;
    }
    app.tlb.hit(VmHit::History, list);

    let show_hart = app.max_cores > 1;
    let instr_w = 20usize;
    let header = {
        let mut s = format!("  {:>5}  ", "#");
        if show_hart {
            s.push_str("hart ");
        }
        s.push_str(&format!(
            "{:<instr_w$}  {:<5}  {:<10}  {:<10}  result",
            "instruction", "kind", "VA", "PA"
        ));
        s
    };
    let mut lines = vec![Line::from(Span::styled(header, style::idle()))];

    let visible = usize::from(list.height - 1);
    let max_scroll = rows.len().saturating_sub(visible);
    let scroll = app.tlb.history_scroll.min(max_scroll);
    for (i, rec) in rows.iter().skip(scroll).take(visible).enumerate() {
        let y = list.y + 1 + i as u16;
        let row_rect = Rect::new(list.x, y, list.width, 1);
        let hit = VmHit::Record(rec.seq);
        app.tlb.hit(hit, row_rect);

        let selected = current.is_some_and(|c| c.seq == rec.seq);
        let (word, color) = outcome_badge(rec.outcome);
        let result = match rec.outcome {
            TranslationOutcome::Fault => format!(
                "{word} {}",
                rec.fault_code.map(|c| c.to_string()).unwrap_or_default()
            ),
            _ => format!("{word:<5} +{}", rec.cycles),
        };
        let instr = instruction_at(app, rec.pc);
        let instr = instr.unicode_truncate(instr_w).0.to_string();
        let mut spans = vec![
            Span::styled(
                if selected { "▸ " } else { "  " },
                Style::default().fg(theme::ACCENT),
            ),
            Span::styled(format!("{:>5}  ", rec.seq), Style::default().fg(theme::BORDER)),
        ];
        if show_hart {
            spans.push(Span::styled(format!("{:<5}", rec.hart), style::label()));
        }
        spans.extend([
            Span::styled(format!("{instr:<instr_w$}  "), style::value()),
            Span::styled(format!("{:<5}  ", rec.access.label()), style::label()),
            Span::styled(format!("{}  ", hex(rec.virtual_address, 8)), style::value()),
            Span::styled(
                format!(
                    "{:<10}  ",
                    rec.physical.map_or_else(|| "—".to_string(), |pa| hex(pa, 8))
                ),
                style::value(),
            ),
            Span::styled(result, Style::default().fg(color)),
        ]);
        let mut line = Line::from(spans);
        if selected {
            line = line.style(Style::default().bg(theme::SEL_ROW_BG));
        } else if app.tlb.is_hovered(hit) {
            line = line.style(Style::default().bg(theme::HOVER_ROW_BG));
        }
        lines.push(line);
    }
    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            match app.tlb.filter {
                HistoryFilter::All => "  nothing translated yet",
                HistoryFilter::Misses => "  no misses among the kept translations",
                HistoryFilter::Faults => "  no faults among the kept translations",
            },
            style::label(),
        )));
    }
    f.render_widget(Paragraph::new(lines), list);
}
