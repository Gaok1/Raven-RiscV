use std::fmt;

#[derive(Debug, Clone)]
pub struct AsmError {
    pub line: usize,
    pub msg: String,
}

impl fmt::Display for AsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line + 1, self.msg)
    }
}

