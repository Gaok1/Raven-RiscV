use crate::ui::app::App;
use ratatui::layout::Rect;

pub mod render;
mod steps;

pub use steps::get_steps;

pub type TargetFn = fn(Rect, &App) -> Option<Rect>;
pub type SetupFn = fn(&mut App);

pub struct TutorialStep {
    pub title_en: &'static str,
    pub title_pt: &'static str,
    pub body_en: &'static str,
    pub body_pt: &'static str,
    pub target: TargetFn,
    pub setup: Option<SetupFn>,
}

/// Advance to next step, calling setup if present. Closes tutorial on last step.
pub fn advance_tutorial(app: &mut App) {
    let next = app.tutorial.step_idx + 1;
    let total = get_steps(app.tutorial.tab).len();
    if next >= total {
        app.tutorial.active = false;
    } else {
        app.tutorial.step_idx = next;
        let setup = get_steps(app.tutorial.tab)[next].setup;
        if let Some(f) = setup {
            f(app);
        }
    }
}

/// Retreat to previous step, calling setup if present.
pub fn retreat_tutorial(app: &mut App) {
    if app.tutorial.step_idx == 0 {
        return;
    }
    let prev = app.tutorial.step_idx - 1;
    app.tutorial.step_idx = prev;
    let setup = get_steps(app.tutorial.tab)[prev].setup;
    if let Some(f) = setup {
        f(app);
    }
}

/// Open tutorial for the given tab, running step-0 setup.
pub fn start_tutorial(app: &mut App) {
    let tab = app.tab;
    app.tutorial.tab = tab;
    app.tutorial.step_idx = 0;
    app.tutorial.active = true;
    let setup = get_steps(tab)[0].setup;
    if let Some(f) = setup {
        f(app);
    }
}
