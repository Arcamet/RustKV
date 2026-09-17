mod log;
mod record;

pub use log::AppendLog;
pub use record::LogRecord;

use std::error::Error;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum PersistenceError {
    Io(io::Error),
    InvalidHeader,
    InvalidRecord(&'static str),
    RecordTooLarge { actual: usize, max: usize },
    Corrupt { offset: u64, message: String },
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "persistence I/O error: {error}"),
            Self::InvalidHeader => f.write_str("invalid RustKV append-log header"),
            Self::InvalidRecord(message) => write!(f, "invalid append-log record: {message}"),
            Self::RecordTooLarge { actual, max } => {
                write!(f, "append-log record is {actual} bytes; maximum is {max}")
            }
            Self::Corrupt { offset, message } => {
                write!(f, "append log is corrupt at byte {offset}: {message}")
            }
        }
    }
}

impl Error for PersistenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for PersistenceError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[cfg(test)]
mod tests;
