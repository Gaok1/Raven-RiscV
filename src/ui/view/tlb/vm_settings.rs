// ui/view/tlb/vm_settings.rs — the comprehensive Virtual Memory control panel.
//
// One centered column, mirroring the Cache tab's settings: VM mode, TLB on/off,
// the parametric paging scheme (Custom mode), the auto-installed page map
// (kind / offset / perms / G / ASID), a read-only root-PT + window readout and
// the TLB geometry. An `apply` button installs the map + scheme and re-points
// satp; `flush tlb` drops cached translations.

use ratatui::{
    Frame,
    prelude::*,
    widgets::Paragraph,
};

use crate::falcon::mmu::{MapKind, VmMode};
use crate::ui::app::{App, TlbHoverTarget, VmSettingsField};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind};
use crate::ui::view::components::{dense_action, dense_value};

/// What a hitbox in the panel points at.
#[derive(Clone)]
enum Hit {
    Field(VmSettingsField),
    Apply,
    Flush,
}

struct RowBuilder {
    x: u16,
    spans: Vec<Span<'static>>,
    hits: Vec<(Hit, u16, u16)>,
}

impl RowBuilder {
    fn new(x: u16) -> Self {
        Self { x, spans: Vec::new(), hits: Vec::new() }
    }
    fn raw(&mut self, s: &str) {
        self.x += s.chars().count() as u16;
        self.spans.push(Span::raw(s.to_string()));
    }
    fn styled(&mut self, s: &str, style: Style) {
        self.x += s.chars().count() as u16;
        self.spans.push(Span::styled(s.to_string(), style));
    }
    /// Push a span and record a hitbox spanning exactly its width.
    fn hit(&mut self, hit: Hit, span: Span<'static>) {
        let w = span.content.chars().count() as u16;
        let x0 = self.x;
        self.x += w;
        self.hits.push((hit, x0, self.x));
        self.spans.push(span);
    }
}

pub(super) fn render_vm_settings(f: &mut Frame, area: Rect, app: &App) {
    let col_w = area.width.min(66);
    let col_x = area.x + (area.width.saturating_sub(col_w)) / 2;
    let col_area = Rect::new(col_x, area.y, col_w, area.height);

    let block = panel::panel("Virtual Memory Settings", PanelKind::Accent);
    let inner = block.inner(col_area);
    f.render_widget(block, col_area);
    if inner.height == 0 || inner.width < 4 {
        return;
    }

    let rows = build_rows(app, inner.x + 1);

    // Scroll the (possibly tall) panel.
    let visible = inner.height as usize;
    let max_scroll = rows.len().saturating_sub(visible);
    app.tlb.vm_settings_max_scroll.set(max_scroll);
    let scroll = app.tlb.vm_settings_scroll.min(max_scroll);

    // Register hitboxes for the visible rows at their real screen y.
    let mut field_hits: Vec<(VmSettingsField, u16, u16, u16)> = Vec::new();
    app.tlb.vm_apply_btn.set((0, 0, 0));
    app.tlb.vm_flush_btn.set((0, 0, 0));
    let mut lines: Vec<Line> = Vec::with_capacity(visible);
    for (i, row) in rows.iter().enumerate().skip(scroll).take(visible) {
        let y = inner.y + (i - scroll) as u16;
        for (hit, x0, x1) in &row.hits {
            match hit {
                Hit::Field(field) => field_hits.push((*field, y, *x0, *x1)),
                Hit::Apply => app.tlb.vm_apply_btn.set((y, *x0, *x1)),
                Hit::Flush => app.tlb.vm_flush_btn.set((y, *x0, *x1)),
            }
        }
        lines.push(Line::from(row.spans.clone()));
    }
    *app.tlb.vm_field_hitboxes.borrow_mut() = field_hits;

    f.render_widget(Paragraph::new(lines), inner);
}

fn build_rows(app: &App, x0: u16) -> Vec<RowBuilder> {
    let mode = app.vm_mode();
    let custom = matches!(mode, VmMode::Custom);
    let auto = mode.is_auto();
    let editing = |field: VmSettingsField| app.tlb.vm_edit_field == Some(field);
    let hov = |field: VmSettingsField| {
        matches!(&app.tlb.hover, Some(TlbHoverTarget::VmField(f)) if *f == field)
    };
    let edit_buf = app.tlb.vm_edit_buf.as_str();

    let label = Style::default().fg(theme::LABEL);
    let dim = Style::default().fg(theme::BORDER);
    let head = Style::default().fg(theme::ACCENT);

    // Render a numeric field's value (edit cursor when focused).
    let num_val = |field: VmSettingsField, value: String| -> Span<'static> {
        if editing(field) {
            Span::styled(format!("{edit_buf}█"), Style::default().fg(theme::ACCENT).bold())
        } else {
            dense_value(&value, hov(field), true, theme::LABEL_Y)
        }
    };

    let mut rows: Vec<RowBuilder> = Vec::new();

    // ── Mode + TLB ───────────────────────────────────────────────────────────
    {
        let mut r = RowBuilder::new(x0);
        r.styled("Mode         ", label);
        r.hit(
            Hit::Field(VmSettingsField::Mode),
            dense_value(&format!("< {} >", mode.as_str()), hov(VmSettingsField::Mode), true, theme::TEXT),
        );
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.styled("TLB cache    ", label);
        let on = app.run.tlb_enabled;
        r.hit(
            Hit::Field(VmSettingsField::TlbEnabled),
            dense_value(if on { "[on]" } else { "[off]" }, hov(VmSettingsField::TlbEnabled), on, theme::RUNNING),
        );
        rows.push(r);
    }

    rows.push(blank());

    // ── Paging scheme ──────────────────────────────────────────────────────
    {
        let mut r = RowBuilder::new(x0);
        let title = if custom {
            "Paging scheme (editable)"
        } else {
            "Paging scheme (preset)"
        };
        r.styled(title, head);
        rows.push(r);
    }
    let scheme = if custom {
        app.tlb.pending_scheme.clone()
    } else {
        crate::falcon::mmu::PagingScheme::sv32()
    };
    {
        let mut r = RowBuilder::new(x0);
        r.styled("  offset bits  ", label);
        if custom {
            r.hit(
                Hit::Field(VmSettingsField::OffsetBits),
                num_val(VmSettingsField::OffsetBits, scheme.offset_bits.to_string()),
            );
        } else {
            r.styled(&scheme.offset_bits.to_string(), dim);
        }
        rows.push(r);
    }
    for (i, &bits) in scheme.level_bits.iter().enumerate() {
        let mut r = RowBuilder::new(x0);
        r.styled(&format!("  L{i} index    "), label);
        let field = VmSettingsField::LevelBits(i);
        if custom {
            r.hit(Hit::Field(field), num_val(field, bits.to_string()));
        } else {
            r.styled(&bits.to_string(), dim);
        }
        rows.push(r);
    }
    if custom {
        let mut r = RowBuilder::new(x0);
        r.styled("  levels       ", label);
        r.hit(
            Hit::Field(VmSettingsField::AddLevel),
            dense_action("[+]", theme::RUNNING, hov(VmSettingsField::AddLevel)),
        );
        r.raw(" ");
        r.hit(
            Hit::Field(VmSettingsField::RemoveLevel),
            dense_action("[-]", theme::DANGER, hov(VmSettingsField::RemoveLevel)),
        );
        rows.push(r);
    }
    {
        // Readout: page size / depth / Σ validity.
        let mut r = RowBuilder::new(x0);
        let ok = scheme.is_valid();
        let page_bytes = 1u64 << scheme.offset_bits;
        let page = human_bytes(page_bytes);
        let sum = scheme.total_bits();
        let sum_style = if ok {
            Style::default().fg(theme::RUNNING)
        } else {
            Style::default().fg(theme::DANGER)
        };
        r.styled(
            &format!("  → page {page} · {} levels · ", scheme.num_levels()),
            dim,
        );
        r.styled(
            &format!("Σ={sum} {}", if ok { "✓" } else { "✗ (must be 32)" }),
            sum_style,
        );
        rows.push(r);
    }

    rows.push(blank());

    // ── Page map ──────────────────────────────────────────────────────────
    {
        let mut r = RowBuilder::new(x0);
        let t = if auto {
            "Page map (auto-installed)"
        } else {
            "Page map (manual mode: program-driven)"
        };
        r.styled(t, head);
        rows.push(r);
    }
    let spec = app.tlb.pending_map;
    let is_offset = matches!(spec.kind, MapKind::Offset(_));
    {
        let mut r = RowBuilder::new(x0);
        r.styled("  kind        ", label);
        let kind_label = if is_offset { "offset" } else { "identity" };
        r.hit(
            Hit::Field(VmSettingsField::Kind),
            dense_value(kind_label, hov(VmSettingsField::Kind), true, theme::TEXT),
        );
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.styled("  offset MiB  ", label);
        if is_offset {
            let v = match spec.kind {
                MapKind::Offset(v) => v,
                _ => 0,
            };
            r.hit(
                Hit::Field(VmSettingsField::Offset),
                num_val(VmSettingsField::Offset, v.to_string()),
            );
        } else {
            r.styled("—", dim);
        }
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.styled("  perms       ", label);
        let perms = [
            (VmSettingsField::PermR, spec.perms.r, "R"),
            (VmSettingsField::PermW, spec.perms.w, "W"),
            (VmSettingsField::PermX, spec.perms.x, "X"),
            (VmSettingsField::PermU, spec.perms.u, "U"),
        ];
        for (field, on, c) in perms {
            r.hit(Hit::Field(field), dense_value(c, hov(field), on, theme::RUNNING));
            r.raw(" ");
        }
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.styled("  global G    ", label);
        r.hit(
            Hit::Field(VmSettingsField::Global),
            dense_value(if spec.global { "[on]" } else { "[off]" }, hov(VmSettingsField::Global), spec.global, theme::RUNNING),
        );
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.styled("  ASID        ", label);
        r.hit(
            Hit::Field(VmSettingsField::Asid),
            num_val(VmSettingsField::Asid, spec.asid.to_string()),
        );
        rows.push(r);
    }

    rows.push(blank());

    // ── Root PT + window (read-only) ─────────────────────────────────────────
    {
        let root_pa = scheme.root_pa(app.run.mem_size as u32);
        let win_lo = app.run.base_pc.min(app.run.data_base);
        let win_hi = app.run.heap_start;
        let mut r = RowBuilder::new(x0);
        r.styled("Root PT @ ", label);
        r.styled(&format!("0x{root_pa:08x}"), Style::default().fg(theme::TEXT));
        rows.push(r);
        let mut r2 = RowBuilder::new(x0);
        r2.styled("Window    ", label);
        r2.styled(
            &format!("0x{win_lo:08x}–0x{win_hi:08x}"),
            Style::default().fg(theme::TEXT),
        );
        rows.push(r2);
    }

    rows.push(blank());

    // ── TLB geometry ─────────────────────────────────────────────────────────
    {
        let mut r = RowBuilder::new(x0);
        r.styled("TLB geometry", head);
        rows.push(r);
    }
    let p = &app.tlb.pending;
    {
        let mut r = RowBuilder::new(x0);
        r.styled("  entries     ", label);
        r.hit(
            Hit::Field(VmSettingsField::TlbEntries),
            num_val(VmSettingsField::TlbEntries, p.entry_count.to_string()),
        );
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.styled("  assoc       ", label);
        r.hit(
            Hit::Field(VmSettingsField::TlbAssoc),
            num_val(VmSettingsField::TlbAssoc, format!("{}-way", p.associativity)),
        );
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.styled("  replacement ", label);
        r.hit(
            Hit::Field(VmSettingsField::TlbReplacement),
            dense_value(
                super::replacement_label(p.replacement),
                hov(VmSettingsField::TlbReplacement),
                true,
                theme::TEXT,
            ),
        );
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.styled("  hit cyc     ", label);
        r.hit(
            Hit::Field(VmSettingsField::TlbHitLat),
            num_val(VmSettingsField::TlbHitLat, p.hit_latency.to_string()),
        );
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.styled("  miss cyc    ", label);
        r.hit(
            Hit::Field(VmSettingsField::TlbMissLat),
            num_val(VmSettingsField::TlbMissLat, p.miss_penalty.to_string()),
        );
        rows.push(r);
    }

    rows.push(blank());

    // ── Apply / flush + status ────────────────────────────────────────────────
    {
        let mut r = RowBuilder::new(x0);
        r.hit(Hit::Apply, dense_action("apply", theme::RUNNING, matches!(app.tlb.hover, Some(TlbHoverTarget::VmApply))));
        r.raw("   ");
        r.hit(Hit::Flush, dense_action("flush tlb", theme::DANGER, matches!(app.tlb.hover, Some(TlbHoverTarget::VmFlush))));
        rows.push(r);
    }
    if let Some(ref status) = app.tlb.map_status {
        let mut r = RowBuilder::new(x0);
        r.styled(status, Style::default().fg(theme::LABEL_Y));
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        let hint = if app.tlb.vm_edit_field.is_some() {
            "Enter=confirm  Esc=cancel"
        } else {
            "Click to edit/toggle · Tab cycles subtabs"
        };
        r.styled(hint, Style::default().fg(theme::IDLE));
        rows.push(r);
    }

    rows
}

fn blank() -> RowBuilder {
    RowBuilder::new(0)
}

fn human_bytes(b: u64) -> String {
    if b >= 1 << 30 {
        format!("{} GiB", b >> 30)
    } else if b >= 1 << 20 {
        format!("{} MiB", b >> 20)
    } else if b >= 1 << 10 {
        format!("{} KiB", b >> 10)
    } else {
        format!("{b} B")
    }
}
