use std::fs::OpenOptions;
use std::io::Write;

use rustkv::database::Database;
use rustkv::store::StoreLimits;
use tempfile::tempdir;

#[test]
fn acknowledged_records_survive_database_recreation() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("data.aof");
    {
        let database = Database::open(&path, StoreLimits::default()).unwrap();
        database
            .set(b"name".to_vec(), b"Jose".to_vec(), None)
            .unwrap();
        database
            .set(b"language".to_vec(), b"Rust".to_vec(), None)
            .unwrap();
        database.delete(b"name").unwrap();
    }

    let recovered = Database::open(&path, StoreLimits::default()).unwrap();
    assert_eq!(recovered.get(b"name").unwrap(), None);
    assert_eq!(recovered.get(b"language").unwrap(), Some(b"Rust".to_vec()));
}

#[test]
fn a_crash_truncated_tail_does_not_hide_the_valid_prefix() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("data.aof");
    {
        let database = Database::open(&path, StoreLimits::default()).unwrap();
        database
            .set(b"safe".to_vec(), b"value".to_vec(), None)
            .unwrap();
    }
    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(&[0, 0, 0, 50, 1, 2, 3]).unwrap();
    drop(file);

    let recovered = Database::open(&path, StoreLimits::default()).unwrap();
    assert_eq!(recovered.get(b"safe").unwrap(), Some(b"value".to_vec()));
}
