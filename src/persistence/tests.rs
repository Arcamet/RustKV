use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};

use tempfile::tempdir;

use super::{AppendLog, LogRecord, PersistenceError};
use crate::persistence::log::{backup_path, compact_path};

#[test]
fn appended_records_replay_in_order_after_reopen() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("data.aof");

    let (mut log, records) = AppendLog::open(&path).unwrap();
    assert!(records.is_empty());
    log.append(&LogRecord::Set {
        key: b"key".to_vec(),
        value: b"one".to_vec(),
        expires_at_ms: None,
    })
    .unwrap();
    log.append(&LogRecord::Set {
        key: b"key".to_vec(),
        value: b"two".to_vec(),
        expires_at_ms: Some(9_000),
    })
    .unwrap();
    log.append(&LogRecord::Delete {
        key: b"gone".to_vec(),
    })
    .unwrap();
    drop(log);

    let (_, recovered) = AppendLog::open(&path).unwrap();
    assert_eq!(
        recovered,
        vec![
            LogRecord::Set {
                key: b"key".to_vec(),
                value: b"one".to_vec(),
                expires_at_ms: None,
            },
            LogRecord::Set {
                key: b"key".to_vec(),
                value: b"two".to_vec(),
                expires_at_ms: Some(9_000),
            },
            LogRecord::Delete {
                key: b"gone".to_vec(),
            },
        ]
    );
}

#[test]
fn truncated_final_record_is_removed_at_the_last_valid_boundary() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("data.aof");
    let (mut log, _) = AppendLog::open(&path).unwrap();
    log.append(&LogRecord::Set {
        key: b"safe".to_vec(),
        value: b"value".to_vec(),
        expires_at_ms: None,
    })
    .unwrap();
    drop(log);
    let valid_length = fs::metadata(&path).unwrap().len();

    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(&[0, 0, 0, 20, 1, 2, 3]).unwrap();
    drop(file);

    let (_, recovered) = AppendLog::open(&path).unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(fs::metadata(&path).unwrap().len(), valid_length);
}

#[test]
fn checksum_corruption_in_a_complete_record_fails_closed() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("data.aof");
    let (mut log, _) = AppendLog::open(&path).unwrap();
    log.append(&LogRecord::Set {
        key: b"key".to_vec(),
        value: b"value".to_vec(),
        expires_at_ms: None,
    })
    .unwrap();
    drop(log);

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    file.seek(SeekFrom::End(-1)).unwrap();
    let mut byte = [0_u8; 1];
    file.read_exact(&mut byte).unwrap();
    file.seek(SeekFrom::End(-1)).unwrap();
    file.write_all(&[byte[0] ^ 0xff]).unwrap();
    drop(file);

    let error = AppendLog::open(&path).unwrap_err();
    assert!(matches!(error, PersistenceError::Corrupt { .. }));
}

#[test]
fn invalid_file_header_is_rejected() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("data.aof");
    fs::write(&path, b"NOTRUST!").unwrap();

    let error = AppendLog::open(&path).unwrap_err();
    assert!(matches!(error, PersistenceError::InvalidHeader));
}

#[test]
fn interrupted_compaction_promotes_complete_replacement() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("data.aof");
    let replacement = compact_path(&path);
    let backup = backup_path(&path);
    let (mut live, _) = AppendLog::open(&path).unwrap();
    live.append(&LogRecord::Set {
        key: b"key".to_vec(),
        value: b"old".to_vec(),
        expires_at_ms: None,
    })
    .unwrap();
    drop(live);
    let (mut compact, _) = AppendLog::open(&replacement).unwrap();
    compact
        .append(&LogRecord::Set {
            key: b"key".to_vec(),
            value: b"new".to_vec(),
            expires_at_ms: None,
        })
        .unwrap();
    drop(compact);
    fs::rename(&path, &backup).unwrap();

    let (_, records) = AppendLog::open(&path).unwrap();
    assert!(matches!(
        records.as_slice(),
        [LogRecord::Set { value, .. }] if value == b"new"
    ));
    assert!(!replacement.exists());
    assert!(!backup.exists());
}

#[test]
fn interrupted_compaction_restores_backup_when_no_replacement_exists() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("data.aof");
    let backup = backup_path(&path);
    let (mut live, _) = AppendLog::open(&path).unwrap();
    live.append(&LogRecord::Set {
        key: b"safe".to_vec(),
        value: b"value".to_vec(),
        expires_at_ms: None,
    })
    .unwrap();
    drop(live);
    fs::rename(&path, &backup).unwrap();

    let (_, records) = AppendLog::open(&path).unwrap();
    assert_eq!(records.len(), 1);
    assert!(!backup.exists());
}

#[test]
fn existing_live_log_wins_over_a_stale_compaction_file() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("data.aof");
    let replacement = compact_path(&path);
    let (mut live, _) = AppendLog::open(&path).unwrap();
    live.append(&LogRecord::Set {
        key: b"key".to_vec(),
        value: b"live".to_vec(),
        expires_at_ms: None,
    })
    .unwrap();
    drop(live);
    let (mut stale, _) = AppendLog::open(&replacement).unwrap();
    stale
        .append(&LogRecord::Set {
            key: b"key".to_vec(),
            value: b"stale".to_vec(),
            expires_at_ms: None,
        })
        .unwrap();
    drop(stale);

    let (_, records) = AppendLog::open(&path).unwrap();
    assert!(matches!(
        records.as_slice(),
        [LogRecord::Set { value, .. }] if value == b"live"
    ));
    assert!(!replacement.exists());
}
