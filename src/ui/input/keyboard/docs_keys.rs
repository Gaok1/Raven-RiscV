use crate::ui::app::{App, DocsPage};
use crate::ui::view::docs::{ALL_MASK, FILTER_ITEMS};
use crossterm::event::{KeyCode, KeyEvent};

use super::paste::clamp_docs_scroll;

pub(super) fn handle(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Tab => {
            app.docs.page = app.docs.page.next();
            app.docs.scroll = 0;
            true
        }
        KeyCode::Char('1') => {
            app.docs.page = DocsPage::InstrRef;
            app.docs.scroll = 0;
            true
        }
        KeyCode::Char('2') => {
            app.docs.page = DocsPage::Syscalls;
            app.docs.scroll = 0;
            true
        }
        KeyCode::Char('3') => {
            app.docs.page = DocsPage::MemoryMap;
            app.docs.scroll = 0;
            true
        }
        KeyCode::Char('4') => {
            app.docs.page = DocsPage::FcacheRef;
            app.docs.scroll = 0;
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
            let n = FILTER_ITEMS.len();
            app.docs.filter_cursor = if app.docs.filter_cursor == 0 {
                n - 1
            } else {
                app.docs.filter_cursor - 1
            };
            true
        }
        KeyCode::Right if !app.docs.search_open => {
            let n = FILTER_ITEMS.len();
            app.docs.filter_cursor = (app.docs.filter_cursor + 1) % n;
            true
        }
        KeyCode::Char(' ') if !app.docs.search_open => {
            if app.docs.filter_cursor == 0 {
                app.docs.type_filter = ALL_MASK;
            } else {
                let bit = FILTER_ITEMS[app.docs.filter_cursor].1;
                app.docs.type_filter ^= bit;
            }
            app.docs.scroll = 0;
            true
        }
        _ => false,
    }
}
