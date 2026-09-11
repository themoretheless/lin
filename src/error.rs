use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Compile,
    Runtime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub message: String,
    pub pos: Option<usize>,
    pub phase: Phase,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            pos: None,
            phase: Phase::Compile,
        }
    }

    pub fn at(pos: usize, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            pos: Some(pos),
            phase: Phase::Compile,
        }
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            pos: None,
            phase: Phase::Runtime,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.phase {
            Phase::Compile => "compile error",
            Phase::Runtime => "runtime error",
        };
        match self.pos {
            Some(p) => write!(f, "{kind} at {}: {}", p, self.message),
            None => write!(f, "{kind}: {}", self.message),
        }
    }
}

impl std::error::Error for Error {}
