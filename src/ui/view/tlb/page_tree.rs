// ui/view/tlb/page_tree.rs — Live tree of the Sv32 page table.
//
// Reads the page table rooted at `satp.ppn` directly from RAM (read-only) and
// draws it as an indented tree: root → L1 PTE → L0 leaf (or L1 megapage leaf).
// Each leaf shows its virtual range, physical base, permissions and A/D bits.
// Entries currently cached in the TLB are marked, tying this view to Entries.
//
// The didactic identity map fills all 1024 root slots with identical megapages;
// runs of ≥4 identity megapages collapse into a single summary line so the tree
// stays readable. In Manual mode the program's real (sparse) table shows in full.

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

pub(super) fn render_page_tree(f: &mut Frame, area: Rect, app: &App) {
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

    let mmu = app.run.mem.mmu();

    // No active translation → nothing to walk.
    if !app.run.vm_enabled || mmu.satp.mode() != SatpMode::Sv32 {
        let msg = vec![
            Line::raw(""),
            Line::from(Span::styled(
                " No Sv32 page table to show.",
                Style::default().fg(theme::LABEL),
            )),
            Line::from(Span::styled(
                " Set VM to Didactic/Manual in Settings and have the program",
                Style::default().fg(theme::LABEL),
            )),
            Line::from(Span::styled(
                " write satp (csrw satp, <ppn|mode>) to install a root table.",
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
    let scroll = app.tlb.page_tree_scroll.min(max_scroll);
    let shown: Vec<Line> = lines.into_iter().skip(scroll).take(visible).collect();

    f.render_widget(Paragraph::new(shown), inner);
}

fn build_tree_lines(app: &App, mmu: &Mmu) -> Vec<Line<'static>> {
    let ram = app.run.mem.ram();
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
            format!("   (satp.ppn=0x{:x}, asid={})", mmu.satp.ppn(), asid),
            Style::default().fg(theme::BORDER),
        ),
    ]));

    let mut valid_seen = false;
    // Accumulator for a run of identity megapages, flushed when the run ends.
    let mut identity_run: Vec<(u32, u32)> = Vec::new(); // (vpn1, pa_base)

    let flush_identity = |lines: &mut Vec<Line<'static>>, run: &mut Vec<(u32, u32)>| {
        if run.is_empty() {
            return;
        }
        if run.len() >= IDENTITY_COLLAPSE_THRESHOLD {
            let first = run.first().unwrap().0;
            let last = run.last().unwrap().0;
            lines.push(Line::from(vec![
                Span::raw("  ├─ "),
                Span::styled(
                    format!("[{first}..={last}] "),
                    Style::default().fg(theme::BORDER),
                ),
                Span::styled(
                    format!("{} identity megapages", run.len()),
                    Style::default().fg(theme::IDLE),
                ),
                Span::styled(
                    "  VA==PA · RWXU  (didactic map)",
                    Style::default().fg(theme::IDLE),
                ),
            ]));
        } else {
            for &(vpn1, pa) in run.iter() {
                lines.push(megapage_line(vpn1, pa, PtePerms::identity(), false));
            }
        }
        run.clear();
    };

    for vpn1 in 0u32..1024 {
        let pte1_addr = root_pa.wrapping_add(vpn1 * 4);
        let pte1 = Pte::new(ram.load32(pte1_addr).unwrap_or(0));
        if !pte1.valid() {
            continue;
        }
        valid_seen = true;

        if pte1.is_leaf() {
            // L1 leaf → 4 MiB megapage.
            let va_base = vpn1 << 22;
            let pa_base = pte1.ppn() << 12;
            let p = pte1.perms();
            let is_identity = va_base == pa_base && p.r && p.w && p.x && p.u;
            if is_identity {
                identity_run.push((vpn1, pa_base));
            } else {
                flush_identity(&mut lines, &mut identity_run);
                let cached = megapage_in_tlb(mmu, va_base, asid);
                lines.push(megapage_line(
                    vpn1,
                    pa_base,
                    PtePerms::from(p, pte1.global(), pte1.accessed(), pte1.dirty()),
                    cached,
                ));
            }
            continue;
        }

        // L1 pointer → walk L0.
        flush_identity(&mut lines, &mut identity_run);
        let l0_pa = pte1.ppn() << 12;
        lines.push(Line::from(vec![
            Span::raw("  ├─ "),
            Span::styled(format!("L1[{vpn1}] "), Style::default().fg(theme::LABEL)),
            Span::styled("→ table @ ", Style::default().fg(theme::BORDER)),
            Span::styled(format!("0x{l0_pa:08x}"), Style::default().fg(theme::TEXT)),
        ]));

        for vpn0 in 0u32..1024 {
            let pte0_addr = l0_pa.wrapping_add(vpn0 * 4);
            let pte0 = Pte::new(ram.load32(pte0_addr).unwrap_or(0));
            if !pte0.valid() || !pte0.is_leaf() {
                continue;
            }
            let va = (vpn1 << 22) | (vpn0 << 12);
            let pa = pte0.ppn() << 12;
            let cached = page_in_tlb(mmu, va, asid);
            lines.push(leaf_line(
                vpn0,
                va,
                pa,
                PtePerms::from(pte0.perms(), pte0.global(), pte0.accessed(), pte0.dirty()),
                cached,
            ));
        }
    }
    flush_identity(&mut lines, &mut identity_run);

    if !valid_seen {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            " Root table has no valid entries yet.",
            Style::default().fg(theme::LABEL),
        )));
    }
    lines
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
    fn identity() -> Self {
        Self { r: true, w: true, x: true, u: true, g: false, a: false, d: false }
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

fn megapage_line(vpn1: u32, pa_base: u32, p: PtePerms, cached: bool) -> Line<'static> {
    let va_base = vpn1 << 22;
    let va_end = va_base.wrapping_add(0x003F_FFFF);
    let mut spans = vec![
        Span::raw("  ├─ "),
        Span::styled(format!("L1[{vpn1}] "), Style::default().fg(theme::LABEL)),
        Span::styled(
            format!("0x{va_base:08x}–0x{va_end:08x} "),
            Style::default().fg(theme::TEXT),
        ),
        Span::styled("→ ", Style::default().fg(theme::BORDER)),
        Span::styled(format!("0x{pa_base:08x} "), Style::default().fg(theme::TEXT)),
        Span::styled("[4M] ", Style::default().fg(theme::ACCENT)),
    ];
    spans.extend(perm_spans(&p));
    spans.push(cached_marker(cached));
    Line::from(spans)
}

fn leaf_line(vpn0: u32, va: u32, pa: u32, p: PtePerms, cached: bool) -> Line<'static> {
    let mut spans = vec![
        Span::raw("  │    └─ "),
        Span::styled(format!("L0[{vpn0}] "), Style::default().fg(theme::LABEL)),
        Span::styled(format!("0x{va:08x} "), Style::default().fg(theme::TEXT)),
        Span::styled("→ ", Style::default().fg(theme::BORDER)),
        Span::styled(format!("0x{pa:08x} "), Style::default().fg(theme::TEXT)),
    ];
    spans.extend(perm_spans(&p));
    spans.push(cached_marker(cached));
    Line::from(spans)
}

/// Is a 4 KiB page for `va` currently cached in the TLB?
fn page_in_tlb(mmu: &Mmu, va: u32, asid: u16) -> bool {
    let vpn = va >> 12;
    mmu.tlb.entries.iter().any(|e| {
        e.valid && !e.megapage && e.vpn == vpn && (e.global || e.asid == asid)
    })
}

/// Is the megapage covering `va` currently cached in the TLB?
fn megapage_in_tlb(mmu: &Mmu, va: u32, asid: u16) -> bool {
    let vpn1 = va >> 22;
    mmu.tlb.entries.iter().any(|e| {
        e.valid && e.megapage && (e.vpn >> 10) == vpn1 && (e.global || e.asid == asid)
    })
}
