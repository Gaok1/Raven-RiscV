#![allow(private_interfaces)]

use super::TutorialStep;
use crate::ui::app::Tab;

mod cache;
mod editor;
mod pipeline;
mod run;

pub fn get_steps(tab: Tab) -> &'static [TutorialStep] {
    match tab {
        Tab::Editor => editor::STEPS,
        Tab::Run => run::STEPS,
        Tab::Cache => cache::STEPS,
        Tab::Pipeline => pipeline::STEPS,
        Tab::Docs => &[],
        Tab::Config => &[],
    }
}
