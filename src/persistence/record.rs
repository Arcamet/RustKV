use crate::persistence::PersistenceError;
use crate::protocol::{MAX_KEY_BYTES, MAX_VALUE_BYTES};

const SET: u8 = 1;
const DELETE: u8 = 2;
const NO_EXPIRY: u64 = u64::MAX;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogRecord {
    Set {
        key: Vec<u8>,
        value: Vec<u8>,
        expires_at_ms: Option<u64>,
    },
    Delete {
        key: Vec<u8>,
    },
}

impl LogRecord {
    pub(crate) fn encode(&self) -> Result<Vec<u8>, PersistenceError> {
        match self {
            Self::Set {
                key,
                value,
                expires_at_ms,
            } => encode_set(key, value, *expires_at_ms),
            Self::Delete { key } => encode_delete(key),
        }
    }

    pub(crate) fn decode(body: &[u8], offset: u64) -> Result<Self, PersistenceError> {
        if body.len() < 17 {
            return Err(corrupt(
                offset,
                "record body is shorter than its fixed header",
            ));
        }
        let operation = body[0];
        let expires = read_u64(&body[1..9]);
        let key_length = read_u32(&body[9..13]) as usize;
        let value_length = read_u32(&body[13..17]) as usize;
        if key_length == 0 || key_length > MAX_KEY_BYTES {
            return Err(corrupt(offset, "record key length is invalid"));
        }
        if value_length > MAX_VALUE_BYTES {
            return Err(corrupt(offset, "record value length is invalid"));
        }
        let data_length = key_length
            .checked_add(value_length)
            .and_then(|length| length.checked_add(17))
            .ok_or_else(|| corrupt(offset, "record field lengths overflow"))?;
        if data_length != body.len() {
            return Err(corrupt(
                offset,
                "record field lengths do not match its body",
            ));
        }
        let key_end = 17 + key_length;
        let key = body[17..key_end].to_vec();
        let value = body[key_end..].to_vec();
        match operation {
            SET => Ok(Self::Set {
                key,
                value,
                expires_at_ms: (expires != NO_EXPIRY).then_some(expires),
            }),
            DELETE if value_length == 0 && expires == NO_EXPIRY => Ok(Self::Delete { key }),
            DELETE => Err(corrupt(offset, "DELETE record contains unexpected fields")),
            _ => Err(corrupt(offset, "record operation is unknown")),
        }
    }
}

pub(crate) fn encode_set(
    key: &[u8],
    value: &[u8],
    expires_at_ms: Option<u64>,
) -> Result<Vec<u8>, PersistenceError> {
    validate_key(key)?;
    validate_value(value)?;
    let mut body = Vec::with_capacity(17 + key.len() + value.len());
    body.push(SET);
    body.extend_from_slice(&expires_at_ms.unwrap_or(NO_EXPIRY).to_be_bytes());
    put_lengths(&mut body, key, value)?;
    body.extend_from_slice(key);
    body.extend_from_slice(value);
    Ok(body)
}

pub(crate) fn encode_delete(key: &[u8]) -> Result<Vec<u8>, PersistenceError> {
    validate_key(key)?;
    let mut body = Vec::with_capacity(17 + key.len());
    body.push(DELETE);
    body.extend_from_slice(&NO_EXPIRY.to_be_bytes());
    put_lengths(&mut body, key, &[])?;
    body.extend_from_slice(key);
    Ok(body)
}

fn validate_key(key: &[u8]) -> Result<(), PersistenceError> {
    if key.is_empty() {
        return Err(PersistenceError::InvalidRecord("key must not be empty"));
    }
    if key.len() > MAX_KEY_BYTES {
        return Err(PersistenceError::InvalidRecord(
            "key exceeds the configured maximum",
        ));
    }
    Ok(())
}

fn validate_value(value: &[u8]) -> Result<(), PersistenceError> {
    if value.len() > MAX_VALUE_BYTES {
        return Err(PersistenceError::InvalidRecord(
            "value exceeds the configured maximum",
        ));
    }
    Ok(())
}

fn put_lengths(body: &mut Vec<u8>, key: &[u8], value: &[u8]) -> Result<(), PersistenceError> {
    let key_length = u32::try_from(key.len())
        .map_err(|_| PersistenceError::InvalidRecord("key is too large"))?;
    let value_length = u32::try_from(value.len())
        .map_err(|_| PersistenceError::InvalidRecord("value is too large"))?;
    body.extend_from_slice(&key_length.to_be_bytes());
    body.extend_from_slice(&value_length.to_be_bytes());
    Ok(())
}

fn read_u32(bytes: &[u8]) -> u32 {
    let mut fixed = [0_u8; 4];
    fixed.copy_from_slice(bytes);
    u32::from_be_bytes(fixed)
}

fn read_u64(bytes: &[u8]) -> u64 {
    let mut fixed = [0_u8; 8];
    fixed.copy_from_slice(bytes);
    u64::from_be_bytes(fixed)
}

fn corrupt(offset: u64, message: &str) -> PersistenceError {
    PersistenceError::Corrupt {
        offset,
        message: message.to_owned(),
    }
}
