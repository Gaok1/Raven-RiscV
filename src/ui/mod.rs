#![allow(dead_code)]

mod app;
pub mod editor;
mod input;
pub mod theme;
pub mod view;
pub mod console;
pub mod tutorial;

pub use app::{run, App};
pub use console::Console;
