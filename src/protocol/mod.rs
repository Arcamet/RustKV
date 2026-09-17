mod codec;
mod command;
mod response;

pub use codec::{ProtocolError, read_command, read_response, write_command, write_response};
pub use command::Command;
pub use response::{Response, StatsSnapshot};

pub const PROTOCOL_VERSION: u8 = 1;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_KEY_BYTES: usize = 4 * 1024;
pub const MAX_VALUE_BYTES: usize = 1_000_000;

#[cfg(test)]
mod tests;
