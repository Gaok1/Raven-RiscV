#![allow(dead_code)]

mod app;
pub mod console;
pub mod debug_hitboxes;
pub mod editor;
mod input;
pub mod pipeline;
pub mod theme;
pub mod tutorial;
pub mod view;

pub use app::{App, CpiConfig, run};
pub use console::Console;
