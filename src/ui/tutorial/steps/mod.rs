#![allow(private_interfaces)]

use crate::ui::app::Tab;
use super::TutorialStep;

mod editor;
mod run;
mod cache;

pub fn get_steps(tab: Tab) -> &'static [TutorialStep] {
    match tab {
        Tab::Editor => editor::STEPS,
        Tab::Run    => run::STEPS,
        Tab::Cache  => cache::STEPS,
        Tab::Docs   => &[],
    }
}
