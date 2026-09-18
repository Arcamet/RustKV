# Architecture

## Boundaries

RustKV is one Cargo package with a library and three binaries. The library owns behavior; the binaries translate arguments into library calls.

| Unit | Responsibility | Depends on |
|---|---|---|
| `store` | Owned byte keys/values, limits, logical expiry, snapshots | Standard library |
| `database` | Locking, time, persistence ordering, recovery application | Store, persistence |
| `protocol` | Bounded request/response serialization and framing | `Read`/`Write` |
| `server` | Admission, threads, connection lifecycle, execution | Database, protocol, metrics |
| `client` | One persistent TCP connection and request/response exchange | Protocol |
| `persistence` | AOF records, checksum, replay, truncation, compaction | File system, CRC32 |
| `metrics` | Lock-free counters and snapshots | Atomics, monotonic time |
| `config` | Manual, dependency-free argument validation | Protocol command types |

## Data flow

1. The listener accepts a socket and checks the active-connection ceiling.
2. An admitted socket becomes blocking and receives a 100 ms idle control timeout.
3. Its named worker thread reads exactly one bounded frame.
4. The codec validates lengths, version, opcode, and trailing bytes.
5. The executor records the command and calls `Database`.
6. The database obtains the minimum required lock and performs the operation.
7. Persistent mutations synchronize their AOF record before memory changes.
8. The worker serializes one deterministic response and waits for the next frame.

## Ownership

`Store` owns its `HashMap<Vec<u8>, Entry>`. `SET` transfers key/value ownership into the map. Lookup borrows `&[u8]` and the pure store returns `Option<&[u8]>`; that lifetime is tied to the store borrow.

Threads cannot borrow a stack-owned database for their full lifetime, so the server owns `Arc<Database>` and clones the `Arc` into each `'static` worker closure. `Arc` only solves shared ownership. `RwLock<Store>` supplies interior mutability.

The database cannot return a value reference after releasing an `RwLockReadGuard`. It clones only the selected value into the owned response while the guard exists. Persistence encodes mutations from borrowed slices, avoiding a second key/value clone before the originals move into the store.

## Error boundaries

- `StoreError`: invalid key/value sizes
- `ProtocolError`: framing, I/O, malformed fields, limits, unsupported tags
- `PersistenceError`: I/O, invalid headers/records, corruption with offset
- `DatabaseError`: wraps storage/persistence plus clock and poisoned-lock failures
- `ServerError` and `ClientError`: process-boundary networking and lifecycle errors

No library operation uses runtime `unwrap` or `expect`. Binaries print top-level failures and exit nonzero.

## Extensibility

The executor sees commands, not wire bytes. The database sees byte operations, not sockets. Persistence sees records/snapshots, not commands. Those boundaries allow a future protocol, persistence policy, or concurrency model to change without rewriting every layer.

