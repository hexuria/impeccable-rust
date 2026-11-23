//! The one error type: a message for stderr and a process exit code.

/// A command that could not finish.
#[derive(Debug)]
pub struct Failure {
    pub message: String,
    pub code: u8,
}

impl Failure {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: 1,
        }
    }
}

pub type Outcome<T> = Result<T, Failure>;

/// Returns early with a [`Failure`] built from a format string.
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::failure::Failure::new(format!($($arg)*)))
    };
}
pub(crate) use bail;

/// Prints a status line to stderr, so stdout stays the command's own output.
pub fn say(message: &str) {
    eprintln!("impeccable: {message}");
}
