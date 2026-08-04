use crate::ui::app::App;
use crate::ui::view::docs::{all_mask, filter_items, visible_pages};
use crossterm::event::{KeyCode, KeyEvent};

use super::paste::clamp_docs_scroll;

pub(super) fn handle(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        // Tab and the number keys walk the pages this architecture actually
        // has, so neither can land on one that is not drawn.
        KeyCode::Tab => {
            let pages = visible_pages(app);
            let next = pages
                .iter()
                .position(|page| *page == app.docs.page)
                .map_or(0, |index| (index + 1) % pages.len().max(1));
            if let Some(page) = pages.get(next) {
                app.docs.page = *page;
                app.docs.scroll = 0;
            }
            true
        }
        KeyCode::Char(digit @ '1'..='9') => {
            let index = digit as usize - '1' as usize;
            if let Some(page) = visible_pages(app).get(index) {
                app.docs.page = *page;
                app.docs.scroll = 0;
            }
            true
        }
        KeyCode::Char('l') if !app.docs.search_open => {
            app.docs.lang = app.docs.lang.toggle();
            true
        }
        KeyCode::Up => {
            app.docs.scroll = app.docs.scroll.saturating_sub(1);
            clamp_docs_scroll(app);
            true
        }
        KeyCode::Down => {
            app.docs.scroll = app.docs.scroll.saturating_add(1);
            clamp_docs_scroll(app);
            true
        }
        KeyCode::PageUp => {
            app.docs.scroll = app.docs.scroll.saturating_sub(10);
            clamp_docs_scroll(app);
            true
        }
        KeyCode::PageDown => {
            app.docs.scroll = app.docs.scroll.saturating_add(10);
            clamp_docs_scroll(app);
            true
        }
        KeyCode::Left if !app.docs.search_open => {
            let n = filter_items(app).len();
            app.docs.filter_cursor = if app.docs.filter_cursor == 0 {
                n - 1
            } else {
                app.docs.filter_cursor - 1
            };
            true
        }
        KeyCode::Right if !app.docs.search_open => {
            let n = filter_items(app).len();
            app.docs.filter_cursor = (app.docs.filter_cursor + 1) % n;
            true
        }
        KeyCode::Char(' ') if !app.docs.search_open => {
            if app.docs.filter_cursor == 0 {
                app.docs.type_filter = all_mask(app);
            } else if let Some(&(_, bit, _)) = filter_items(app).get(app.docs.filter_cursor) {
                app.docs.type_filter ^= bit;
            }
            app.docs.scroll = 0;
            true
        }
        _ => false,
    }
}
