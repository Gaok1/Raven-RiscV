// MVP: placeholder (depois podemos trocar por thiserror)
#[allow(dead_code)]
pub enum FalconError {
    Decode(&'static str),
    Bus(&'static str),
}
