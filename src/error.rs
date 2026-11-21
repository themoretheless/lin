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

/// Longest message an error keeps. Messages quote user input (names, literals); a
/// statement carrying a megabyte identifier must not produce a megabyte error.
const MAX_MESSAGE_BYTES: usize = 4096;

#[cold]
#[inline(never)]
fn truncate_message(m: &mut String) {
    use fmt::Write as _;
    let mut cut = MAX_MESSAGE_BYTES;
    while !m.is_char_boundary(cut) {
        cut -= 1;
    }
    let dropped = m.len() - cut;
    m.truncate(cut);
    let _ = write!(m, "… [{dropped} bytes truncated]");
}

#[cold]
#[inline(never)]
fn bounded(message: impl Into<String>) -> String {
    let mut m = message.into();
    if m.len() > MAX_MESSAGE_BYTES {
        truncate_message(&mut m);
    }
    m
}

impl Error {
    #[cold]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: bounded(message),
            pos: None,
            phase: Phase::Compile,
        }
    }

    #[cold]
    pub fn at(pos: usize, message: impl Into<String>) -> Self {
        Self {
            message: bounded(message),
            pos: Some(pos),
            phase: Phase::Compile,
        }
    }

    #[cold]
    pub fn runtime(message: impl Into<String>) -> Self {
        Self {
            message: bounded(message),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_messages_are_truncated_on_a_char_boundary() {
        let short = Error::new("short");
        assert_eq!(short.message, "short");
        let long = Error::runtime("é".repeat(5000));
        assert!(long.message.len() < MAX_MESSAGE_BYTES + 64);
        assert!(long.message.ends_with("bytes truncated]"));
        assert!(long.message.starts_with("éé"));
    }
}
