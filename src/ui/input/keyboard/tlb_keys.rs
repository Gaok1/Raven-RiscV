// Keyboard handler for the Virtual Memory tab, and the one place every VM
// action lives — the mouse handler calls `activate` with whatever was clicked,
// so a key and a click on the same control can never do different things.
//
// Keys shared by every subtab: Tab/Shift-Tab change subtab, g goes back to the
// current access, s steps to the next translation and b back one step (s is the
// snapshot key on stats, as on the Cache tab), r/p/f reset, run/pause and
// change speed. The rest:
//
//   walk    ←/→ older/newer translation   ↑/↓ the same, through the history
//           v which kind to follow        1/2/3 show all/misses/faults
//   tlb     arrows move between sets and ways
//   map     ↑/↓ entry   →/enter open   ← back   / go to an address
//   stats   s snapshot   ↑/↓ select   enter open   d delete
//   config  m mode   e edit a number (tab next, enter keeps, esc drops)

use crate::falcon::mmu::VmMode;
use crate::ui::app::{App, HistoryFilter, MAP_COLUMNS, MapRow, VmField, VmHit, VmSubtab, WalkFollow};
use crossterm::event::{KeyCode, KeyEvent};
use raven_engine::capability::{AddressTranslation, TableEntryKind};

use super::serialization::{
    capture_session_snapshot, do_export_config, do_export_results, do_import_config,
};

pub(super) fn handle(app: &mut App, key: KeyEvent) -> bool {
    if app.tlb.edit.is_some() {
        return handle_field_edit(app, key.code);
    }
    if app.tlb.map_query.is_some() {
        return handle_map_query(app, key.code);
    }

    let sub = app.tlb.vm_subtab;
    match key.code {
        KeyCode::Tab => {
            select(app, next_subtab(sub, true));
            return true;
        }
        KeyCode::BackTab => {
            select(app, next_subtab(sub, false));
            return true;
        }
        KeyCode::Char('g') => {
            app.tlb.follow_current();
            return true;
        }
        KeyCode::Char('s') if sub != VmSubtab::Stats => {
            step(app, true);
            return true;
        }
        KeyCode::Char('b') => {
            step(app, false);
            return true;
        }
        KeyCode::Char('r') => {
            app.restart_simulation();
            return true;
        }
        KeyCode::Char('p') | KeyCode::Char(' ') => {
            toggle_run(app);
            return true;
        }
        KeyCode::Char('f') => {
            app.session.speed = app.session.speed.cycle();
            return true;
        }
        _ => {}
    }

    match sub {
        VmSubtab::Walk => handle_walk(app, key.code),
        VmSubtab::Tlb => handle_tlb(app, key.code),
        VmSubtab::Map => handle_map(app, key.code),
        VmSubtab::Stats => handle_stats(app, key.code),
        VmSubtab::Config => handle_config(app, key.code),
    }
}

fn toggle_run(app: &mut App) {
    if app.session.is_running {
        app.session.is_running = false;
    } else if app.core_status(app.selected_core) == crate::ui::app::HartLifecycle::Paused
        || !app.session.faulted
    {
        app.resume_selected_hart();
        if app.can_start_run() {
            app.session.is_running = true;
        }
    }
}

/// Steps a forward step may take looking for the next translation.
const MAX_STEPS: usize = 2048;

/// Forward to the next translation, or back one step, then follow the newest
/// access: whoever steps here wants to see the translation that step caused.
///
/// A step retires one instruction, and with the pipeline on that is not always
/// a new translation: instructions fetched before a TLB miss keep retiring
/// while fetch waits on the walk, and pressing `s` would show nothing new. So
/// while addresses are being translated, a forward step runs until one more
/// translation is recorded (or the program stops); otherwise it is one step.
pub(crate) fn step(app: &mut App, forward: bool) {
    if app.session.is_running {
        return;
    }
    if !forward {
        app.stepback_one();
        app.tlb.follow_current();
        return;
    }
    let translating = app.translation().is_some_and(|t| t.enabled());
    let newest = |app: &App| app.vm_records().last().map(|rec| rec.seq);
    let before = newest(app);
    for _ in 0..if translating { MAX_STEPS } else { 1 } {
        if app.session.faulted {
            break;
        }
        let pc = app.program_counter();
        let cycles = app.execution_totals().cycles;
        app.single_step();
        let stuck = app.program_counter() == pc && app.execution_totals().cycles == cycles;
        if newest(app) != before || stuck {
            break;
        }
    }
    app.tlb.follow_current();
}

fn next_subtab(cur: VmSubtab, forward: bool) -> VmSubtab {
    let seq = VmSubtab::ALL;
    let i = seq.iter().position(|&s| s == cur).unwrap_or(0);
    let n = seq.len();
    seq[if forward { (i + 1) % n } else { (i + n - 1) % n }]
}

/// Open a subtab. Config starts from the live TLB shape.
pub(crate) fn select(app: &mut App, vm: VmSubtab) {
    app.commit_vm_edit();
    app.tlb.map_query = None;
    app.tlb.vm_subtab = vm;
    if matches!(vm, VmSubtab::Config)
        && let Some(rv32) = app.rv32()
    {
        app.tlb.pending = rv32.mem().mmu().tlb.config.clone();
    }
}

// ── walk ─────────────────────────────────────────────────────────────────────

fn handle_walk(app: &mut App, code: KeyCode) -> bool {
    match code {
        KeyCode::Left | KeyCode::Down => step_pin(app, true),
        KeyCode::Right | KeyCode::Up => step_pin(app, false),
        KeyCode::Char('v') => {
            let i = WalkFollow::ALL
                .iter()
                .position(|&k| k == app.tlb.follow)
                .unwrap_or(0);
            app.tlb.follow = WalkFollow::ALL[(i + 1) % WalkFollow::ALL.len()];
            app.tlb.pinned = None;
        }
        KeyCode::Char('1') => set_filter(app, HistoryFilter::All),
        KeyCode::Char('2') => set_filter(app, HistoryFilter::Misses),
        KeyCode::Char('3') => set_filter(app, HistoryFilter::Faults),
        _ => return false,
    }
    true
}

fn set_filter(app: &mut App, filter: HistoryFilter) {
    app.tlb.filter = filter;
    app.tlb.history_scroll = 0;
}

/// Move the pin one translation older or newer through the history list.
/// Stepping newer than the newest goes back to following.
fn step_pin(app: &mut App, older: bool) {
    let records = app.vm_records();
    let list: Vec<_> = records
        .iter()
        .rev()
        .filter(|r| app.tlb.filter.admits(r))
        .collect();
    if list.is_empty() {
        return;
    }
    let current = app.vm_current().map(|r| r.seq);
    let at = current.and_then(|seq| list.iter().position(|r| r.seq == seq));
    let next = match (at, older) {
        (None, _) => Some(0),
        (Some(i), true) => Some((i + 1).min(list.len() - 1)),
        (Some(0), false) => None,
        (Some(i), false) => Some(i - 1),
    };
    match next {
        Some(i) => {
            app.tlb.pinned = Some(list[i].clone());
            // Keep the pinned row on screen.
            app.tlb.history_scroll = app.tlb.history_scroll.min(i);
            if i >= app.tlb.history_scroll + 8 {
                app.tlb.history_scroll = i.saturating_sub(7);
            }
        }
        None => app.tlb.follow_current(),
    }
    app.tlb.tlb_cursor = None;
    app.tlb.map_cursor = [None; MAP_COLUMNS];
}

fn pin_record(app: &mut App, seq: u64) {
    if let Some(rec) = app.vm_records().into_iter().find(|r| r.seq == seq) {
        app.tlb.pinned = Some(rec);
        app.tlb.tlb_cursor = None;
        app.tlb.map_cursor = [None; MAP_COLUMNS];
    }
}

// ── tlb ──────────────────────────────────────────────────────────────────────

fn handle_tlb(app: &mut App, code: KeyCode) -> bool {
    match code {
        KeyCode::Up => move_tlb_cursor(app, -1, 0),
        KeyCode::Down => move_tlb_cursor(app, 1, 0),
        KeyCode::Left => move_tlb_cursor(app, 0, -1),
        KeyCode::Right => move_tlb_cursor(app, 0, 1),
        _ => return false,
    }
    true
}

fn tlb_shape(app: &App) -> Option<(usize, usize)> {
    let translation = app.translation()?;
    let sets = translation.tlb_sets().max(1);
    Some((sets, (translation.tlb_len() / sets).max(1)))
}

fn move_tlb_cursor(app: &mut App, d_set: isize, d_way: isize) {
    let Some((sets, ways)) = tlb_shape(app) else {
        return;
    };
    let slot = app.vm_tlb_cursor().unwrap_or(0);
    let set = (slot / ways) as isize + d_set;
    let way = (slot % ways) as isize + d_way;
    let set = set.clamp(0, sets as isize - 1) as usize;
    let way = way.clamp(0, ways as isize - 1) as usize;
    app.tlb.tlb_cursor = Some(set * ways + way);
}

// ── map ──────────────────────────────────────────────────────────────────────

fn handle_map(app: &mut App, code: KeyCode) -> bool {
    match code {
        KeyCode::Up => map_move(app, -1),
        KeyCode::Down => map_move(app, 1),
        KeyCode::PageUp => map_move(app, -10),
        KeyCode::PageDown => map_move(app, 10),
        KeyCode::Right | KeyCode::Enter => map_enter(app),
        KeyCode::Left | KeyCode::Backspace => {
            app.tlb.map_column = app.tlb.map_column.saturating_sub(1);
        }
        KeyCode::Char('/') => app.tlb.map_query = Some(String::new()),
        _ => return false,
    }
    true
}

/// Select the row `delta` rows away in the focused column.
fn map_move(app: &mut App, delta: isize) {
    let columns = app.vm_map_columns();
    let Some(column) = columns.get(app.tlb.map_column.min(columns.len().saturating_sub(1)))
    else {
        return;
    };
    let row = (column.selected as isize + delta).clamp(0, column.rows.len() as isize - 1);
    let index = column.rows[row as usize].first().index;
    select_map_entry(app, column.level, index);
}

fn select_map_entry(app: &mut App, level: usize, index: u64) {
    app.tlb.map_column = level;
    app.tlb.map_cursor[level] = Some(index);
    // A different entry above means a different table below.
    for deeper in app.tlb.map_cursor.iter_mut().skip(level + 1) {
        *deeper = None;
    }
}

/// Open the selected run, or follow the selected pointer into its table.
fn map_enter(app: &mut App) {
    let columns = app.vm_map_columns();
    let Some(column) = columns.get(app.tlb.map_column) else {
        return;
    };
    match column.rows.get(column.selected) {
        Some(MapRow::Run { first, .. }) => {
            app.tlb.map_open.push((column.table, first.index));
        }
        Some(MapRow::One(entry))
            if matches!(entry.kind, TableEntryKind::Pointer { .. })
                && column.level + 1 < columns.len() =>
        {
            app.tlb.map_column = column.level + 1;
        }
        Some(MapRow::One(_)) | None => {}
    }
}

fn handle_map_query(app: &mut App, code: KeyCode) -> bool {
    let Some(text) = app.tlb.map_query.as_mut() else {
        return false;
    };
    match code {
        KeyCode::Esc => app.tlb.map_query = None,
        KeyCode::Backspace => {
            text.pop();
        }
        KeyCode::Char(c) if c.is_ascii_hexdigit() && text.len() < 8 => {
            text.push(c.to_ascii_lowercase());
        }
        KeyCode::Enter => {
            let parsed = u64::from_str_radix(text, 16).ok();
            app.tlb.map_query = None;
            if let Some(address) = parsed {
                app.tlb.map_target = Some(address);
                app.tlb.map_cursor = [None; MAP_COLUMNS];
                app.tlb.map_column = 0;
            }
        }
        _ => {}
    }
    true
}

// ── stats ────────────────────────────────────────────────────────────────────

fn handle_stats(app: &mut App, code: KeyCode) -> bool {
    let len = app.cache.session_history.len();
    match code {
        KeyCode::Char('s') => capture_session_snapshot(app),
        KeyCode::Char('d') | KeyCode::Char('D') if len > 0 => app.delete_selected_snapshot(),
        KeyCode::Enter if len > 0 && !app.session.is_running => {
            app.cache.viewing_snapshot = Some(app.cache.history_scroll.min(len - 1));
        }
        KeyCode::Up => app.cache.history_scroll = app.cache.history_scroll.saturating_sub(1),
        KeyCode::Down if len > 0 => {
            app.cache.history_scroll = (app.cache.history_scroll + 1).min(len - 1);
        }
        _ => return false,
    }
    true
}

// ── config ───────────────────────────────────────────────────────────────────

fn handle_config(app: &mut App, code: KeyCode) -> bool {
    match code {
        KeyCode::Char('m') => app.set_vm_mode(app.vm_mode().cycle()),
        KeyCode::Char('e') => app.start_vm_edit(first_editable(app)),
        _ => return false,
    }
    true
}

/// The number fields the current mode lets the reader change, in screen order.
fn editable(app: &App) -> Vec<VmField> {
    VmField::ALL
        .into_iter()
        .filter(|&f| f != VmField::Asid || app.vm_mode() == VmMode::Auto)
        .collect()
}

fn first_editable(app: &App) -> VmField {
    editable(app)[0]
}

fn handle_field_edit(app: &mut App, code: KeyCode) -> bool {
    let Some(field) = app.tlb.edit.as_ref().map(|(field, _)| *field) else {
        return false;
    };
    match code {
        KeyCode::Esc => app.tlb.edit = None,
        KeyCode::Enter => app.commit_vm_edit(),
        KeyCode::Tab | KeyCode::BackTab => {
            let order = editable(app);
            let i = order.iter().position(|&f| f == field).unwrap_or(0);
            let n = order.len();
            let next = if code == KeyCode::Tab {
                order[(i + 1) % n]
            } else {
                order[(i + n - 1) % n]
            };
            app.start_vm_edit(next);
        }
        KeyCode::Backspace => {
            if let Some((_, text)) = app.tlb.edit.as_mut() {
                text.pop();
            }
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            if let Some((_, text)) = app.tlb.edit.as_mut().filter(|(_, t)| t.len() < 5) {
                text.push(c);
            }
        }
        _ => {}
    }
    true
}

// ── Clicks ───────────────────────────────────────────────────────────────────

/// Do what `hit` means. Execution buttons are the mouse handler's own, as in
/// every other tab.
pub(crate) fn activate(app: &mut App, hit: VmHit) {
    match hit {
        VmHit::Subtab(sub) => select(app, sub),
        VmHit::Run(_) | VmHit::History | VmHit::TlbArea => {}
        VmHit::Results => mirror_status(app, do_export_results),
        VmHit::ImportCfg => mirror_status(app, do_import_config),
        VmHit::ExportCfg => mirror_status(app, do_export_config),
        VmHit::FlushTlb => app.flush_tlb(),
        VmHit::InstallMap => {
            app.install_auto_map();
            if app.tlb.config_error.is_none() {
                app.tlb.config_status = Some("page map installed".into());
            }
        }
        VmHit::TurnOn => app.set_vm_mode(VmMode::Auto),

        VmHit::Older => step_pin(app, true),
        VmHit::Newer => step_pin(app, false),
        VmHit::Follow(kind) => {
            app.tlb.follow = kind;
            app.tlb.follow_current();
        }
        VmHit::Filter(filter) => set_filter(app, filter),
        VmHit::Record(seq) => pin_record(app, seq),

        VmHit::Slot(slot) => app.tlb.tlb_cursor = Some(slot),
        VmHit::SetTab(set) => {
            if let Some((_, ways)) = tlb_shape(app) {
                app.tlb.tlb_cursor = Some(set * ways);
            }
        }

        VmHit::MapRow { column, row } => {
            let columns = app.vm_map_columns();
            let Some(col) = columns.get(column) else {
                return;
            };
            let already = app.tlb.map_column == column && col.selected == row;
            if let Some(r) = col.rows.get(row) {
                select_map_entry(app, column, r.first().index);
            }
            if already {
                map_enter(app);
            }
        }
        VmHit::MapColumn(column) => app.tlb.map_column = column,
        VmHit::MapFollow => app.tlb.follow_current(),
        VmHit::MapQuery => app.tlb.map_query = Some(String::new()),

        VmHit::Snapshot(i) => {
            if app.cache.history_scroll == i && !app.session.is_running {
                app.cache.viewing_snapshot = Some(i);
            }
            app.cache.history_scroll = i;
        }

        VmHit::Mode(mode) => app.set_vm_mode(mode),
        VmHit::Perm(i) => {
            if app.vm_mode() == VmMode::Auto {
                let mut spec = app.tlb.page_map;
                match i {
                    0 => spec.perms.r ^= true,
                    1 => spec.perms.w ^= true,
                    2 => spec.perms.x ^= true,
                    _ => spec.perms.u ^= true,
                }
                app.set_page_map(spec);
            }
        }
        VmHit::Global => {
            if app.vm_mode() == VmMode::Auto {
                let mut spec = app.tlb.page_map;
                spec.global ^= true;
                app.set_page_map(spec);
            }
        }
        VmHit::Edit(field) => app.start_vm_edit(field),
        VmHit::TlbOn(on) => app.set_tlb_enabled(on),
        VmHit::Entries(n) => {
            let mut cfg = app.tlb.pending.clone();
            // "all ways" stays fully associative at the new size.
            let full = u8::try_from(cfg.entry_count).unwrap_or(u8::MAX);
            if cfg.associativity >= full {
                cfg.associativity = u8::try_from(n).unwrap_or(u8::MAX);
            }
            cfg.entry_count = n;
            app.set_tlb_geometry(cfg);
        }
        VmHit::Ways(n) => {
            let mut cfg = app.tlb.pending.clone();
            cfg.associativity = n;
            app.set_tlb_geometry(cfg);
        }
        VmHit::Policy(policy) => {
            let mut cfg = app.tlb.pending.clone();
            cfg.replacement = policy;
            app.set_tlb_geometry(cfg);
        }
    }
}

/// Export and import report through the Cache tab's status line; show it on
/// this tab's footer too.
fn mirror_status(app: &mut App, action: fn(&mut App)) {
    action(app);
    app.tlb.config_status = app.cache.config_status.clone();
    app.tlb.config_error = app.cache.config_error.clone();
}

/// The wheel over `hit`: one step through whatever that region lists.
pub(crate) fn scroll(app: &mut App, hit: Option<VmHit>, down: bool) {
    let delta = if down { 1 } else { -1 };
    match (app.tlb.vm_subtab, hit) {
        (VmSubtab::Walk, _) => {
            app.tlb.history_scroll = if down {
                app.tlb.history_scroll + 1
            } else {
                app.tlb.history_scroll.saturating_sub(1)
            };
            // Clamp to the list so scrolling back up responds at once.
            let max = app
                .vm_records()
                .iter()
                .filter(|r| app.tlb.filter.admits(r))
                .count()
                .saturating_sub(1);
            app.tlb.history_scroll = app.tlb.history_scroll.min(max);
        }
        (VmSubtab::Tlb, _) => move_tlb_cursor(app, delta, 0),
        (VmSubtab::Map, Some(VmHit::MapColumn(column)))
        | (VmSubtab::Map, Some(VmHit::MapRow { column, .. })) => {
            app.tlb.map_column = column;
            map_move(app, delta);
        }
        (VmSubtab::Map, _) => map_move(app, delta),
        (VmSubtab::Stats, _) => {
            let _ = handle_stats(app, if down { KeyCode::Down } else { KeyCode::Up });
        }
        (VmSubtab::Config, _) => {}
    }
}
