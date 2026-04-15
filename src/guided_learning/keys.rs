use crossterm::event::KeyCode;
use crate::ui::App;
use super::{GuidedPreset, apply_preset};

/// Handle a key press on the Activity tab.
/// Returns `true` if the key was consumed.
pub fn handle(app: &mut App, key: KeyCode) -> bool {
    let total = GuidedPreset::all().len();
    if total == 0 {
        return false;
    }

    match key {
        KeyCode::Up => {
            if app.activity.cursor > 0 {
                app.activity.cursor -= 1;
            } else {
                app.activity.cursor = total - 1;
            }
            true
        }
        KeyCode::Down => {
            app.activity.cursor = (app.activity.cursor + 1) % total;
            true
        }
        KeyCode::Enter => {
            let preset = GuidedPreset::all()[app.activity.cursor];
            match apply_preset(app, preset) {
                Ok(()) => {
                    // Update state — must re-borrow after apply_preset moved app
                    app.activity.last_applied = Some(preset);
                    app.activity.status_msg =
                        Some(format!("{} aplicado", preset.label()));
                    app.activity.status_err = None;
                }
                Err(msg) => {
                    app.activity.status_err = Some(msg);
                    app.activity.status_msg = None;
                }
            }
            true
        }
        _ => false,
    }
}
