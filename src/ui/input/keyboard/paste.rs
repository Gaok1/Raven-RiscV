use crate::ui::app::{App, DocsPage, Tab};
use crate::ui::view::docs::{docs_body_line_count, free_page_line_count};
use crossterm::terminal;

use super::editor_shared::mark_editor_edited;
use super::serialization::{apply_imem_search, apply_mem_search};

pub fn paste_from_terminal(app: &mut App, text: &str) {
    if matches!(app.tab, Tab::Run) && app.run.imem_search_open {
        paste_imem_search(app, text);
        return;
    }
    if matches!(app.tab, Tab::Run) && app.run.mem_search_open {
        paste_mem_search(app, text);
        return;
    }

    if matches!(app.tab, Tab::Editor) {
        paste_editor(app, text);
    }
}

pub(super) fn paste_editor(app: &mut App, text: &str) {
    app.editor.buf.paste_text(text);
    mark_editor_edited(app);
}

pub(crate) fn paste_imem_search(app: &mut App, text: &str) {
    let sanitized: String = text.chars().filter(|&c| c != '\r' && c != '\n').collect();
    if sanitized.is_empty() {
        return;
    }
    app.run.imem_search_query.push_str(&sanitized);
    apply_imem_search(app);
}

pub(crate) fn paste_mem_search(app: &mut App, text: &str) {
    let sanitized: String = text
        .chars()
        .filter(|&c| c != '\r' && c != '\n' && !c.is_whitespace())
        .collect();
    let sanitized = sanitized
        .strip_prefix("0x")
        .or_else(|| sanitized.strip_prefix("0X"))
        .unwrap_or(&sanitized);
    if sanitized.is_empty() {
        return;
    }
    app.run.mem_search_query.push_str(sanitized);
    apply_mem_search(app);
}

pub(super) fn clamp_docs_scroll(app: &mut App) {
    if let Ok((_, h)) = terminal::size() {
        let viewport_h = h.saturating_sub(6) as usize;
        if viewport_h == 0 {
            app.docs.scroll = 0;
            return;
        }

        let total = match app.docs.page {
            DocsPage::InstrRef => {
                let vp = viewport_h.saturating_sub(6);
                let q = app.docs.search_query.clone();
                docs_body_line_count(80, &q, app.docs.type_filter).saturating_sub(vp)
            }
            p => {
                free_page_line_count(p, app.docs.lang).saturating_sub(viewport_h.saturating_sub(2))
            }
        };

        if app.docs.scroll > total {
            app.docs.scroll = total;
        }
    }
}
