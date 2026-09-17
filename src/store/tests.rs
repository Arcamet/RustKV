use super::{Store, StoreError, StoreLimits};

const NOW: u64 = 1_000;

fn store() -> Store {
    Store::new(StoreLimits {
        max_key_bytes: 8,
        max_value_bytes: 16,
    })
}

#[test]
fn set_owns_bytes_and_get_borrows_the_value() {
    let mut store = store();
    store.set(b"name".to_vec(), b"Jose".to_vec(), None).unwrap();

    assert_eq!(store.get(b"name", NOW).unwrap(), Some(&b"Jose"[..]));
}

#[test]
fn set_overwrites_an_existing_value() {
    let mut store = store();
    store.set(b"key".to_vec(), b"old".to_vec(), None).unwrap();
    store.set(b"key".to_vec(), b"new".to_vec(), None).unwrap();

    assert_eq!(store.get(b"key", NOW).unwrap(), Some(&b"new"[..]));
    assert_eq!(store.live_len(NOW), 1);
}

#[test]
fn delete_and_exists_report_observable_state() {
    let mut store = store();
    store.set(b"key".to_vec(), b"value".to_vec(), None).unwrap();

    assert!(store.exists(b"key", NOW).unwrap());
    assert!(store.delete(b"key", NOW).unwrap());
    assert!(!store.delete(b"key", NOW).unwrap());
    assert!(!store.exists(b"key", NOW).unwrap());
}

#[test]
fn invalid_key_and_value_sizes_are_rejected() {
    let mut store = store();

    assert_eq!(
        store.set(Vec::new(), b"value".to_vec(), None),
        Err(StoreError::EmptyKey)
    );
    assert_eq!(
        store.set(b"123456789".to_vec(), b"value".to_vec(), None),
        Err(StoreError::KeyTooLarge { actual: 9, max: 8 })
    );
    assert_eq!(
        store.set(b"key".to_vec(), vec![0; 17], None),
        Err(StoreError::ValueTooLarge {
            actual: 17,
            max: 16
        })
    );
}

#[test]
fn expired_entries_are_logically_absent() {
    let mut store = store();
    store
        .set(b"session".to_vec(), b"value".to_vec(), Some(1_100))
        .unwrap();

    assert_eq!(store.get(b"session", 1_099).unwrap(), Some(&b"value"[..]));
    assert_eq!(store.get(b"session", 1_100).unwrap(), None);
    assert!(!store.exists(b"session", 1_100).unwrap());
    assert_eq!(store.live_len(1_100), 0);
}

#[test]
fn snapshot_clones_only_live_entries() {
    let mut store = store();
    store.set(b"live".to_vec(), b"one".to_vec(), None).unwrap();
    store
        .set(b"expired".to_vec(), b"two".to_vec(), Some(999))
        .unwrap();

    let snapshot = store.snapshot(NOW);

    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].key, b"live");
    assert_eq!(snapshot[0].value, b"one");
    assert_eq!(snapshot[0].expires_at_ms, None);
}
