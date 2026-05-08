#[cfg(not(all(target_os = "linux", target_env = "musl")))]
pub(crate) use arboard::Clipboard;

#[cfg(not(all(target_os = "linux", target_env = "musl")))]
pub(crate) use rfd::FileDialog as OSFileDialog;

#[cfg(all(target_os = "linux", target_env = "musl"))]
pub(crate) struct Clipboard;

#[cfg(all(target_os = "linux", target_env = "musl"))]
impl Clipboard {
    pub(crate) fn new() -> Result<Self, std::io::Error> {
        Err(std::io::Error::other(
            "clipboard integration is unavailable on linux-musl builds",
        ))
    }

    pub(crate) fn set_text(&mut self, _text: String) -> Result<(), std::io::Error> {
        Err(std::io::Error::other(
            "clipboard integration is unavailable on linux-musl builds",
        ))
    }

    pub(crate) fn get_text(&mut self) -> Result<String, std::io::Error> {
        Err(std::io::Error::other(
            "clipboard integration is unavailable on linux-musl builds",
        ))
    }
}

#[cfg(all(target_os = "linux", target_env = "musl"))]
pub(crate) struct OSFileDialog;

#[cfg(all(target_os = "linux", target_env = "musl"))]
impl OSFileDialog {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn add_filter(self, _name: &str, _extensions: &[&str]) -> Self {
        self
    }

    pub(crate) fn set_file_name(self, _file_name: &str) -> Self {
        self
    }

    pub(crate) fn pick_file(self) -> Option<std::path::PathBuf> {
        None
    }

    pub(crate) fn save_file(self) -> Option<std::path::PathBuf> {
        None
    }
}
