// ui/view/tlb/page_tree.rs — Live tree of the (parametric) page table.
//
// Reads the page table rooted at `satp.ppn` directly from RAM (read-only) and
// draws it as an indented tree, walking the active [`PagingScheme`]'s N levels:
// each pointer PTE recurses into its child table; a leaf PTE at any level is a
// (super)page showing its virtual range, physical base, page size, permissions
// and A/D bits. Entries currently cached in the TLB are marked, tying this view
// to Entries.
//
// The didactic identity map fills a table's slots with uniform leaves; runs of
// ≥4 such leaves collapse into a single summary line so the tree stays readable.
// In Manual mode the program's real (sparse) table shows in full. Editing the
// map / scheme lives in the Virtual Memory → settings panel.

use ratatui::{
    Frame,
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::falcon::memory::Bus;
use crate::falcon::mmu::walker::Pte;
use crate::falcon::mmu::{Mmu, SatpMode};
use crate::ui::app::App;
use crate::ui::theme;

/// Minimum length of an identity-megapage run before it collapses to one line.
const IDENTITY_COLLAPSE_THRESHOLD: usize = 4;

/// The Tree subtab is read-only: it walks and renders the live page table.
/// Editing the map / scheme lives in the Virtual Memory → settings panel.
pub(super) fn render_page_tree(f: &mut Frame, area: Rect, app: &App) {
    render_tree(f, area, app);
}

fn render_tree(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled("Page Table Tree", Style::default().fg(theme::LABEL)));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let mmu = app.run.mem().mmu();

    // No active translation → nothing to walk.
    if !app.run.vm_enabled() || mmu.satp.mode() != SatpMode::Sv32 {
        let msg = vec![
            Line::raw(""),
            Line::from(Span::styled(
                " No Sv32 page table to show.",
                Style::default().fg(theme::LABEL),
            )),
            Line::from(Span::styled(
                " Pick Sv32/Custom/Manual in the settings subtab; in Manual mode",
                Style::default().fg(theme::LABEL),
            )),
            Line::from(Span::styled(
                " the program writes satp (csrw satp, <ppn|mode>) to install one.",
                Style::default().fg(theme::LABEL),
            )),
        ];
        f.render_widget(Paragraph::new(msg), inner);
        return;
    }

    let lines = build_tree_lines(app, mmu);

    let total = lines.len();
    let visible = inner.height as usize;
    let max_scroll = total.saturating_sub(visible);
    app.tlb.page_tree_max_scroll.set(max_scroll);
    let scroll = app.tlb.page_tree_scroll.min(max_scroll);
    let shown: Vec<Line> = lines.into_iter().skip(scroll).take(visible).collect();

    f.render_widget(Paragraph::new(shown), inner);
}

fn build_tree_lines(app: &App, mmu: &Mmu) -> Vec<Line<'static>> {
    let ram = app.run.mem().ram();
    let scheme = &mmu.scheme;
    let root_pa = mmu.satp.ppn() << 12;
    let asid = mmu.satp.asid();

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("root @ ", Style::default().fg(theme::LABEL)),
        Span::styled(
            format!("0x{root_pa:08x}"),
            Style::default().fg(theme::ACCENT).bold(),
        ),
        Span::styled(
            format!(
                "   (satp.ppn=0x{:x}, asid={}, {} levels)",
                mmu.satp.ppn(),
                asid,
                scheme.num_levels()
            ),
            Style::default().fg(theme::BORDER),
        ),
    ]));

    let mut valid_seen = false;
    walk_table(&mut lines, ram, scheme, mmu, root_pa, 0, 0, asid, &mut valid_seen);

    if !valid_seen {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " Root table has no valid entries yet.",
            Style::default().fg(theme::LABEL),
        )));
    }
    lines
}

/// Recursively render the page table at `table_pa` for `level` (0 = root),
/// where `va_base` is the virtual address this table covers. Leaves at any
/// level produce (super)pages; runs of uniform leaves collapse to one line.
#[allow(clippy::too_many_arguments)]
fn walk_table(
    lines: &mut Vec<Line<'static>>,
    ram: &crate::falcon::memory::Ram,
    scheme: &crate::falcon::mmu::PagingScheme,
    mmu: &Mmu,
    table_pa: u32,
    level: usize,
    va_base: u32,
    asid: u16,
    valid_seen: &mut bool,
) {
    let n = scheme.num_levels();
    let is_last = level + 1 == n;
    let shift = scheme.shift_at(level); // page_bits of a leaf at this level
    let entries = scheme.entries_at(level);
    let offset_bits = scheme.offset_bits as u32;
    let mask_bits = (shift - offset_bits) as u8;
    let size = size_short(1u64 << shift);
    let indent = "  ".repeat(level + 1);

    // Run accumulator: uniform consecutive leaves (same PA−VA delta + perms)
    // collapse to one summary line; short runs render individually.
    type LeafSig = (i64, (bool, bool, bool, bool));
    // (idx, va, pa, perms, cached)
    let mut run: Vec<(u32, u32, u32, PtePerms, bool)> = Vec::new();
    let mut run_sig: Option<LeafSig> = None;

    let flush = |lines: &mut Vec<Line<'static>>,
                 run: &mut Vec<(u32, u32, u32, PtePerms, bool)>,
                 sig: &mut Option<LeafSig>| {
        if run.is_empty() {
            *sig = None;
            return;
        }
        if run.len() >= IDENTITY_COLLAPSE_THRESHOLD {
            let first = run.first().unwrap();
            let last = run.last().unwrap();
            let (delta, (r, w, x, u)) = sig.unwrap_or((0, (true, true, true, true)));
            let map_desc = if delta == 0 {
                "VA==PA".to_string()
            } else {
                format!("PA=VA{:+} MiB", delta / (1024 * 1024))
            };
            let perms = format!(
                "{}{}{}{}",
                if r { 'R' } else { '-' },
                if w { 'W' } else { '-' },
                if x { 'X' } else { '-' },
                if u { 'U' } else { '-' },
            );
            let va_lo = first.1;
            let va_hi = last.1.wrapping_add((1u32 << shift).wrapping_sub(1));
            lines.push(Line::from(vec![
                Span::raw(format!("{indent}├─ ")),
                Span::styled(
                    format!("0x{va_lo:08x}–0x{va_hi:08x} "),
                    Style::default().fg(theme::BORDER),
                ),
                Span::styled(
                    format!("{} ×{size} pages", run.len()),
                    Style::default().fg(theme::IDLE),
                ),
                Span::styled(
                    format!("  {map_desc} · {perms}"),
                    Style::default().fg(theme::IDLE),
                ),
            ]));
        } else {
            for (idx, va, pa, perms, cached) in run.drain(..) {
                lines.push(leaf_line(&indent, level, idx, va, pa, &size, perms, cached));
            }
        }
        run.clear();
        *sig = None;
    };

    for idx in 0u32..entries {
        let pte_addr = table_pa.wrapping_add(idx * 4);
        let pte = Pte::new(ram.load32(pte_addr).unwrap_or(0));
        if !pte.valid() {
            continue;
        }
        *valid_seen = true;
        let va = va_base | (idx << shift);

        if pte.is_leaf() || is_last {
            let pa_base = pte.ppn() << 12;
            let p = pte.perms();
            let sig: LeafSig = (pa_base as i64 - va as i64, (p.r, p.w, p.x, p.u));
            if run_sig != Some(sig) {
                flush(lines, &mut run, &mut run_sig);
                run_sig = Some(sig);
            }
            let cached = leaf_in_tlb(mmu, va, offset_bits, mask_bits, asid);
            run.push((
                idx,
                va,
                pa_base,
                PtePerms::from(p, pte.global(), pte.accessed(), pte.dirty()),
                cached,
            ));
            continue;
        }

        // Pointer PTE → recurse into the child table.
        flush(lines, &mut run, &mut run_sig);
        let child_pa = pte.ppn() << 12;
        lines.push(Line::from(vec![
            Span::raw(format!("{indent}├─ ")),
            Span::styled(format!("L{level}[{idx}] "), Style::default().fg(theme::LABEL)),
            Span::styled("→ table @ ", Style::default().fg(theme::BORDER)),
            Span::styled(format!("0x{child_pa:08x}"), Style::default().fg(theme::TEXT)),
        ]));
        walk_table(lines, ram, scheme, mmu, child_pa, level + 1, va, asid, valid_seen);
    }
    flush(lines, &mut run, &mut run_sig);
}

/// Short byte-size label for a page: "4K", "4M", "1G", …
fn size_short(bytes: u64) -> String {
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

/// Permission/flag bundle for rendering a leaf row.
struct PtePerms {
    r: bool,
    w: bool,
    x: bool,
    u: bool,
    g: bool,
    a: bool,
    d: bool,
}

impl PtePerms {
    fn from(p: crate::falcon::mmu::PtePerms, g: bool, a: bool, d: bool) -> Self {
        Self { r: p.r, w: p.w, x: p.x, u: p.u, g, a, d }
    }
}

fn perm_spans(p: &PtePerms) -> Vec<Span<'static>> {
    let mark = |on: bool, c: char| {
        if on {
            Span::styled(c.to_string(), Style::default().fg(theme::RUNNING))
        } else {
            Span::styled("-".to_string(), Style::default().fg(theme::IDLE))
        }
    };
    vec![
        mark(p.r, 'R'),
        mark(p.w, 'W'),
        mark(p.x, 'X'),
        mark(p.u, 'U'),
        Span::raw(" "),
        mark(p.g, 'G'),
        mark(p.a, 'A'),
        mark(p.d, 'D'),
    ]
}

fn cached_marker(cached: bool) -> Span<'static> {
    if cached {
        Span::styled(" ●TLB", Style::default().fg(theme::RUNNING))
    } else {
        Span::raw("")
    }
}

/// Render a single (super)page leaf row at `level`, indented by `indent`.
#[allow(clippy::too_many_arguments)]
fn leaf_line(
    indent: &str,
    level: usize,
    idx: u32,
    va: u32,
    pa: u32,
    size: &str,
    p: PtePerms,
    cached: bool,
) -> Line<'static> {
    let mut spans = vec![
        Span::raw(format!("{indent}└─ ")),
        Span::styled(format!("L{level}[{idx}] "), Style::default().fg(theme::LABEL)),
        Span::styled(format!("0x{va:08x} "), Style::default().fg(theme::TEXT)),
        Span::styled("→ ", Style::default().fg(theme::BORDER)),
        Span::styled(format!("0x{pa:08x} "), Style::default().fg(theme::TEXT)),
        Span::styled(format!("[{size}] "), Style::default().fg(theme::ACCENT)),
    ];
    spans.extend(perm_spans(&p));
    spans.push(cached_marker(cached));
    Line::from(spans)
}

/// Is the (super)page of size `mask_bits` covering `va` cached in the TLB?
fn leaf_in_tlb(mmu: &Mmu, va: u32, offset_bits: u32, mask_bits: u8, asid: u16) -> bool {
    let vpn = va >> offset_bits;
    mmu.tlb.entries.iter().any(|e| {
        e.valid
            && e.mask_bits == mask_bits
            && (e.vpn >> mask_bits) == (vpn >> mask_bits)
            && (e.global || e.asid == asid)
    })
}
