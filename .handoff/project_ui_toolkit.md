---
name: project_ui_toolkit
description: branch refactor/ui-component-toolkit — standardized modular ratatui UI toolkit (7 phases)
metadata: 
  node_type: memory
  type: project
  originSessionId: fb504418-1323-439e-97b5-d9887467dd98
---

Branch `refactor/ui-component-toolkit` (from main, since [[project_modularize_refactor]] is a separate branch). Plan: `/home/gaok1/.claude/plans/cuddly-forging-russell.md` — 7 phases (0–6), each phase = 1+ commit, compiles in isolation.

Goal: build a modular UI toolkit under `src/ui/view/components/` + `src/ui/view/style.rs`, then migrate ALL call sites. Unify visual inconsistencies (one convention per archetype), share layout geometry `pub(crate)` between view and `input::mouse`.

Unified conventions: edit cursor `█` everywhere; bool → `true`/`false` green/red; popups all `BorderType::Rounded`; panel titles ACCENT.bold (primary) vs LABEL (secondary).

**Status:**
- Phase 0 DONE (commit 42df316): created `view/style.rs` (semantic Style/Span builders) + `components/layout.rs` (pure geometry, pub(crate)). Both carry `#![allow(dead_code)]` until consumed; removed in phase 6.
- Phase 1 DONE (commit 30f7ad0): `components/panel.rs` (PanelKind, panel/panel_frame/panel_square/handle_bar/render_panel) + `components/overlay.rs` (OverlayStyle/overlay). Migrated all ~48 panels + popups across 31 files (−177 LOC net). `components` + panel/overlay are now `pub(crate)` (so input::mouse + tutorial share them; also unblocks layout sharing in phase 5). mouse.rs hit-test now uses same panel_frame() as render (no drift). Title styles preserved exactly via panel_frame+.title for bespoke colors; only square→round popups changed visually. Left bespoke: bg-fill blocks, the square `[?]` help button (mod.rs:104).
- Phase 2 DONE (commits b00792b + 41d9eff + 35f1a5a): migrated ~358 open-coded `Style::default().fg(theme::{LABEL,TEXT,IDLE,DANGER,RUNNING,PAUSED})` → `style::{label,value,idle,danger,success,warning}()` across 26 view files (exact-equivalent prefix swap preserves chained `.bold()`). Collapsed toggle chips onto `style::toggle` (cache::{level_btn_style,subtab_style}, tlb::btn_style, pipeline::subtab_style — pipeline unifies active+hover→hovered-first; removed cache::dense_style + dead scope_btn_style). Deleted run/status.rs literal dups (push_dense_pair/value_btn/action_btn → components::controls). Metrics → `style::metric(Metric::…)`. User-approved visual changes: global footer (mod.rs) now uses `style::hint_bar` (keys accent-bold); ELF popup pills now `style::badge` (bold). Left bespoke: splash mutated styles, build.rs Red/Green pills, path_input list-selection highlight (Fase 4), docs/content didactic colors (out of scope).
- Phase 3 DONE (commit 9fa2627): extended `components/controls.rs` with the shared control vocabulary — `ControlState{Normal,Hovered,Selected,Disabled}` (+ `from`/`disabled_if`), `label_span()` (the ~14 open-coded label triangles), `bool_value()` (true/false green/red chip), `edit_value()` (single source of the `█` cursor), `field_value()`/`field_row()` (cache/TLB field_item adapters). Migrated all 5 panels: settings.rs (10 rows→builders, `_`→`█` cursor), cache/config.rs + tlb/config.rs (field_item→field_row, `preset_btn_style`/tlb-preset-closure → `dense_action(.., ACCENT, hov)`), vm_settings.rs (num_val→edit_value, render-only — hitbox/geometry stays in caller), pipeline/config_view.rs (Val{Bool,Text} split in the value loop). **User-decided visual changes:** CPI selected row now ACCENT bold (was CPI_PANEL bold); vm_settings + pipeline booleans converged to true/false green/red (vm `[on]/[off]` off-state was gray→now red; pipeline bools were neutral→now green/red, only colored values in that panel). Perms R W X U kept as-is. build ±jit clean; test 343 passed (329 lib + 11 + 3), 0 failed.
- Phase 4 DONE (commit 6f60c4d): `components/lists.rs` (`visible_window(total,view_h,scroll)->(start,end)` w/ unit tests; `ListRow`/`selectable_list`) + `components/tables.rs` (`Align`/`Col`/`DataTable` builder over real Table, `kv_table`/`kv_styled`). Centralized scattered list-selection bgs in theme (`SEL_ROW_BG`/`HOVER_ROW_BG`/`PIN_HOVER_BG`). Migrated: docs/free_page + docs/instr_ref data window → `visible_window`; tlb/entries scroll → `visible_window`; run/sidebar register rows → theme consts. Left bespoke: page_tree/vm_settings (keep their `max_scroll.set()` side-effect); didactic table colors. tables/lists carry `#![allow(dead_code)]` (toolkit offered ahead of consumers — DataTable/kv/selectable_list have no caller yet, deliberate).
- Phase 5 DONE (commit a31527e): killed the root-frame `[Length(3),Min(5),Length(1)]` duplication. `layout::app_frame_chunks(area)->[Rect;3]` (indexable form of `app_frame`). view/mod.rs main ui + help_button_area, and ALL 9 `root_chunks` + the editor `chunks` splits in input/mouse.rs → `app_frame_chunks`. Both private `centered_rect` (view + mouse) now delegate to `layout::centered_rect`. Each swap is bit-identical (same Layout/constraints/split) → geometry provably unchanged. **DEFERRED:** `best_popup_rect`/`tutorial_popup_rect` → `anchored_popup` convergence — the bespoke helpers right-align to the anchor and lack right/left fallbacks, so adopting `anchored_popup` MOVES popups (a real behavior change); left for the interactive mouse pass.
- Phase 6 DONE (commit eaed9c3): finalized the Raven-way doc in components/mod.rs (before/after for panel chrome + root frame); collapsed the tables.rs zebra `if` (only lint the new code introduced). `cargo build`, `cargo build --features jit`, `cargo test` (333 lib + 11 + 3 + 3, 0 failed) all green. clippy clean on the new toolkit modules; broad pre-existing backend clippy baseline untouched/out of scope. `#![allow(dead_code)]` kept on style/layout/tables/lists (documented toolkit surface offered ahead of consumers) — NOT removed, contrary to the original plan's aspiration, because not every helper has a caller and deleting the planned API would be wrong.
- **ALL 7 PHASES (0–6) DONE.** Outstanding before merge: the interactive TUI mouse/visual sweep (plan verification steps 3–4) — cannot be done headless; and the deferred `anchored_popup` popup-position convergence.
- Safety net: plan + this memory copied into gitignored `Raven/.handoff/` (added `/.handoff/` to .gitignore in the phase-3 commit).
- **GOTCHA:** never run repo-wide `cargo fmt` (reformats whole crate → churn in unrelated files); and bare `rustfmt <file>` reorders imports differently than `cargo fmt` (different edition/config) → spurious churn. Keep edits inline; if formatting needed, scope carefully and revert non-target files.

**Note:** pre-existing clippy error on this base (main): `never_loop` at `src/ui/app/mod.rs:2725` (correctness lint, deny-by-default). NOT from this work — `cargo clippy --all-targets` fails on it. `cargo build` + `cargo test` are clean.

---

> ⚠️ **STATUS 2026-06-04: the two follow-up sessions below were DISCARDED.**
> The user reverted the working tree (`git checkout`) to clean HEAD (`7a0c091`)
> before handing the repo to a separate large refactor (step-back + journaling).
> `git status` on `src/` is clean — **none of the responsiveness / scrollbar /
> draggable-bar code described below is on disk anymore.** The notes are kept as a
> design record. See the consolidated **PENDING CHECKLIST** at the bottom for what
> to re-do when this work resumes.

## Follow-up session — responsiveness + scrollbar (DISCARDED — was working tree)

Post-merge feature work requested by user: "UI totalmente responsiva (esconder colunas ou scrollbar quando não couber)" + bug "scrollbar do cache não respeita o arraste do mouse". Also acted on the quality review (prove/prune the speculative toolkit surface). All built + tested green; **2 PRE-EXISTING test failures** confirmed via `git stash` on clean HEAD: `run_dyn_register_view_uses_register_scroll_keys` + `run_sidebar_wheel_scrolls_registers_in_dyn_register_view` (run sidebar, NOT from this work).

- **Cache h-scrollbar drag bug — FIXED.** Root cause: on `Down` the code jumped `view_h_scroll` to the click ratio but stored the drag baseline (`hscroll_start`) as the OLD scroll → snap-back on first drag. Unified click+drag onto one absolute mapping `hscroll_pos_from_column(column, track_x, track_w, max)` in `input/mouse.rs`. Removed `hscroll_start` + renamed `hscroll_drag_start_x → hscroll_drag_track_x` (cache_state.rs + app/mod.rs init).
- **Reusable responsive primitives — ADDED + tested.** Chose NOT to bolt onto `DataTable` (poor fit for token-colored tables). `tables::fit_columns(&[ColFit], avail, gap) -> Vec<Option<u16>>` — priority-based column drop + flex distribution (4 unit tests). `lists::vertical_scrollbar(f, area, content_len, viewport, offset)` — ratatui Scrollbar overlay, no-op when it fits; caller reserves 1 right column. Re-exported from components/mod.rs (`Align, Col, ColFit, DataTable, fit_columns, vertical_scrollbar`).
- **instr_ref.rs — migrated (proving consumer).** Replaced binary `show_exp`/`col_widths` with `fit_columns` (progressive drop Expands→Operands→Type; Mnemonic+Description always kept) + added vertical scrollbar. Token coloring preserved. Removed `SHOW_EXP_MIN_W`.
- **tlb/entries.rs — migrated to `DataTable` + scrollbar.** DataTable now has a real consumer (kills its dead-code claim). NOTE: header is now bold (DataTable convention) — minor visual change. NOTE: `tlb/stats.rs` is metrics+chart, NOT a table — was a bad DataTable target, so entries proved it instead.
- **cache/stats.rs `render_history_table` — added vertical scrollbar** (reserves right col via `text_w`).
- **STILL DEAD:** `tables::kv_table`/`kv_styled` (0 consumers) → `#![allow(dead_code)]` stays on tables.rs/lists.rs/style.rs/layout.rs.
- **DEFERRED (task #5):** run sidebar register/mem scrollbars — bespoke pinned+offset scroll math + the 2 failing tests live there; needs care + interactive verification. And the kv_* prune/keep decision.
- **NEEDS INTERACTIVE VERIFICATION (headless can't):** cache scrollbar drag feel; instr_ref column-drop + scrollbar on terminal resize; tlb entries scrollbar; cache history scrollbar.
- Not committed yet — awaiting user (visual verification + commit decision).

### 2nd follow-up — horizontal scroll + DRAGGABLE scrollbars (user feedback)

User clarified intent: they want SCROLLBARS (incl. horizontal), NOT column hiding; and the bars must be mouse-draggable ("n da pra puxar o scroll com mouse. eu queria que desse"). So:
- **PRUNED `fit_columns` + `ColFit` + 4 tests** (column-hiding was a misread of the original ask). Re-export dropped.
- **Generalized drag math:** `lists::scroll_offset_from_pos(pos, track_start, track_len, max_offset)` (+2 tests) — absolute cursor→offset map. Cache's `hscroll_pos_from_column` DELETED and both its call sites now use this shared fn.
- **Added `lists::horizontal_scrollbar`** (HorizontalBottom). Re-exported `horizontal_scrollbar, scroll_offset_from_pos`.
- **instr_ref.rs reworked:** now shows ALL columns at fixed widths (Description flexes to fill spare width; `col_dims` → `(desc_w, natural_w)`); when `natural_w > content_w` it HORIZONTALLY SCROLLS (Paragraph `.scroll((0, h_off))` on header/sep/rows) with a bottom h-scrollbar — no column hiding. Vertical scrollbar kept. Both bars register `(start,len,cross,max)` track geom into new `docs.sb_v`/`docs.sb_h` Cells; `docs.h_scroll` added; `SbDrag{None,Vert,Horz}` enum + `docs.sb_drag`.
- **mouse.rs Tab::Docs:** `start_docs_scrollbar_drag` (Down: hit-test track, jump, begin drag) + `drag_docs_scrollbar` (Drag: map cursor→offset) + Up clears drag. Both docs bars are now click-to-jump AND draggable.
- Build + `cargo test --lib` green: 333 passed, same 2 pre-existing run-sidebar failures.
- **STILL render-only (task #6):** the tlb/entries + cache-history vertical scrollbars are NOT yet draggable — reuse the same mechanism next.
- **NEEDS INTERACTIVE VERIFICATION:** docs h-scroll appearing on narrow terminals; dragging both docs bars; cache matrix drag still good after the shared-fn refactor.

---

## PENDING CHECKLIST (2026-06-04) — all DISCARDED, to re-do from clean HEAD

Everything below was implemented + built + tested green this session, then thrown
away with the working-tree revert. Re-implement when UI work resumes (note: the
incoming step-back/journaling refactor may move these files, so re-confirm
locations first). Rough priority order:

1. **Cache h-scrollbar drag bug.** Matrix h-scrollbar didn't follow the mouse:
   on click it jumped to the click ratio but seeded the drag baseline with the
   OLD scroll → snap-back on first drag. Fix = one absolute mapping
   `column → scroll` shared by click + drag (drop the relative baseline).
   Files: `input/mouse.rs` (Tab::Cache drag block), `app/cache_state.rs`,
   `app/mod.rs` init.
2. **Reusable scroll primitives** in `view/components`:
   `vertical_scrollbar`, `horizontal_scrollbar`, and `scroll_offset_from_pos`
   (absolute cursor→offset map, unit-tested). Refactor the cache matrix to use
   the shared mapping (delete its local `hscroll_pos_from_column`).
3. **Docs / instr_ref responsiveness (user's explicit ask):** show ALL columns,
   never hide; Description flexes to fill; when the natural width overflows,
   HORIZONTALLY SCROLL with a bottom h-scrollbar (Paragraph `.scroll((0,h_off))`
   on header+sep+rows). Add a vertical scrollbar too. **Both bars must be
   mouse-draggable** (click-to-jump + drag) — this was the headline request.
   Needs `docs.h_scroll`, scrollbar track-geom cells, and a drag-target enum in
   `app/docs_state.rs`; Tab::Docs Down/Drag/Up handling in `input/mouse.rs`.
   ⚠️ Do NOT reintroduce column-hiding (`fit_columns`) — user wants scrollbars,
   not hidden columns.
4. **Roll out draggable vertical scrollbars** to the other overflowing lists
   using the same mechanism: `tlb/entries`, cache history table (`cache/stats`),
   and the run sidebar register/mem lists. The run sidebar has bespoke
   pinned+separator+offset scroll math AND is where the 2 pre-existing test
   failures live — do it carefully, last.
5. **Decide `tables::kv_table`/`kv_styled` fate** — 0 consumers; prune or wire up.
6. **DataTable**: `tlb/stats` is metrics+chart (not a table); `tlb/entries` is the
   genuine table to prove `DataTable` on (header goes bold — minor visual change).

**Pre-existing, NOT ours (verify before blaming any change):** 2 failing tests
`run_dyn_register_view_uses_register_scroll_keys` +
`run_sidebar_wheel_scrolls_registers_in_dyn_register_view` (fail on clean
`7a0c091` too). And clippy `never_loop` at `app/mod.rs:2725`.

**Always verify interactively (headless can't):** scrollbar drag feel, terminal
resize behavior — open the TUI in a real terminal.
