mod cache_keys;
mod config_keys;
mod docs_keys;
mod editor_keys;
mod editor_shared;
mod find_replace;
mod insert_mode;
mod intercepts;
mod paste;
mod pipeline_keys;
mod run_keys;
mod serialization;

pub(crate) use self::paste::paste_from_terminal;
#[cfg(test)]
use self::paste::{paste_imem_search, paste_mem_search};
#[cfg(test)]
use self::serialization::{
    apply_imem_search, capture_snapshot, serialize_pipeline_results_pstats, serialize_results_csv,
    serialize_results_fstats,
};
pub(crate) use self::serialization::{
    apply_fcache_text, apply_pcfg_text, apply_rcfg_text, do_export_cfg, do_export_pcfg,
    do_export_pipeline_results, do_export_results, do_import_cfg, do_import_pcfg, do_import_rcfg,
    do_export_rcfg, open_path_input,
};

use crate::ui::app::{App, EditorMode, Tab};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::io;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyOutcome {
    Ignored,
    Handled,
    Quit,
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> io::Result<KeyOutcome> {
    if key.kind != KeyEventKind::Press {
        return Ok(KeyOutcome::Ignored);
    }

    if let Some(quit) = intercepts::handle_pre_find_intercepts(app, key) {
        return Ok(if quit {
            KeyOutcome::Quit
        } else {
            KeyOutcome::Handled
        });
    }

    if find_replace::handle(app, key) {
        return Ok(KeyOutcome::Handled);
    }

    if let Some(quit) = intercepts::handle_post_find_intercepts(app, key) {
        return Ok(if quit {
            KeyOutcome::Quit
        } else {
            KeyOutcome::Handled
        });
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    if intercepts::handle_global_shortcuts(app, key, ctrl) {
        return Ok(KeyOutcome::Handled);
    }

    if run_keys::handle_execution_key(app, key.code) {
        return Ok(KeyOutcome::Handled);
    }

    let handled = match app.mode {
        EditorMode::Insert => insert_mode::handle(app, key, ctrl, shift),
        EditorMode::Command => handle_command_mode(app, key, ctrl, shift),
    };

    Ok(if handled {
        KeyOutcome::Handled
    } else {
        KeyOutcome::Ignored
    })
}

fn handle_command_mode(app: &mut App, key: KeyEvent, ctrl: bool, shift: bool) -> bool {
    if matches!(key.code, KeyCode::Esc) {
        app.show_exit_popup = true;
        return true;
    }

    if editor_shared::handle_command_readonly_guard(app, key, ctrl) {
        return true;
    }

    if editor_shared::handle_common_shortcuts(app, key, ctrl) {
        return true;
    }

    if ctrl {
        match (key.code, app.tab) {
            (KeyCode::Char('e'), Tab::Cache) => {
                serialization::do_export_cfg(app);
                return true;
            }
            (KeyCode::Char('l'), Tab::Cache) => {
                serialization::do_import_cfg(app);
                return true;
            }
            (KeyCode::Char('e'), Tab::Config) => {
                serialization::do_export_rcfg(app);
                return true;
            }
            (KeyCode::Char('l'), Tab::Config) => {
                serialization::do_import_rcfg(app);
                return true;
            }
            (KeyCode::Char('e'), Tab::Pipeline) => {
                serialization::do_export_pcfg(app);
                return true;
            }
            (KeyCode::Char('l'), Tab::Pipeline) => {
                serialization::do_import_pcfg(app);
                return true;
            }
            (KeyCode::Char('r'), Tab::Cache) => {
                serialization::do_export_results(app);
                return true;
            }
            (KeyCode::Char('r'), Tab::Pipeline) => {
                serialization::do_export_pipeline_results(app);
                return true;
            }
            _ => {}
        }
    }

    match app.tab {
        Tab::Run => run_keys::handle(app, key, ctrl),
        Tab::Editor => editor_keys::handle(app, key, shift),
        Tab::Cache => cache_keys::handle(app, key),
        Tab::Pipeline => pipeline_keys::handle(app, key),
        Tab::Docs => docs_keys::handle(app, key),
        Tab::Config => config_keys::handle(app, key),
        Tab::Activity => crate::guided_learning::keys::handle(app, key.code),
    }
}

#[cfg(test)]
#[path = "../../../../tests/support/ui_input_keyboard.rs"]
mod tests;
