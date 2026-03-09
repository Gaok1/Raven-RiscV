mod loader;
pub mod elf;

pub use loader::{load_words, load_bytes, zero_bytes};
pub use elf::load_elf;
