#![allow(dead_code)]

mod app;
pub mod console { pub use raven_riscv_engine::host::console::*; }
pub mod debug_hitboxes;
pub mod editor;
pub(crate) mod input;
pub mod pipeline;
mod platform;
pub mod screen { pub use raven_riscv_engine::host::screen::*; }
pub mod theme;
pub mod tutorial;
pub mod view;

pub(crate) use app::Tab;
pub use app::{App, CpiConfig, run};
pub use console::Console;
pub(crate) use input::keyboard::{apply_fcache_text, apply_pcfg_text, apply_rcfg_text};
