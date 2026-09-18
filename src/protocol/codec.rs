use std::error::Error;
use std::fmt;
use std::io::{self, Read, Write};
use std::time::Duration;

use super::{
    Command, MAX_FRAME_BYTES, MAX_KEY_BYTES, MAX_VALUE_BYTES, PROTOCOL_VERSION, Response,
    StatsSnapshot,
};

const OP_SET: u8 = 1;
const OP_GET: u8 = 2;
const OP_DELETE: u8 = 3;
const OP_EXISTS: u8 = 4;
const OP_STATS: u8 = 5;
const OP_SET_EX: u8 = 6;

const RESPONSE_OK: u8 = 0;
const RESPONSE_VALUE: u8 = 1;
const RESPONSE_BOOLEAN: u8 = 2;
const RESPONSE_STATS: u8 = 3;
const RESPONSE_ERROR: u8 = 255;

#[derive(Debug)]
pub enum ProtocolError {
    Io(io::Error),
    Idle,
    UnexpectedEof,
    EmptyFrame,
    FrameTooLarge { actual: usize, max: usize },
    KeyTooLarge { actual: usize },
    ValueTooLarge { actual: usize },
    UnsupportedVersion(u8),
    UnknownTag(u8),
    TrailingBytes,
    Malformed(&'static str),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "protocol I/O error: {error}"),
            Self::Idle => f.write_str("no frame is currently available"),
            Self::UnexpectedEof => f.write_str("unexpected EOF inside a frame"),
            Self::EmptyFrame => f.write_str("empty frame"),
            Self::FrameTooLarge { actual, max } => {
                write!(f, "frame is {actual} bytes; maximum is {max}")
            }
            Self::KeyTooLarge { actual } => {
                write!(f, "key is {actual} bytes; maximum is {MAX_KEY_BYTES}")
            }
            Self::ValueTooLarge { actual } => {
                write!(f, "value is {actual} bytes; maximum is {MAX_VALUE_BYTES}")
            }
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported protocol version {version}")
            }
            Self::UnknownTag(tag) => write!(f, "unknown protocol tag {tag}"),
            Self::TrailingBytes => f.write_str("frame has trailing bytes"),
            Self::Malformed(message) => write!(f, "malformed frame: {message}"),
        }
    }
}

impl Error for ProtocolError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ProtocolError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn write_command<W: Write>(writer: &mut W, command: &Command) -> Result<(), ProtocolError> {
    let mut payload = vec![PROTOCOL_VERSION];
    match command {
        Command::Set {
            key,
            value,
            expires_in,
        } => {
            validate_key(key)?;
            validate_value(value)?;
            match expires_in {
                Some(duration) => {
                    if duration.is_zero() {
                        return Err(ProtocolError::Malformed("TTL must be greater than zero"));
                    }
                    payload.push(OP_SET_EX);
                    put_bytes(&mut payload, key)?;
                    put_bytes(&mut payload, value)?;
                    payload.extend_from_slice(&duration.as_secs().to_be_bytes());
                }
                None => {
                    payload.push(OP_SET);
                    put_bytes(&mut payload, key)?;
                    put_bytes(&mut payload, value)?;
                }
            }
        }
        Command::Get { key } => {
            validate_key(key)?;
            payload.push(OP_GET);
            put_bytes(&mut payload, key)?;
        }
        Command::Delete { key } => {
            validate_key(key)?;
            payload.push(OP_DELETE);
            put_bytes(&mut payload, key)?;
        }
        Command::Exists { key } => {
            validate_key(key)?;
            payload.push(OP_EXISTS);
            put_bytes(&mut payload, key)?;
        }
        Command::Stats => payload.push(OP_STATS),
    }
    write_frame(writer, &payload)
}

pub fn read_command<R: Read>(reader: &mut R) -> Result<Option<Command>, ProtocolError> {
    let Some(payload) = read_frame(reader)? else {
        return Ok(None);
    };
    let mut frame = FrameReader::new(&payload);
    read_version(&mut frame)?;
    let opcode = frame.u8()?;
    let command = match opcode {
        OP_SET => Command::Set {
            key: frame.key()?,
            value: frame.value()?,
            expires_in: None,
        },
        OP_SET_EX => {
            let key = frame.key()?;
            let value = frame.value()?;
            let seconds = frame.u64()?;
            if seconds == 0 {
                return Err(ProtocolError::Malformed("TTL must be greater than zero"));
            }
            Command::Set {
                key,
                value,
                expires_in: Some(Duration::from_secs(seconds)),
            }
        }
        OP_GET => Command::Get { key: frame.key()? },
        OP_DELETE => Command::Delete { key: frame.key()? },
        OP_EXISTS => Command::Exists { key: frame.key()? },
        OP_STATS => Command::Stats,
        tag => return Err(ProtocolError::UnknownTag(tag)),
    };
    frame.finish()?;
    Ok(Some(command))
}

pub fn write_response<W: Write>(writer: &mut W, response: &Response) -> Result<(), ProtocolError> {
    let mut payload = vec![PROTOCOL_VERSION];
    match response {
        Response::Ok => payload.push(RESPONSE_OK),
        Response::Value(value) => {
            payload.push(RESPONSE_VALUE);
            match value {
                Some(value) => {
                    validate_value(value)?;
                    payload.push(1);
                    put_bytes(&mut payload, value)?;
                }
                None => payload.push(0),
            }
        }
        Response::Boolean(value) => {
            payload.push(RESPONSE_BOOLEAN);
            payload.push(u8::from(*value));
        }
        Response::Stats(stats) => {
            payload.push(RESPONSE_STATS);
            for value in stats_values(stats) {
                payload.extend_from_slice(&value.to_be_bytes());
            }
        }
        Response::Error { code, message } => {
            payload.push(RESPONSE_ERROR);
            payload.extend_from_slice(&code.to_be_bytes());
            put_bytes(&mut payload, message.as_bytes())?;
        }
    }
    write_frame(writer, &payload)
}

pub fn read_response<R: Read>(reader: &mut R) -> Result<Option<Response>, ProtocolError> {
    let Some(payload) = read_frame(reader)? else {
        return Ok(None);
    };
    let mut frame = FrameReader::new(&payload);
    read_version(&mut frame)?;
    let tag = frame.u8()?;
    let response = match tag {
        RESPONSE_OK => Response::Ok,
        RESPONSE_VALUE => match frame.u8()? {
            0 => Response::Value(None),
            1 => Response::Value(Some(frame.value()?)),
            _ => return Err(ProtocolError::Malformed("invalid optional value marker")),
        },
        RESPONSE_BOOLEAN => match frame.u8()? {
            0 => Response::Boolean(false),
            1 => Response::Boolean(true),
            _ => return Err(ProtocolError::Malformed("invalid boolean")),
        },
        RESPONSE_STATS => Response::Stats(StatsSnapshot {
            uptime_ms: frame.u64()?,
            active_connections: frame.u64()?,
            total_connections: frame.u64()?,
            total_requests: frame.u64()?,
            total_errors: frame.u64()?,
            get_commands: frame.u64()?,
            set_commands: frame.u64()?,
            delete_commands: frame.u64()?,
            exists_commands: frame.u64()?,
            stats_commands: frame.u64()?,
            key_count: frame.u64()?,
        }),
        RESPONSE_ERROR => {
            let code = frame.u16()?;
            let bytes = frame.bytes(MAX_FRAME_BYTES)?;
            let message = String::from_utf8(bytes)
                .map_err(|_| ProtocolError::Malformed("error message is not UTF-8"))?;
            Response::Error { code, message }
        }
        other => return Err(ProtocolError::UnknownTag(other)),
    };
    frame.finish()?;
    Ok(Some(response))
}

fn stats_values(stats: &StatsSnapshot) -> [u64; 11] {
    [
        stats.uptime_ms,
        stats.active_connections,
        stats.total_connections,
        stats.total_requests,
        stats.total_errors,
        stats.get_commands,
        stats.set_commands,
        stats.delete_commands,
        stats.exists_commands,
        stats.stats_commands,
        stats.key_count,
    ]
}

fn validate_key(key: &[u8]) -> Result<(), ProtocolError> {
    if key.is_empty() {
        return Err(ProtocolError::Malformed("empty key"));
    }
    if key.len() > MAX_KEY_BYTES {
        return Err(ProtocolError::KeyTooLarge { actual: key.len() });
    }
    Ok(())
}

fn validate_value(value: &[u8]) -> Result<(), ProtocolError> {
    if value.len() > MAX_VALUE_BYTES {
        return Err(ProtocolError::ValueTooLarge {
            actual: value.len(),
        });
    }
    Ok(())
}

fn put_bytes(target: &mut Vec<u8>, bytes: &[u8]) -> Result<(), ProtocolError> {
    let length = u32::try_from(bytes.len())
        .map_err(|_| ProtocolError::Malformed("field length exceeds u32"))?;
    target.extend_from_slice(&length.to_be_bytes());
    target.extend_from_slice(bytes);
    Ok(())
}

fn write_frame<W: Write>(writer: &mut W, payload: &[u8]) -> Result<(), ProtocolError> {
    if payload.is_empty() {
        return Err(ProtocolError::EmptyFrame);
    }
    if payload.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge {
            actual: payload.len(),
            max: MAX_FRAME_BYTES,
        });
    }
    let length = u32::try_from(payload.len())
        .map_err(|_| ProtocolError::Malformed("frame length exceeds u32"))?;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(payload)?;
    Ok(())
}

fn read_frame<R: Read>(reader: &mut R) -> Result<Option<Vec<u8>>, ProtocolError> {
    let mut prefix = [0_u8; 4];
    loop {
        match reader.read(&mut prefix[..1]) {
            Ok(0) => return Ok(None),
            Ok(1) => break,
            Ok(_) => {
                return Err(ProtocolError::Malformed(
                    "reader returned an invalid byte count",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Err(ProtocolError::Idle);
            }
            Err(error) => return Err(ProtocolError::Io(error)),
        }
    }
    read_exact(reader, &mut prefix[1..])?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length == 0 {
        return Err(ProtocolError::EmptyFrame);
    }
    if length > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge {
            actual: length,
            max: MAX_FRAME_BYTES,
        });
    }
    let mut payload = vec![0_u8; length];
    read_exact(reader, &mut payload)?;
    Ok(Some(payload))
}

fn read_exact<R: Read>(reader: &mut R, bytes: &mut [u8]) -> Result<(), ProtocolError> {
    match reader.read_exact(bytes) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            Err(ProtocolError::UnexpectedEof)
        }
        Err(error) => Err(ProtocolError::Io(error)),
    }
}

fn read_version(frame: &mut FrameReader<'_>) -> Result<(), ProtocolError> {
    let version = frame.u8()?;
    if version != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion(version));
    }
    Ok(())
}

struct FrameReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> FrameReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], ProtocolError> {
        let end = self
            .position
            .checked_add(count)
            .ok_or(ProtocolError::Malformed("field length overflow"))?;
        let result = self
            .bytes
            .get(self.position..end)
            .ok_or(ProtocolError::UnexpectedEof)?;
        self.position = end;
        Ok(result)
    }

    fn u8(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, ProtocolError> {
        let mut bytes = [0_u8; 2];
        bytes.copy_from_slice(self.take(2)?);
        Ok(u16::from_be_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, ProtocolError> {
        let mut bytes = [0_u8; 4];
        bytes.copy_from_slice(self.take(4)?);
        Ok(u32::from_be_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, ProtocolError> {
        let mut bytes = [0_u8; 8];
        bytes.copy_from_slice(self.take(8)?);
        Ok(u64::from_be_bytes(bytes))
    }

    fn bytes(&mut self, maximum: usize) -> Result<Vec<u8>, ProtocolError> {
        let length = self.u32()? as usize;
        if length > maximum {
            return Err(ProtocolError::Malformed("field exceeds its size limit"));
        }
        Ok(self.take(length)?.to_vec())
    }

    fn key(&mut self) -> Result<Vec<u8>, ProtocolError> {
        let key = self.bytes(MAX_KEY_BYTES)?;
        validate_key(&key)?;
        Ok(key)
    }

    fn value(&mut self) -> Result<Vec<u8>, ProtocolError> {
        let value = self.bytes(MAX_VALUE_BYTES)?;
        validate_value(&value)?;
        Ok(value)
    }

    fn finish(self) -> Result<(), ProtocolError> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(ProtocolError::TrailingBytes)
        }
    }
}
