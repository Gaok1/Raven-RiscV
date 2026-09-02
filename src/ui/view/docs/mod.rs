mod chrome;
mod content;
mod free_page;
mod instr_ref;

pub(crate) use self::chrome::{all_mask, build_docs_filter_bar, filter_items, visible_pages};
pub(crate) use self::free_page::free_page_line_count;
pub(crate) use self::instr_ref::docs_body_line_count;

use crate::ui::app::DocsPage;
use crate::ui::view::App;
use ratatui::prelude::*;

pub(super) fn render_docs(f: &mut Frame, area: Rect, app: &App) {
    match app.docs.page {
        DocsPage::InstrRef => instr_ref::render(f, area, app),
        DocsPage::Syscalls => free_page::render(
            f,
            area,
            app,
            content::syscalls::syscall_lines(app.docs.lang, app.architecture.descriptor().id),
        ),
        DocsPage::MemoryMap => free_page::render(
            f,
            area,
            app,
            content::memory_map::memory_map_lines(app.docs.lang),
        ),
        DocsPage::FcacheRef => free_page::render(
            f,
            area,
            app,
            content::fcache_ref::fcache_ref_lines(app.docs.lang),
        ),
    }
}


