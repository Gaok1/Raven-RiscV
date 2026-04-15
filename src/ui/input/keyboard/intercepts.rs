use crate::ui::app::{App, Tab};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::paste::{paste_imem_search, paste_mem_search};
use super::serialization::{
    apply_imem_search, apply_mem_search, dispatch_path_input, refresh_path_completions,
};

pub(super) fn handle_pre_find_intercepts(app: &mut App, key: KeyEvent) -> Option<bool> {
    if app.splash_start.is_some() {
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => {
                app.splash_start = None;
                return Some(false);
            }
            KeyCode::Esc => {
                app.show_exit_popup = true;
                return Some(false);
            }
            _ => {
                app.splash_start = None;
            }
        }
    }

    if app.path_input.open {
        match key.code {
            KeyCode::Esc => {
                app.path_input.open = false;
                app.path_input.query.clear();
                app.path_input.completions.clear();
            }
            KeyCode::Enter => {
                let q = app.path_input.query.trim().to_string();
                let selected = app
                    .path_input
                    .completions
                    .get(app.path_input.completion_sel)
                    .cloned();
                let typed_path = std::path::PathBuf::from(&q);

                let chosen = if q.ends_with('/') || typed_path.is_dir() {
                    selected.unwrap_or(q)
                } else if let Some(sel) = selected {
                    if sel != q {
                        sel
                    } else {
                        q
                    }
                } else {
                    q
                };

                let chosen_path = std::path::PathBuf::from(&chosen);
                if chosen.ends_with('/') || chosen_path.is_dir() {
                    if chosen.ends_with('/') {
                        app.path_input.query = chosen;
                    } else {
                        app.path_input.query = format!("{chosen}/");
                    }
                    refresh_path_completions(&mut app.path_input);
                } else {
                    let action = app.path_input.action.clone();
                    app.path_input.open = false;
                    app.path_input.query.clear();
                    app.path_input.completions.clear();
                    dispatch_path_input(app, action, chosen_path);
                }
            }
            KeyCode::Tab => {
                if !app.path_input.completions.is_empty() {
                    let sel = app.path_input.completion_sel;
                    let selected = app.path_input.completions[sel].clone();
                    if selected.ends_with('/') {
                        app.path_input.query = selected;
                        refresh_path_completions(&mut app.path_input);
                    } else if app.path_input.query == selected {
                        let next = (sel + 1) % app.path_input.completions.len();
                        app.path_input.completion_sel = next;
                        app.path_input.query = app.path_input.completions[next].clone();
                    } else {
                        app.path_input.query = selected;
                    }
                }
            }
            KeyCode::Down => {
                if !app.path_input.completions.is_empty() {
                    app.path_input.completion_sel =
                        (app.path_input.completion_sel + 1) % app.path_input.completions.len();
                    app.path_input.query =
                        app.path_input.completions[app.path_input.completion_sel].clone();
                }
            }
            KeyCode::Up => {
                if !app.path_input.completions.is_empty() {
                    let n = app.path_input.completions.len();
                    app.path_input.completion_sel = if app.path_input.completion_sel == 0 {
                        n - 1
                    } else {
                        app.path_input.completion_sel - 1
                    };
                    app.path_input.query =
                        app.path_input.completions[app.path_input.completion_sel].clone();
                }
            }
            KeyCode::Backspace => {
                app.path_input.query.pop();
                refresh_path_completions(&mut app.path_input);
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.path_input.query.push(c);
                refresh_path_completions(&mut app.path_input);
            }
            _ => {}
        }
        return Some(false);
    }

    if app.console.reading {
        match key.code {
            KeyCode::Char(c) => app.console.current.push(c),
            KeyCode::Backspace => {
                app.console.current.pop();
            }
            KeyCode::Enter => {
                let line = std::mem::take(&mut app.console.current);
                app.console.push_input(line);
                app.console.reading = false;
                if app.can_start_run() {
                    app.run.is_running = true;
                }
            }
            _ => {}
        }
        return Some(false);
    }

    if app.show_exit_popup {
        match key.code {
            KeyCode::Esc => app.show_exit_popup = false,
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => return Some(true),
            _ => {}
        }
        return Some(false);
    }

    if app.tutorial.active {
        use crate::ui::tutorial::{advance_tutorial, retreat_tutorial};
        match key.code {
            KeyCode::Esc => {
                app.tutorial.active = false;
            }
            KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => advance_tutorial(app),
            KeyCode::Left | KeyCode::Backspace => retreat_tutorial(app),
            KeyCode::Char('l') | KeyCode::Char('L') => {
                app.tutorial.lang = app.tutorial.lang.toggle();
            }
            _ => {}
        }
        return Some(false);
    }

    if app.help_open {
        let pages_count: usize = match app.tab {
            Tab::Run => 2,
            Tab::Editor | Tab::Cache | Tab::Pipeline | Tab::Docs | Tab::Config | Tab::Activity => 1,
        };
        match key.code {
            KeyCode::Esc => {
                app.help_open = false;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                app.help_page = app.help_page.saturating_sub(1);
            }
            KeyCode::Right | KeyCode::Char('l') => {
                app.help_page = (app.help_page + 1).min(pages_count.saturating_sub(1));
            }
            _ => {
                app.help_open = false;
            }
        }
        return Some(false);
    }

    None
}

pub(super) fn handle_post_find_intercepts(app: &mut App, key: KeyEvent) -> Option<bool> {
    if matches!(app.tab, Tab::Run) && app.run.imem_search_open {
        match key.code {
            KeyCode::Esc => {
                app.run.imem_search_open = false;
                app.run.imem_search_query.clear();
                app.run.imem_search_matches.clear();
                app.run.imem_search_cursor = 0;
                app.run.imem_search_match_count = 0;
            }
            KeyCode::Enter => {
                let n = app.run.imem_search_matches.len();
                if n > 0 {
                    app.run.imem_search_cursor = (app.run.imem_search_cursor + 1) % n;
                    let addr = app.run.imem_search_matches[app.run.imem_search_cursor];
                    app.scroll_imem_to_addr(addr);
                }
            }
            KeyCode::Backspace => {
                app.run.imem_search_query.pop();
                apply_imem_search(app);
            }
            KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let recent_bracketed = app
                    .last_bracketed_paste
                    .is_some_and(|t| t.elapsed().as_millis() < 100);
                if !recent_bracketed {
                    let text = app.clipboard.as_mut().and_then(|clip| clip.get_text().ok());
                    if let Some(text) = text {
                        paste_imem_search(app, &text);
                    }
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.run.imem_search_query.push(c);
                apply_imem_search(app);
            }
            _ => {}
        }
        return Some(false);
    }

    if matches!(app.tab, Tab::Run) && app.run_sidebar_shows_memory() && app.run.mem_search_open {
        match key.code {
            KeyCode::Esc => {
                app.run.mem_search_open = false;
                app.run.mem_search_query.clear();
            }
            KeyCode::Enter => {
                app.run.mem_search_open = false;
                app.run.mem_search_query.clear();
            }
            KeyCode::Backspace => {
                app.run.mem_search_query.pop();
            }
            KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let recent_bracketed = app
                    .last_bracketed_paste
                    .is_some_and(|t| t.elapsed().as_millis() < 100);
                if !recent_bracketed {
                    let text = app.clipboard.as_mut().and_then(|clip| clip.get_text().ok());
                    if let Some(text) = text {
                        paste_mem_search(app, &text);
                    }
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.run.mem_search_query.push(c);
            }
            _ => {}
        }
        apply_mem_search(app);
        return Some(false);
    }

    if matches!(app.tab, Tab::Docs) && app.docs.search_open {
        match key.code {
            KeyCode::Esc => {
                app.docs.search_open = false;
                app.docs.search_query.clear();
            }
            KeyCode::Backspace => {
                app.docs.search_query.pop();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.docs.search_query.push(c);
            }
            _ => {}
        }
        return Some(false);
    }

    if app.editor.elf_prompt_open && matches!(app.tab, Tab::Editor) {
        if key.code == KeyCode::Esc {
            app.editor.elf_prompt_open = false;
        }
        return Some(false);
    }

    if app.cache.viewing_snapshot.is_some() && matches!(app.tab, Tab::Cache) {
        if key.code == KeyCode::Esc {
            app.cache.viewing_snapshot = None;
        }
        return Some(false);
    }

    None
}

pub(super) fn handle_global_shortcuts(app: &mut App, key: KeyEvent, ctrl: bool) -> bool {
    if matches!(app.tab, Tab::Run) && matches!(key.code, KeyCode::Char('R')) {
        app.restart_simulation();
        return true;
    }

    if matches!(app.tab, Tab::Run | Tab::Pipeline) {
        match key.code {
            KeyCode::Char('[') => {
                app.cycle_selected_core(-1);
                return true;
            }
            KeyCode::Char(']') => {
                app.cycle_selected_core(1);
                return true;
            }
            _ => {}
        }
    }

    if key.code == KeyCode::Char('?') {
        if !matches!(app.tab, Tab::Docs) && !crate::ui::tutorial::get_steps(app.tab).is_empty() {
            crate::ui::tutorial::start_tutorial(app);
        } else {
            app.help_open = !app.help_open;
            app.help_page = 0;
        }
        return true;
    }

    if key.code == KeyCode::F(9) && matches!(app.tab, Tab::Run) {
        let addr = app.run.hover_imem_addr.unwrap_or(app.run.cpu.pc);
        if app.run.breakpoints.contains(&addr) {
            app.run.breakpoints.remove(&addr);
        } else {
            app.run.breakpoints.insert(addr);
        }
        return true;
    }

    if ctrl && matches!(key.code, KeyCode::Char('g')) && matches!(app.tab, Tab::Run) {
        app.run.imem_search_open = !app.run.imem_search_open;
        if !app.run.imem_search_open {
            app.run.imem_search_query.clear();
            app.run.imem_search_matches.clear();
            app.run.imem_search_cursor = 0;
            app.run.imem_search_match_count = 0;
        }
        return true;
    }

    if ctrl && matches!(key.code, KeyCode::Char('f')) && matches!(app.tab, Tab::Run) {
        app.run.show_registers = false;
        app.run.show_dyn = false;
        app.run.mem_search_open = !app.run.mem_search_open;
        if !app.run.mem_search_open {
            app.run.mem_search_query.clear();
        }
        return true;
    }

    if ctrl && matches!(key.code, KeyCode::Char('f')) && matches!(app.tab, Tab::Docs) {
        app.docs.search_open = !app.docs.search_open;
        if !app.docs.search_open {
            app.docs.search_query.clear();
        }
        return true;
    }

    false
}
