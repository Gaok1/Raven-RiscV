// Keyboard handler for the top-level Virtual Memory tab.

use crate::ui::app::{App, TlbSubtab, VmSettingsField, VmSubtab};
use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle(app: &mut App, key: KeyEvent) -> bool {
    // Field-edit mode in the TLB Settings subtab consumes most keys.
    if in_settings(app) && app.tlb.edit_field.is_some() {
        return handle_field_edit(app, key.code);
    }
    // Numeric edit of a VM Settings field consumes most keys.
    if in_vm_settings(app) && app.tlb.vm_edit_field.is_some() {
        return handle_vm_field_edit(app, key.code);
    }

    match key.code {
        KeyCode::Up if in_vm_settings(app) => {
            app.tlb.vm_settings_scroll = app.tlb.vm_settings_scroll.saturating_sub(1);
            true
        }
        KeyCode::Down if in_vm_settings(app) => {
            let max = app.tlb.vm_settings_max_scroll.get();
            app.tlb.vm_settings_scroll = app.tlb.vm_settings_scroll.saturating_add(1).min(max);
            true
        }
        KeyCode::Tab => {
            let (vm, sub) = cycle_subtab(app, true);
            select(app, vm, sub);
            true
        }
        KeyCode::BackTab => {
            let (vm, sub) = cycle_subtab(app, false);
            select(app, vm, sub);
            true
        }
        KeyCode::Char('f') if in_settings(app) => {
            app.flush_tlb();
            true
        }
        KeyCode::Up if in_entries(app) => {
            app.tlb.entries_scroll = app.tlb.entries_scroll.saturating_sub(1);
            true
        }
        KeyCode::Down if in_entries(app) => {
            let total = app.run.mem().mmu().tlb.entries.len();
            let next = app.tlb.entries_scroll.saturating_add(1);
            app.tlb.entries_scroll = next.min(total.saturating_sub(1));
            true
        }
        KeyCode::Up if in_tree(app) => {
            app.tlb.page_tree_scroll = app.tlb.page_tree_scroll.saturating_sub(1);
            true
        }
        KeyCode::Down if in_tree(app) => {
            let max = app.tlb.page_tree_max_scroll.get();
            app.tlb.page_tree_scroll = app.tlb.page_tree_scroll.saturating_add(1).min(max);
            true
        }
        KeyCode::PageUp if in_tree(app) => {
            app.tlb.page_tree_scroll = app.tlb.page_tree_scroll.saturating_sub(10);
            true
        }
        KeyCode::PageDown if in_tree(app) => {
            let max = app.tlb.page_tree_max_scroll.get();
            app.tlb.page_tree_scroll = app.tlb.page_tree_scroll.saturating_add(10).min(max);
            true
        }
        _ => false,
    }
}

fn in_settings(app: &App) -> bool {
    matches!(app.tlb.vm_subtab, VmSubtab::Tlb)
        && matches!(app.tlb.subtab, TlbSubtab::Settings)
        && app.run.tlb_enabled
}

fn in_entries(app: &App) -> bool {
    matches!(app.tlb.vm_subtab, VmSubtab::Tlb)
        && matches!(app.tlb.subtab, TlbSubtab::Entries)
        && app.run.tlb_enabled
}

fn in_tree(app: &App) -> bool {
    matches!(app.tlb.vm_subtab, VmSubtab::Tree)
}

fn in_vm_settings(app: &App) -> bool {
    matches!(app.tlb.vm_subtab, VmSubtab::Settings)
}

/// Flattened, visible navigation sequence. The TLB world's three sub-subtabs
/// only appear when the TLB is enabled; otherwise Tab cycles status ↔ tree.
fn cycle_subtab(app: &App, forward: bool) -> (VmSubtab, Option<TlbSubtab>) {
    let mut seq: Vec<(VmSubtab, Option<TlbSubtab>)> = vec![
        (VmSubtab::Status, None),
        (VmSubtab::Tree, None),
        (VmSubtab::Settings, None),
    ];
    if app.run.tlb_enabled {
        seq.push((VmSubtab::Tlb, Some(TlbSubtab::Stats)));
        seq.push((VmSubtab::Tlb, Some(TlbSubtab::Entries)));
        seq.push((VmSubtab::Tlb, Some(TlbSubtab::Settings)));
    }
    let cur = seq
        .iter()
        .position(|&(v, t)| v == app.tlb.vm_subtab && (t.is_none() || t == Some(app.tlb.subtab)))
        .unwrap_or(0);
    let n = seq.len();
    let idx = if forward {
        (cur + 1) % n
    } else {
        (cur + n - 1) % n
    };
    seq[idx]
}

/// Apply a navigation target, snapshotting the pending TLB config when entering
/// the Settings subtab (so the editor starts from the live config).
pub(crate) fn select(app: &mut App, vm: VmSubtab, sub: Option<TlbSubtab>) {
    app.tlb.vm_subtab = vm;
    // Entering either Settings panel snapshots the live TLB config so the
    // editor starts from the current geometry.
    if matches!(vm, VmSubtab::Settings) {
        app.tlb.pending = app.run.mem().mmu().tlb.config.clone();
        app.tlb.vm_edit_field = None;
        app.tlb.vm_edit_buf.clear();
        app.tlb.map_status = None;
    }
    if let Some(t) = sub {
        app.tlb.subtab = t;
        if matches!(t, TlbSubtab::Settings) {
            app.tlb.pending = app.run.mem().mmu().tlb.config.clone();
        }
    }
}

/// Numeric edit of a VM Settings field. Non-numeric controls are toggled /
/// cycled directly by mouse clicks (see `mouse.rs`).
fn handle_vm_field_edit(app: &mut App, code: KeyCode) -> bool {
    let signed = matches!(app.tlb.vm_edit_field, Some(VmSettingsField::Offset));
    match code {
        KeyCode::Esc => {
            app.tlb.vm_edit_field = None;
            app.tlb.vm_edit_buf.clear();
        }
        KeyCode::Enter | KeyCode::Tab => {
            app.commit_vm_edit();
            app.tlb.vm_edit_field = None;
            app.tlb.vm_edit_buf.clear();
        }
        KeyCode::Char(c) if c.is_ascii_digit() || (signed && c == '-') => {
            app.tlb.vm_edit_buf.push(c);
            app.tlb.map_status = None;
        }
        KeyCode::Backspace => {
            app.tlb.vm_edit_buf.pop();
            app.tlb.map_status = None;
        }
        _ => {}
    }
    true
}

fn handle_field_edit(app: &mut App, code: KeyCode) -> bool {
    let field = match app.tlb.edit_field {
        Some(f) => f,
        None => return false,
    };
    match code {
        KeyCode::Esc => {
            app.tlb.edit_field = None;
            app.tlb.edit_buf.clear();
        }
        KeyCode::Enter => {
            app.commit_tlb_edit();
        }
        KeyCode::Tab | KeyCode::Down => {
            app.commit_tlb_edit();
            let next = field.next();
            app.tlb.edit_field = Some(next);
            app.tlb.edit_buf = if next.is_numeric() {
                app.tlb_field_value_str(next)
            } else {
                String::new()
            };
        }
        KeyCode::Up => {
            app.commit_tlb_edit();
            let prev = field.prev();
            app.tlb.edit_field = Some(prev);
            app.tlb.edit_buf = if prev.is_numeric() {
                app.tlb_field_value_str(prev)
            } else {
                String::new()
            };
        }
        KeyCode::Left if !field.is_numeric() => {
            app.cycle_tlb_field(field, false);
        }
        KeyCode::Right if !field.is_numeric() => {
            app.cycle_tlb_field(field, true);
        }
        KeyCode::Char(c) if field.is_numeric() && c.is_ascii_digit() => {
            app.tlb.edit_buf.push(c);
            app.tlb.config_error = None;
            app.tlb.config_status = None;
        }
        KeyCode::Backspace if field.is_numeric() => {
            app.tlb.edit_buf.pop();
            app.tlb.config_error = None;
            app.tlb.config_status = None;
        }
        _ => {}
    }
    true
}
