// ui/view/tlb/vm_settings.rs — the comprehensive Virtual Memory control panel.
//
// One centered column, mirroring the Cache tab's settings: VM mode, TLB on/off,
// the parametric paging scheme (Custom mode), the auto-installed page map
// (kind / offset / perms / G / ASID), a read-only root-PT + window readout and
// the TLB geometry. An `apply` button installs the map + scheme and re-points
// satp; `flush tlb` drops cached translations.

use ratatui::{Frame, prelude::*, widgets::Paragraph};

use crate::falcon::mmu::{MapKind, VmMode};
use crate::ui::app::{App, TlbHoverTarget, VmSettingsField};
use crate::ui::theme;
use crate::ui::view::components::panel::{self, PanelKind};
use crate::ui::view::components::{bool_value, dense_action, dense_value, edit_value};
use crate::ui::view::style;

/// What a hitbox in the panel points at.
#[derive(Clone)]
enum Hit {
    Field(VmSettingsField),
    /// TLB geometry preset (0=small, 1=med, 2=large).
    Preset(usize),
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
        Self {
            x,
            spans: Vec::new(),
            hits: Vec::new(),
        }
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

    /// A top-level field label, padded so every value in the panel starts in
    /// the same column.
    fn label(&mut self, name: &str, style: Style) {
        self.styled(&format!("{name:<LABEL_W$}"), style);
    }

    /// An indented sub-field label, landing on that same value column.
    fn sub(&mut self, name: &str, style: Style) {
        self.styled(&format!("  {:<w$}", name, w = LABEL_W - 2), style);
    }

    /// A value you can only read, aligned with the ones you can click.
    ///
    /// A clickable value is a pill and carries a padding column; a plain one
    /// does not, so writing it bare left every read-only row sitting one column
    /// to the left of the editable rows above and below it.
    fn readout(&mut self, s: &str, style: Style) {
        self.styled(&format!(" {s}"), style);
    }

    /// A section heading, drawn as the labelled hairline the rest of the app
    /// uses to divide a panel.
    fn section(name: &str, width: u16) -> Self {
        let mut r = Self::new(0);
        r.spans = panel::section_rule(name, width.saturating_sub(1)).spans;
        r
    }
}

/// Columns reserved for a field label. Every label was padded by hand to its
/// own guessed width before, so `offset bits` and `L0 index` put their values
/// one column apart and the panel read as two ragged lists.
const LABEL_W: usize = 15;

/// Minimum width a settings column needs before the two-column split is worth
/// taking; below it the panel keeps one centred column.
const COL_W: u16 = 52;

pub(super) fn render_vm_settings(f: &mut Frame, area: Rect, app: &App) {
    let inner = panel::render_panel(
        f,
        area,
        panel::panel("Virtual Memory Settings", PanelKind::Accent),
    );
    if inner.height == 0 || inner.width < 4 {
        return;
    }

    // Two columns when there is room. A single 66-column strip centred in a
    // 158-column panel left 46 blank columns on each side *and* had to scroll
    // its own content, which is the shape a settings panel should never be.
    let split_cols = inner.width >= COL_W * 2;
    let (left, right) = if split_cols {
        // Two columns of the width the content needs, centred as a pair — not
        // stretched to half the panel each, which pushed the right column's
        // labels away from the values in the left.
        let pair = (COL_W * 2).min(inner.width);
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(pair / 2),
                Constraint::Length(pair - pair / 2),
                Constraint::Min(0),
            ])
            .split(inner);
        (cols[1], Some(cols[2]))
    } else {
        let w = inner.width.min(66);
        (
            Rect::new(inner.x + (inner.width - w) / 2, inner.y, w, inner.height),
            None,
        )
    };

    // Rows are measured from one column in, and then *drawn* from one column in
    // too. They used to be measured at `x + 1` but rendered at `x`, so every
    // hitbox in the panel sat one column right of the control it covered.
    let indent = |r: Rect| Rect::new(r.x + 1, r.y, r.width.saturating_sub(1), r.height);
    let (left, right) = (indent(left), right.map(indent));

    // Built once per column origin, because a `RowBuilder` bakes its hitbox
    // columns in as it measures the spans — asking it for the same rows at a
    // second origin is how the right column gets clickable controls without a
    // parallel copy of the layout.
    let (rows_l, split_at) = build_rows(app, left.x, left.width);
    let rows_r = right.map(|r| build_rows(app, r.x, r.width).0);

    let mut field_hits: Vec<(VmSettingsField, u16, u16, u16)> = Vec::new();
    let mut preset_hits = [(0u16, 0u16, 0u16); 3];
    app.tlb.vm_apply_btn.set((0, 0, 0));
    app.tlb.vm_flush_btn.set((0, 0, 0));

    let register = |row: &RowBuilder,
                        y: u16,
                        field_hits: &mut Vec<(VmSettingsField, u16, u16, u16)>,
                        preset_hits: &mut [(u16, u16, u16); 3]| {
        for (hit, x0, x1) in &row.hits {
            match hit {
                Hit::Field(field) => field_hits.push((*field, y, *x0, *x1)),
                Hit::Preset(p) => preset_hits[*p] = (y, *x0, *x1),
                Hit::Apply => app.tlb.vm_apply_btn.set((y, *x0, *x1)),
                Hit::Flush => app.tlb.vm_flush_btn.set((y, *x0, *x1)),
            }
        }
    };

    // With two columns nothing overflows, so the scroll offset is only used by
    // the single-column fallback.
    let (head, tail): (&[RowBuilder], &[RowBuilder]) = match (&rows_r, split_cols) {
        (Some(rows_r), true) => (&rows_l[..split_at], &rows_r[split_at..]),
        _ => (&rows_l[..], &[]),
    };

    let visible = left.height as usize;
    let max_scroll = head.len().saturating_sub(visible);
    app.tlb.vm_settings_max_scroll.set(max_scroll);
    let scroll = app.tlb.vm_settings_scroll.min(max_scroll);

    let mut lines: Vec<Line> = Vec::with_capacity(visible);
    for (i, row) in head.iter().enumerate().skip(scroll).take(visible) {
        let y = left.y + (i - scroll) as u16;
        register(row, y, &mut field_hits, &mut preset_hits);
        lines.push(Line::from(row.spans.clone()));
    }
    f.render_widget(Paragraph::new(lines), left);

    if let Some(right) = right {
        let mut lines: Vec<Line> = Vec::with_capacity(right.height as usize);
        for (i, row) in tail.iter().take(right.height as usize).enumerate() {
            register(row, right.y + i as u16, &mut field_hits, &mut preset_hits);
            lines.push(Line::from(row.spans.clone()));
        }
        f.render_widget(Paragraph::new(lines), right);
    }

    *app.tlb.vm_field_hitboxes.borrow_mut() = field_hits;
    app.tlb.preset_btns.set(preset_hits);
}

/// Builds every settings row at origin `x0`, and returns the index the second
/// column starts at — the read-only Root PT / window readout, which is where
/// the editable form ends and the reference numbers begin.
fn build_rows(app: &App, x0: u16, col_w: u16) -> (Vec<RowBuilder>, usize) {
    let mode = app.vm_mode();
    let custom = matches!(mode, VmMode::Custom);
    let auto = mode.is_auto();
    let editing = |field: VmSettingsField| app.tlb.vm_edit_field == Some(field);
    let hov = |field: VmSettingsField| matches!(&app.tlb.hover, Some(TlbHoverTarget::VmField(f)) if *f == field);
    let edit_buf = app.tlb.vm_edit_buf.as_str();

    let label = style::label();
    let dim = Style::default().fg(theme::BORDER);

    // Render a numeric field's value (unified `█` edit cursor when focused).
    let num_val = |field: VmSettingsField, value: String| -> Span<'static> {
        edit_value(
            &value,
            editing(field).then_some(edit_buf),
            hov(field),
            theme::LABEL_Y,
        )
    };

    let mut rows: Vec<RowBuilder> = Vec::new();

    // ── Mode + TLB ───────────────────────────────────────────────────────────
    {
        let mut r = RowBuilder::new(x0);
        r.label("mode", label);
        r.hit(
            Hit::Field(VmSettingsField::Mode),
            dense_value(
                &format!("< {} >", mode.as_str()),
                hov(VmSettingsField::Mode),
                true,
                theme::TEXT,
            ),
        );
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.label("tlb cache", label);
        r.hit(
            Hit::Field(VmSettingsField::TlbEnabled),
            bool_value(app.session.tlb_enabled, hov(VmSettingsField::TlbEnabled)),
        );
        rows.push(r);
    }

    rows.push(blank());

    // ── Paging scheme ──────────────────────────────────────────────────────
    rows.push(RowBuilder::section(
        if custom {
            "PAGING SCHEME · editable"
        } else {
            "PAGING SCHEME · preset"
        },
        col_w,
    ));
    let scheme = if custom {
        app.tlb.pending_scheme.clone()
    } else {
        crate::falcon::mmu::PagingScheme::sv32()
    };
    {
        let mut r = RowBuilder::new(x0);
        r.sub("offset bits", label);
        if custom {
            r.hit(
                Hit::Field(VmSettingsField::OffsetBits),
                num_val(VmSettingsField::OffsetBits, scheme.offset_bits.to_string()),
            );
        } else {
            r.readout(&scheme.offset_bits.to_string(), dim);
        }
        rows.push(r);
    }
    for (i, &bits) in scheme.level_bits.iter().enumerate() {
        let mut r = RowBuilder::new(x0);
        r.sub(&format!("L{i} index"), label);
        let field = VmSettingsField::LevelBits(i);
        if custom {
            r.hit(Hit::Field(field), num_val(field, bits.to_string()));
        } else {
            r.readout(&bits.to_string(), dim);
        }
        rows.push(r);
    }
    if custom {
        let mut r = RowBuilder::new(x0);
        r.sub("levels", label);
        // Words, not `[+]`/`[-]`: a bracket is how this UI writes a keyboard
        // key, and a button says what it does.
        r.hit(
            Hit::Field(VmSettingsField::AddLevel),
            dense_action("add", theme::RUNNING, hov(VmSettingsField::AddLevel)),
        );
        r.raw(" ");
        r.hit(
            Hit::Field(VmSettingsField::RemoveLevel),
            dense_action("remove", theme::DANGER, hov(VmSettingsField::RemoveLevel)),
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
            style::success()
        } else {
            style::danger()
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
    rows.push(RowBuilder::section(
        if auto {
            "PAGE MAP · auto-installed"
        } else {
            "PAGE MAP · program-driven"
        },
        col_w,
    ));
    let spec = app.tlb.pending_map;
    let is_offset = matches!(spec.kind, MapKind::Offset(_));
    {
        let mut r = RowBuilder::new(x0);
        r.sub("kind", label);
        let kind_label = if is_offset { "offset" } else { "identity" };
        r.hit(
            Hit::Field(VmSettingsField::Kind),
            dense_value(kind_label, hov(VmSettingsField::Kind), true, theme::TEXT),
        );
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.sub("offset MiB", label);
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
            r.readout("—", dim);
        }
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.sub("perms", label);
        let perms = [
            (VmSettingsField::PermR, spec.perms.r, "R"),
            (VmSettingsField::PermW, spec.perms.w, "W"),
            (VmSettingsField::PermX, spec.perms.x, "X"),
            (VmSettingsField::PermU, spec.perms.u, "U"),
        ];
        for (field, on, c) in perms {
            r.hit(
                Hit::Field(field),
                dense_value(c, hov(field), on, theme::RUNNING),
            );
            r.raw(" ");
        }
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.sub("global G", label);
        r.hit(
            Hit::Field(VmSettingsField::Global),
            bool_value(spec.global, hov(VmSettingsField::Global)),
        );
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.sub("ASID", label);
        r.hit(
            Hit::Field(VmSettingsField::Asid),
            num_val(VmSettingsField::Asid, spec.asid.to_string()),
        );
        rows.push(r);
    }

    let split_at = rows.len();

    // ── Root PT + window (read-only) ─────────────────────────────────────────
    {
        let root_pa = scheme.root_pa(app.session.mem_size as u32);
        let win_lo = app.session.base_pc.min(app.session.data_base);
        let win_hi = app.session.heap_start;
        let mut r = RowBuilder::new(x0);
        r.label("root table", label);
        r.readout(&format!("0x{root_pa:08x}"), style::value());
        rows.push(r);
        let mut r2 = RowBuilder::new(x0);
        r2.label("window", label);
        r2.readout(&format!("0x{win_lo:08x}–0x{win_hi:08x}"), style::value());
        rows.push(r2);
    }

    rows.push(blank());

    // ── TLB geometry ─────────────────────────────────────────────────────────
    rows.push(RowBuilder::section("TLB GEOMETRY", col_w));
    let p = &app.tlb.pending;
    {
        let mut r = RowBuilder::new(x0);
        r.sub("entries", label);
        r.hit(
            Hit::Field(VmSettingsField::TlbEntries),
            num_val(VmSettingsField::TlbEntries, p.entry_count.to_string()),
        );
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.sub("assoc", label);
        r.hit(
            Hit::Field(VmSettingsField::TlbAssoc),
            num_val(
                VmSettingsField::TlbAssoc,
                format!("{}-way", p.associativity),
            ),
        );
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.sub("replacement", label);
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
        r.sub("hit cycles", label);
        r.hit(
            Hit::Field(VmSettingsField::TlbHitLat),
            num_val(VmSettingsField::TlbHitLat, p.hit_latency.to_string()),
        );
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.sub("miss cycles", label);
        r.hit(
            Hit::Field(VmSettingsField::TlbMissLat),
            num_val(VmSettingsField::TlbMissLat, p.miss_penalty.to_string()),
        );
        rows.push(r);
    }
    {
        let mut r = RowBuilder::new(x0);
        r.sub("presets", label);
        let hov_preset =
            |i: usize| matches!(&app.tlb.hover, Some(TlbHoverTarget::Preset(p)) if *p == i);
        for (i, label) in ["small 16", "med 32", "large 64"].iter().enumerate() {
            if i > 0 {
                r.raw("  ");
            }
            r.hit(
                Hit::Preset(i),
                dense_action(label, theme::ACCENT, hov_preset(i)),
            );
        }
        rows.push(r);
    }

    rows.push(blank());

    // ── Apply / flush + status ────────────────────────────────────────────────
    {
        let mut r = RowBuilder::new(x0);
        r.hit(
            Hit::Apply,
            dense_action(
                "apply",
                theme::RUNNING,
                matches!(app.tlb.hover, Some(TlbHoverTarget::VmApply)),
            ),
        );
        r.raw("   ");
        r.hit(
            Hit::Flush,
            dense_action(
                "flush tlb",
                theme::DANGER,
                matches!(app.tlb.hover, Some(TlbHoverTarget::VmFlush)),
            ),
        );
        rows.push(r);
    }
    if let Some(ref status) = app.tlb.map_status {
        let mut r = RowBuilder::new(x0);
        r.styled(status, Style::default().fg(theme::LABEL_Y));
        rows.push(r);
    }
    // Only while a field is open. The idle copy explained how to click, which is
    // what the button surfaces are for; the keys are in the footer.
    if app.tlb.vm_edit_field.is_some() {
        let mut r = RowBuilder::new(x0);
        r.styled("[enter] confirm  [esc] cancel  [tab] next field", style::idle());
        rows.push(r);
    }

    (rows, split_at)
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
