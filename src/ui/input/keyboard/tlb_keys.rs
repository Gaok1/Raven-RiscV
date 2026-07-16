// Keyboard handler for the top-level Virtual Memory tab.

use crate::ui::app::{App, VmSettingsField, VmSubtab};
use crossterm::event::{KeyCode, KeyEvent};

use super::serialization::capture_session_snapshot;

pub(super) fn handle(app: &mut App, key: KeyEvent) -> bool {
    // Numeric edit of a VM Settings field consumes most keys.
    if in_settings(app) && app.tlb.vm_edit_field.is_some() {
        return handle_vm_field_edit(app, key.code);
    }

    match key.code {
        KeyCode::Tab => {
            select(app, next_subtab(app.tlb.vm_subtab, true));
            true
        }
        KeyCode::BackTab => {
            select(app, next_subtab(app.tlb.vm_subtab, false));
            true
        }
        // ── Execution keys (mirror the Cache tab; off in Settings so they
        //    never collide with field editing) ─────────────────────────────
        KeyCode::Char('r') if !in_settings(app) => {
            app.restart_simulation();
            true
        }
        KeyCode::Char('p') | KeyCode::Char(' ') if !in_settings(app) => {
            if app.run.is_running {
                app.run.is_running = false;
            } else if app.core_status(app.selected_core) == crate::ui::app::HartLifecycle::Paused
                || !app.run.faulted
            {
                app.resume_selected_hart();
                if app.can_start_run() {
                    app.run.is_running = true;
                }
            }
            true
        }
        KeyCode::Char('f') if !in_settings(app) => {
            app.run.speed = app.run.speed.cycle();
            true
        }
        // ── Stats: session-snapshot capture + history (shared with Cache) ──
        KeyCode::Char('s') if in_stats(app) => {
            capture_session_snapshot(app);
            true
        }
        KeyCode::Char('d') | KeyCode::Char('D')
            if in_stats(app) && !app.cache.session_history.is_empty() =>
        {
            app.delete_selected_snapshot();
            true
        }
        KeyCode::Enter
            if in_stats(app) && !app.cache.session_history.is_empty() && !app.run.is_running =>
        {
            let idx = app
                .cache
                .history_scroll
                .min(app.cache.session_history.len() - 1);
            app.cache.viewing_snapshot = Some(idx);
            true
        }
        // ── Per-subtab scrolling ────────────────────────────────────────────
        KeyCode::Up => {
            match app.tlb.vm_subtab {
                VmSubtab::Map => {
                    app.tlb.page_tree_scroll = app.tlb.page_tree_scroll.saturating_sub(1);
                }
                VmSubtab::Settings => {
                    app.tlb.vm_settings_scroll = app.tlb.vm_settings_scroll.saturating_sub(1);
                }
                VmSubtab::Tlb => {
                    app.tlb.entries_scroll = app.tlb.entries_scroll.saturating_sub(1);
                }
                VmSubtab::Stats => {
                    app.cache.history_scroll = app.cache.history_scroll.saturating_sub(1);
                }
                VmSubtab::Overview => {}
            }
            true
        }
        KeyCode::Down => {
            match app.tlb.vm_subtab {
                VmSubtab::Map => {
                    let max = app.tlb.page_tree_max_scroll.get();
                    app.tlb.page_tree_scroll = app.tlb.page_tree_scroll.saturating_add(1).min(max);
                }
                VmSubtab::Settings => {
                    let max = app.tlb.vm_settings_max_scroll.get();
                    app.tlb.vm_settings_scroll =
                        app.tlb.vm_settings_scroll.saturating_add(1).min(max);
                }
                VmSubtab::Tlb => {
                    let total = app.run.mem().mmu().tlb.entries.len();
                    let next = app.tlb.entries_scroll.saturating_add(1);
                    app.tlb.entries_scroll = next.min(total.saturating_sub(1));
                }
                VmSubtab::Stats => {
                    if !app.cache.session_history.is_empty() {
                        app.cache.history_scroll = (app.cache.history_scroll + 1)
                            .min(app.cache.session_history.len() - 1);
                    }
                }
                VmSubtab::Overview => {}
            }
            true
        }
        KeyCode::PageUp if matches!(app.tlb.vm_subtab, VmSubtab::Map) => {
            app.tlb.page_tree_scroll = app.tlb.page_tree_scroll.saturating_sub(10);
            true
        }
        KeyCode::PageDown if matches!(app.tlb.vm_subtab, VmSubtab::Map) => {
            let max = app.tlb.page_tree_max_scroll.get();
            app.tlb.page_tree_scroll = app.tlb.page_tree_scroll.saturating_add(10).min(max);
            true
        }
        _ => false,
    }
}

fn in_settings(app: &App) -> bool {
    matches!(app.tlb.vm_subtab, VmSubtab::Settings)
}

fn in_stats(app: &App) -> bool {
    matches!(app.tlb.vm_subtab, VmSubtab::Stats)
}

fn next_subtab(cur: VmSubtab, forward: bool) -> VmSubtab {
    let seq = VmSubtab::ALL;
    let i = seq.iter().position(|&s| s == cur).unwrap_or(0);
    let n = seq.len();
    seq[if forward { (i + 1) % n } else { (i + n - 1) % n }]
}

/// Apply a navigation target, snapshotting the pending TLB config when entering
/// the Settings subtab (so the editor starts from the live config).
pub(crate) fn select(app: &mut App, vm: VmSubtab) {
    app.tlb.vm_subtab = vm;
    if matches!(vm, VmSubtab::Settings) {
        app.tlb.pending = app.run.mem().mmu().tlb.config.clone();
        app.tlb.vm_edit_field = None;
        app.tlb.vm_edit_buf.clear();
        app.tlb.map_status = None;
    }
}

/// Ordered numeric fields of the VM Settings panel, matching the rendered row
/// order for the current mode / map kind. Tab and ↑↓ walk this sequence.
fn vm_numeric_field_order(app: &App) -> Vec<VmSettingsField> {
    use crate::falcon::mmu::{MapKind, VmMode};
    let mut v = Vec::new();
    if matches!(app.vm_mode(), VmMode::Custom) {
        v.push(VmSettingsField::OffsetBits);
        for i in 0..app.tlb.pending_scheme.level_bits.len() {
            v.push(VmSettingsField::LevelBits(i));
        }
    }
    if matches!(app.tlb.pending_map.kind, MapKind::Offset(_)) {
        v.push(VmSettingsField::Offset);
    }
    v.push(VmSettingsField::Asid);
    v.push(VmSettingsField::TlbEntries);
    v.push(VmSettingsField::TlbAssoc);
    v.push(VmSettingsField::TlbHitLat);
    v.push(VmSettingsField::TlbMissLat);
    v
}

/// Commit the current edit and move focus to the adjacent numeric field.
fn move_vm_edit(app: &mut App, forward: bool) {
    let Some(cur) = app.tlb.vm_edit_field else {
        return;
    };
    app.commit_vm_edit();
    let order = vm_numeric_field_order(app);
    let i = order.iter().position(|&f| f == cur).unwrap_or(0);
    let n = order.len();
    let next = order[if forward { (i + 1) % n } else { (i + n - 1) % n }];
    app.tlb.vm_edit_field = Some(next);
    app.tlb.vm_edit_buf = app.vm_field_value_str(next);
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
        KeyCode::Enter => {
            app.commit_vm_edit();
            app.tlb.vm_edit_field = None;
            app.tlb.vm_edit_buf.clear();
        }
        KeyCode::Tab | KeyCode::Down => {
            move_vm_edit(app, true);
        }
        KeyCode::BackTab | KeyCode::Up => {
            move_vm_edit(app, false);
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
