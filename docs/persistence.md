# Persistence and Recovery

## Append-only format

The file begins with the eight bytes `RUSTKV01`. Records follow:

```text
body length: u32
operation: u8                 # 1 SET, 2 DELETE
expiry: u64                   # Unix milliseconds, u64::MAX means none
key length: u32
value length: u32
key bytes
value bytes
CRC32(body): u32
```

DELETE must have no value and no expiry. All lengths are validated against protocol limits before allocation or application.

## Mutation ordering

Persistent `SET` and effective `DELETE` operations:

1. acquire the store write lock;
2. validate the mutation;
3. acquire the append-log mutex;
4. encode from borrowed key/value slices;
5. `write_all` the record and checksum;
6. call `File::sync_data`;
7. update the in-memory store;
8. release locks and acknowledge.

An append or sync failure leaves memory unchanged. The store write lock is held across disk synchronization so no reader observes a mutation whose durable record failed, and concurrent writers have one order in the log and map.

This is intentionally conservative and slow. `sync_data` durability still depends on OS, filesystem, controller, and device behavior. RustKV does not claim ACID.

## Recovery

Recovery validates the file header and replays records in order. CRC mismatch, invalid operation, impossible length, or malformed complete record fails startup with the byte offset.

An incomplete final prefix/body/checksum is a normal crash shape. Recovery applies the valid prefix, truncates the file to its last valid boundary, synchronizes that truncation, and resumes appending. Incomplete bytes are never applied.

TTL records store absolute expiration time. Recovery applies record order and then removes entries already expired at startup, so an older value cannot reappear behind an expired overwrite.

## Crash outcomes

- Crash before record sync: the operation may be absent after recovery.
- Crash after record sync but before memory update or response: recovery applies it, although the client may not have observed success.
- Crash after response: the synchronized record is designed to be replayable.

Clients that lose a response cannot infer whether the mutation committed. RustKV has no transaction ID or idempotency token.

## Compaction

Append-only history grows indefinitely. `Database::compact` takes the store write lock, drops expired/deleted history, and writes one SET record per live entry to `data.aof.compact`. It synchronizes that file before replacing the live log.

For Windows-compatible replacement, RustKV performs recoverable atomic rename steps:

1. live log -> `.backup`
2. synchronized `.compact` -> live log
3. reopen live log
4. remove `.backup`

Startup resolves interrupted states:

- live exists: use it and remove stale sidecars;
- live missing, `.compact` exists: promote it, validate it, then remove backup;
- only `.backup` exists: restore it.

Unix additionally synchronizes the parent directory after replacement. Stable Rust does not provide the same directory-sync operation on Windows, so the sidecar recovery state machine is the documented safeguard there.

