use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Set {
        key: Vec<u8>,
        value: Vec<u8>,
        expires_in: Option<Duration>,
    },
    Get {
        key: Vec<u8>,
    },
    Delete {
        key: Vec<u8>,
    },
    Exists {
        key: Vec<u8>,
    },
    Stats,
}
