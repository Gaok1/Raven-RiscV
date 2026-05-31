// Keyboard handler for the top-level TLB / Virtual Memory tab.

use crate::ui::app::{App, TlbSubtab};
use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle(app: &mut App, key: KeyEvent) -> bool {
    // Field-edit mode in Config subtab consumes most keys.
    if matches!(app.tlb.subtab, TlbSubtab::Config) && app.tlb.edit_field.is_some() {
        return handle_field_edit(app, key.code);
    }

    match key.code {
        KeyCode::Tab => {
            app.tlb.subtab = match app.tlb.subtab {
                TlbSubtab::Stats => TlbSubtab::Entries,
                TlbSubtab::Entries => TlbSubtab::Status,
                TlbSubtab::Status => TlbSubtab::PageTree,
                TlbSubtab::PageTree => TlbSubtab::Config,
                TlbSubtab::Config => TlbSubtab::Stats,
            };
            // Snapshot pending config when entering Config.
            if matches!(app.tlb.subtab, TlbSubtab::Config) {
                app.tlb.pending = app.run.mem.mmu().tlb.config.clone();
            }
            true
        }
        KeyCode::Char('f') if matches!(app.tlb.subtab, TlbSubtab::Config) => {
            app.flush_tlb();
            true
        }
        KeyCode::Up if matches!(app.tlb.subtab, TlbSubtab::Entries) => {
            app.tlb.entries_scroll = app.tlb.entries_scroll.saturating_sub(1);
            true
        }
        KeyCode::Down if matches!(app.tlb.subtab, TlbSubtab::Entries) => {
            let total = app.run.mem.mmu().tlb.entries.len();
            let next = app.tlb.entries_scroll.saturating_add(1);
            app.tlb.entries_scroll = next.min(total.saturating_sub(1));
            true
        }
        KeyCode::Up if matches!(app.tlb.subtab, TlbSubtab::PageTree) => {
            app.tlb.page_tree_scroll = app.tlb.page_tree_scroll.saturating_sub(1);
            true
        }
        KeyCode::Down if matches!(app.tlb.subtab, TlbSubtab::PageTree) => {
            app.tlb.page_tree_scroll = app.tlb.page_tree_scroll.saturating_add(1);
            true
        }
        KeyCode::PageUp if matches!(app.tlb.subtab, TlbSubtab::PageTree) => {
            app.tlb.page_tree_scroll = app.tlb.page_tree_scroll.saturating_sub(10);
            true
        }
        KeyCode::PageDown if matches!(app.tlb.subtab, TlbSubtab::PageTree) => {
            app.tlb.page_tree_scroll = app.tlb.page_tree_scroll.saturating_add(10);
            true
        }
        _ => false,
    }
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
