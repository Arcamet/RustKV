use std::error::Error;
use std::fmt;
use std::path::Path;
use std::sync::{Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::persistence::{AppendLog, LogRecord, PersistenceError};
use crate::store::{Store, StoreError, StoreLimits};

#[derive(Debug)]
pub enum DatabaseError {
    Store(StoreError),
    Persistence(PersistenceError),
    LockPoisoned(&'static str),
    InvalidTtl,
    ClockBeforeUnixEpoch,
    ExpirationOverflow,
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(error) => write!(f, "store error: {error}"),
            Self::Persistence(error) => write!(f, "persistence error: {error}"),
            Self::LockPoisoned(lock) => write!(f, "{lock} lock is poisoned"),
            Self::InvalidTtl => f.write_str("TTL must be greater than zero"),
            Self::ClockBeforeUnixEpoch => f.write_str("system clock is before the Unix epoch"),
            Self::ExpirationOverflow => f.write_str("expiration timestamp overflowed u64"),
        }
    }
}

impl Error for DatabaseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Store(error) => Some(error),
            Self::Persistence(error) => Some(error),
            _ => None,
        }
    }
}

impl From<StoreError> for DatabaseError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}

impl From<PersistenceError> for DatabaseError {
    fn from(value: PersistenceError) -> Self {
        Self::Persistence(value)
    }
}

#[derive(Debug)]
pub struct Database {
    store: RwLock<Store>,
    log: Option<Mutex<AppendLog>>,
}

impl Database {
    pub fn in_memory(limits: StoreLimits) -> Self {
        Self {
            store: RwLock::new(Store::new(limits)),
            log: None,
        }
    }

    pub fn open(path: impl AsRef<Path>, limits: StoreLimits) -> Result<Self, DatabaseError> {
        let (log, records) = AppendLog::open(path)?;
        let mut store = Store::new(limits);
        let now_ms = now_ms()?;
        for record in records {
            match record {
                LogRecord::Set {
                    key,
                    value,
                    expires_at_ms,
                } => store.set(key, value, expires_at_ms)?,
                LogRecord::Delete { key } => {
                    store.delete(&key, now_ms)?;
                }
            }
        }
        store.purge_expired(now_ms);
        Ok(Self {
            store: RwLock::new(store),
            log: Some(Mutex::new(log)),
        })
    }

    pub fn set(
        &self,
        key: Vec<u8>,
        value: Vec<u8>,
        expires_in: Option<Duration>,
    ) -> Result<(), DatabaseError> {
        let now_ms = now_ms()?;
        let expires_at_ms = match expires_in {
            Some(duration) if duration.is_zero() => return Err(DatabaseError::InvalidTtl),
            Some(duration) => {
                let duration_ms = u64::try_from(duration.as_millis())
                    .map_err(|_| DatabaseError::ExpirationOverflow)?;
                Some(
                    now_ms
                        .checked_add(duration_ms)
                        .ok_or(DatabaseError::ExpirationOverflow)?,
                )
            }
            None => None,
        };

        let mut store = self
            .store
            .write()
            .map_err(|_| DatabaseError::LockPoisoned("store"))?;
        store.validate_key(&key)?;
        store.validate_value(&value)?;
        store.purge_expired(now_ms);
        if let Some(log) = &self.log {
            let mut log = log
                .lock()
                .map_err(|_| DatabaseError::LockPoisoned("append log"))?;
            log.append(&LogRecord::Set {
                key: key.clone(),
                value: value.clone(),
                expires_at_ms,
            })?;
        }
        store.set(key, value, expires_at_ms)?;
        Ok(())
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, DatabaseError> {
        let now_ms = now_ms()?;
        let store = self
            .store
            .read()
            .map_err(|_| DatabaseError::LockPoisoned("store"))?;
        Ok(store.get(key, now_ms)?.map(ToOwned::to_owned))
    }

    pub fn exists(&self, key: &[u8]) -> Result<bool, DatabaseError> {
        let now_ms = now_ms()?;
        let store = self
            .store
            .read()
            .map_err(|_| DatabaseError::LockPoisoned("store"))?;
        Ok(store.exists(key, now_ms)?)
    }

    pub fn delete(&self, key: &[u8]) -> Result<bool, DatabaseError> {
        let now_ms = now_ms()?;
        let mut store = self
            .store
            .write()
            .map_err(|_| DatabaseError::LockPoisoned("store"))?;
        store.validate_key(key)?;
        let existed = store.exists(key, now_ms)?;
        if existed && let Some(log) = &self.log {
            let mut log = log
                .lock()
                .map_err(|_| DatabaseError::LockPoisoned("append log"))?;
            log.append(&LogRecord::Delete { key: key.to_vec() })?;
        }
        store.delete(key, now_ms)?;
        Ok(existed)
    }

    pub fn key_count(&self) -> Result<usize, DatabaseError> {
        let now_ms = now_ms()?;
        let store = self
            .store
            .read()
            .map_err(|_| DatabaseError::LockPoisoned("store"))?;
        Ok(store.live_len(now_ms))
    }
}

fn now_ms() -> Result<u64, DatabaseError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| DatabaseError::ClockBeforeUnixEpoch)?;
    u64::try_from(elapsed.as_millis()).map_err(|_| DatabaseError::ExpirationOverflow)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tempfile::tempdir;

    use super::Database;
    use crate::store::StoreLimits;

    #[test]
    fn persistent_database_recovers_set_overwrite_and_delete() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("data.aof");
        {
            let database = Database::open(&path, StoreLimits::default()).unwrap();
            database
                .set(b"name".to_vec(), b"Jose".to_vec(), None)
                .unwrap();
            database
                .set(b"name".to_vec(), b"Carlos".to_vec(), None)
                .unwrap();
            database
                .set(b"gone".to_vec(), b"value".to_vec(), None)
                .unwrap();
            assert!(database.delete(b"gone").unwrap());
        }

        let database = Database::open(&path, StoreLimits::default()).unwrap();
        assert_eq!(database.get(b"name").unwrap(), Some(b"Carlos".to_vec()));
        assert_eq!(database.get(b"gone").unwrap(), None);
        assert_eq!(database.key_count().unwrap(), 1);
    }

    #[test]
    fn in_memory_database_supports_ttl_commands() {
        let database = Database::in_memory(StoreLimits::default());

        database
            .set(
                b"session".to_vec(),
                b"value".to_vec(),
                Some(Duration::from_secs(60)),
            )
            .unwrap();

        assert_eq!(database.get(b"session").unwrap(), Some(b"value".to_vec()));
    }

    #[test]
    fn zero_ttl_is_rejected_without_mutating_state() {
        let database = Database::in_memory(StoreLimits::default());

        assert!(
            database
                .set(b"session".to_vec(), b"value".to_vec(), Some(Duration::ZERO),)
                .is_err()
        );
        assert_eq!(database.get(b"session").unwrap(), None);
    }
}
