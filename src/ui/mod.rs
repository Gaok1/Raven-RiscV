#![allow(dead_code)]

mod app;
pub mod console;
pub mod debug_hitboxes;
pub mod editor;
mod input;
pub mod pipeline;
mod platform;
pub mod theme;
pub mod tutorial;
pub mod view;

pub(crate) use app::Tab;
pub use app::{App, CpiConfig, run};
pub use console::Console;
pub(crate) use input::keyboard::{apply_fcache_text, apply_pcfg_text, apply_rcfg_text};
