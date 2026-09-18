use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crc32fast::hash;

use super::record::{encode_delete, encode_set};
use super::{LogRecord, PersistenceError};
use crate::protocol::MAX_FRAME_BYTES;
use crate::store::SnapshotEntry;

const HEADER: &[u8; 8] = b"RUSTKV01";
const MAX_RECORD_BYTES: usize = MAX_FRAME_BYTES + 32;

#[derive(Debug)]
pub struct AppendLog {
    file: Option<File>,
    path: PathBuf,
}

impl AppendLog {
    pub fn open(path: impl AsRef<Path>) -> Result<(Self, Vec<LogRecord>), PersistenceError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let backup_to_remove = resolve_interrupted_compaction(&path)?;
        let mut file = open_active(&path)?;
        if file.metadata()?.len() == 0 {
            file.write_all(HEADER)?;
            file.sync_data()?;
        }
        let records = recover(&mut file)?;
        file.seek(SeekFrom::End(0))?;
        if let Some(backup) = backup_to_remove {
            remove_if_exists(&backup)?;
        }
        Ok((
            Self {
                file: Some(file),
                path,
            },
            records,
        ))
    }

    pub fn append(&mut self, record: &LogRecord) -> Result<(), PersistenceError> {
        let body = record.encode()?;
        self.append_body(&body)
    }

    pub fn append_set(
        &mut self,
        key: &[u8],
        value: &[u8],
        expires_at_ms: Option<u64>,
    ) -> Result<(), PersistenceError> {
        let body = encode_set(key, value, expires_at_ms)?;
        self.append_body(&body)
    }

    pub fn append_delete(&mut self, key: &[u8]) -> Result<(), PersistenceError> {
        let body = encode_delete(key)?;
        self.append_body(&body)
    }

    pub fn compact(&mut self, entries: &[SnapshotEntry]) -> Result<(), PersistenceError> {
        let replacement = compact_path(&self.path);
        let backup = backup_path(&self.path);
        remove_if_exists(&replacement)?;
        remove_if_exists(&backup)?;

        let mut replacement_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&replacement)?;
        replacement_file.write_all(HEADER)?;
        for entry in entries {
            let body = encode_set(&entry.key, &entry.value, entry.expires_at_ms)?;
            write_record(&mut replacement_file, &body)?;
        }
        replacement_file.sync_data()?;
        drop(replacement_file);

        drop(self.file.take());
        if let Err(error) = fs::rename(&self.path, &backup) {
            self.file = Some(open_active(&self.path)?);
            remove_if_exists(&replacement)?;
            return Err(PersistenceError::Io(error));
        }
        if let Err(error) = fs::rename(&replacement, &self.path) {
            let _ = fs::rename(&backup, &self.path);
            self.file = Some(open_active(&self.path)?);
            return Err(PersistenceError::Io(error));
        }
        self.file = Some(open_active(&self.path)?);
        remove_if_exists(&backup)?;
        sync_parent_directory(&self.path)?;
        Ok(())
    }

    fn append_body(&mut self, body: &[u8]) -> Result<(), PersistenceError> {
        if body.len() > MAX_RECORD_BYTES {
            return Err(PersistenceError::RecordTooLarge {
                actual: body.len(),
                max: MAX_RECORD_BYTES,
            });
        }
        let file = self
            .file
            .as_mut()
            .ok_or(PersistenceError::InvalidRecord("append log is not open"))?;
        write_record(file, body)?;
        file.sync_data()?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn write_record(file: &mut File, body: &[u8]) -> Result<(), PersistenceError> {
    if body.len() > MAX_RECORD_BYTES {
        return Err(PersistenceError::RecordTooLarge {
            actual: body.len(),
            max: MAX_RECORD_BYTES,
        });
    }
    let length = u32::try_from(body.len())
        .map_err(|_| PersistenceError::InvalidRecord("record length exceeds u32"))?;
    file.write_all(&length.to_be_bytes())?;
    file.write_all(body)?;
    file.write_all(&hash(body).to_be_bytes())?;
    Ok(())
}

fn open_active(path: &Path) -> Result<File, PersistenceError> {
    Ok(OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?)
}

fn resolve_interrupted_compaction(path: &Path) -> Result<Option<PathBuf>, PersistenceError> {
    let replacement = compact_path(path);
    let backup = backup_path(path);
    if path.exists() {
        remove_if_exists(&replacement)?;
        remove_if_exists(&backup)?;
        return Ok(None);
    }
    if replacement.exists() {
        fs::rename(&replacement, path)?;
        return Ok(backup.exists().then_some(backup));
    }
    if backup.exists() {
        fs::rename(backup, path)?;
    }
    Ok(None)
}

fn remove_if_exists(path: &Path) -> Result<(), PersistenceError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(PersistenceError::Io(error)),
    }
}

pub(crate) fn compact_path(path: &Path) -> PathBuf {
    sidecar_path(path, ".compact")
}

pub(crate) fn backup_path(path: &Path) -> PathBuf {
    sidecar_path(path, ".backup")
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> Result<(), PersistenceError> {
    if let Some(parent) = path.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> Result<(), PersistenceError> {
    Ok(())
}

fn recover(file: &mut File) -> Result<Vec<LogRecord>, PersistenceError> {
    file.seek(SeekFrom::Start(0))?;
    let mut header = [0_u8; HEADER.len()];
    if let Err(error) = file.read_exact(&mut header) {
        return if error.kind() == io::ErrorKind::UnexpectedEof {
            Err(PersistenceError::InvalidHeader)
        } else {
            Err(PersistenceError::Io(error))
        };
    }
    if &header != HEADER {
        return Err(PersistenceError::InvalidHeader);
    }

    let mut records = Vec::new();
    let mut valid_end = HEADER.len() as u64;
    loop {
        let record_offset = file.stream_position()?;
        let mut prefix = [0_u8; 4];
        match file.read(&mut prefix[..1]) {
            Ok(0) => break,
            Ok(1) => {}
            Ok(_) => {
                return Err(PersistenceError::Corrupt {
                    offset: record_offset,
                    message: "reader returned an invalid byte count".to_owned(),
                });
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(PersistenceError::Io(error)),
        }
        if !read_tail(file, &mut prefix[1..])? {
            truncate_tail(file, valid_end)?;
            break;
        }
        let length = u32::from_be_bytes(prefix) as usize;
        if !(17..=MAX_RECORD_BYTES).contains(&length) {
            return Err(PersistenceError::Corrupt {
                offset: record_offset,
                message: format!("record length {length} is invalid"),
            });
        }
        let mut body = vec![0_u8; length];
        if !read_tail(file, &mut body)? {
            truncate_tail(file, valid_end)?;
            break;
        }
        let mut checksum = [0_u8; 4];
        if !read_tail(file, &mut checksum)? {
            truncate_tail(file, valid_end)?;
            break;
        }
        let expected = u32::from_be_bytes(checksum);
        let actual = hash(&body);
        if expected != actual {
            return Err(PersistenceError::Corrupt {
                offset: record_offset,
                message: "record checksum does not match".to_owned(),
            });
        }
        records.push(LogRecord::decode(&body, record_offset)?);
        valid_end = file.stream_position()?;
    }
    Ok(records)
}

fn read_tail(file: &mut File, bytes: &mut [u8]) -> Result<bool, PersistenceError> {
    match file.read_exact(bytes) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => Err(PersistenceError::Io(error)),
    }
}

fn truncate_tail(file: &mut File, valid_end: u64) -> Result<(), PersistenceError> {
    file.set_len(valid_end)?;
    file.sync_data()?;
    file.seek(SeekFrom::Start(valid_end))?;
    Ok(())
}
