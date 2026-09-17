use std::collections::HashMap;
use std::error::Error;
use std::fmt;

pub const DEFAULT_MAX_KEY_BYTES: usize = 4 * 1024;
pub const DEFAULT_MAX_VALUE_BYTES: usize = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreLimits {
    pub max_key_bytes: usize,
    pub max_value_bytes: usize,
}

impl Default for StoreLimits {
    fn default() -> Self {
        Self {
            max_key_bytes: DEFAULT_MAX_KEY_BYTES,
            max_value_bytes: DEFAULT_MAX_VALUE_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    EmptyKey,
    KeyTooLarge { actual: usize, max: usize },
    ValueTooLarge { actual: usize, max: usize },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyKey => f.write_str("key must not be empty"),
            Self::KeyTooLarge { actual, max } => {
                write!(f, "key is {actual} bytes; maximum is {max}")
            }
            Self::ValueTooLarge { actual, max } => {
                write!(f, "value is {actual} bytes; maximum is {max}")
            }
        }
    }
}

impl Error for StoreError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotEntry {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub expires_at_ms: Option<u64>,
}

#[derive(Debug, Clone)]
struct Entry {
    value: Vec<u8>,
    expires_at_ms: Option<u64>,
}

impl Entry {
    fn is_expired(&self, now_ms: u64) -> bool {
        self.expires_at_ms.is_some_and(|expires| expires <= now_ms)
    }
}

#[derive(Debug)]
pub struct Store {
    entries: HashMap<Vec<u8>, Entry>,
    limits: StoreLimits,
}

impl Store {
    pub fn new(limits: StoreLimits) -> Self {
        Self {
            entries: HashMap::new(),
            limits,
        }
    }

    pub fn validate_key(&self, key: &[u8]) -> Result<(), StoreError> {
        if key.is_empty() {
            return Err(StoreError::EmptyKey);
        }
        if key.len() > self.limits.max_key_bytes {
            return Err(StoreError::KeyTooLarge {
                actual: key.len(),
                max: self.limits.max_key_bytes,
            });
        }
        Ok(())
    }

    pub fn validate_value(&self, value: &[u8]) -> Result<(), StoreError> {
        if value.len() > self.limits.max_value_bytes {
            return Err(StoreError::ValueTooLarge {
                actual: value.len(),
                max: self.limits.max_value_bytes,
            });
        }
        Ok(())
    }

    pub fn set(
        &mut self,
        key: Vec<u8>,
        value: Vec<u8>,
        expires_at_ms: Option<u64>,
    ) -> Result<(), StoreError> {
        self.validate_key(&key)?;
        self.validate_value(&value)?;
        self.entries.insert(
            key,
            Entry {
                value,
                expires_at_ms,
            },
        );
        Ok(())
    }

    pub fn get(&self, key: &[u8], now_ms: u64) -> Result<Option<&[u8]>, StoreError> {
        self.validate_key(key)?;
        Ok(self
            .entries
            .get(key)
            .filter(|entry| !entry.is_expired(now_ms))
            .map(|entry| entry.value.as_slice()))
    }

    pub fn exists(&self, key: &[u8], now_ms: u64) -> Result<bool, StoreError> {
        Ok(self.get(key, now_ms)?.is_some())
    }

    pub fn delete(&mut self, key: &[u8], now_ms: u64) -> Result<bool, StoreError> {
        self.validate_key(key)?;
        let was_live = self
            .entries
            .get(key)
            .is_some_and(|entry| !entry.is_expired(now_ms));
        self.entries.remove(key);
        Ok(was_live)
    }

    pub fn live_len(&self, now_ms: u64) -> usize {
        self.entries
            .values()
            .filter(|entry| !entry.is_expired(now_ms))
            .count()
    }

    pub fn purge_expired(&mut self, now_ms: u64) {
        self.entries.retain(|_, entry| !entry.is_expired(now_ms));
    }

    pub fn snapshot(&self, now_ms: u64) -> Vec<SnapshotEntry> {
        self.entries
            .iter()
            .filter(|(_, entry)| !entry.is_expired(now_ms))
            .map(|(key, entry)| SnapshotEntry {
                key: key.clone(),
                value: entry.value.clone(),
                expires_at_ms: entry.expires_at_ms,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
