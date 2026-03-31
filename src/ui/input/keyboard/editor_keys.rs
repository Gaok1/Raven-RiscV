use crate::ui::app::App;
use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle(app: &mut App, key: KeyEvent, _shift: bool) -> bool {
    match key.code {
        KeyCode::Up => {
            app.editor.buf.move_up();
            true
        }
        KeyCode::Down => {
            app.editor.buf.move_down();
            true
        }
        _ => false,
    }
}
