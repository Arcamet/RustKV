use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crc32fast::hash;

use super::{LogRecord, PersistenceError};
use crate::protocol::MAX_FRAME_BYTES;

const HEADER: &[u8; 8] = b"RUSTKV01";
const MAX_RECORD_BYTES: usize = MAX_FRAME_BYTES + 32;

#[derive(Debug)]
pub struct AppendLog {
    file: File,
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
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)?;
        if file.metadata()?.len() == 0 {
            file.write_all(HEADER)?;
            file.sync_data()?;
        }
        let records = recover(&mut file)?;
        file.seek(SeekFrom::End(0))?;
        Ok((Self { file, path }, records))
    }

    pub fn append(&mut self, record: &LogRecord) -> Result<(), PersistenceError> {
        let body = record.encode()?;
        if body.len() > MAX_RECORD_BYTES {
            return Err(PersistenceError::RecordTooLarge {
                actual: body.len(),
                max: MAX_RECORD_BYTES,
            });
        }
        let length = u32::try_from(body.len())
            .map_err(|_| PersistenceError::InvalidRecord("record length exceeds u32"))?;
        self.file.write_all(&length.to_be_bytes())?;
        self.file.write_all(&body)?;
        self.file.write_all(&hash(&body).to_be_bytes())?;
        self.file.sync_data()?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
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
