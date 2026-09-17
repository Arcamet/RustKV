# RustKV Design Specification

## Purpose

RustKV is a compact persistent network key-value server for demonstrating Rust systems programming. It is intentionally smaller than Redis: one process, one node, no authentication, replication, clustering, transactions, or SQL.

The first release is successful when it provides a binary-safe store, a documented framed protocol, concurrent TCP clients, append-only persistence with recovery, expiration, compaction, metrics, benchmarks, tests, CI, and documentation whose claims are limited to verified behavior.

## Architecture

RustKV is one Cargo package. Its library contains all behavior; `rustkvd`, `rustkv-cli`, and `rustkv-bench` are thin binaries.

```text
client -> TcpStream -> protocol codec -> command executor -> Arc<Database>
                                                        |-> RwLock<Store>
                                                        |-> Mutex<AppendLog>
                                                        `-> atomic Metrics
```

The server uses `std::net` and one OS thread per accepted connection. A configurable connection ceiling prevents unbounded thread creation. Tokio and a custom thread pool are excluded from V1 because they would complicate the ownership and persistence lessons without serving the expected connection scale.

## Store and ownership

Keys and values are `Vec<u8>` so storage and the wire protocol are binary-safe. `Store` owns a `HashMap<Vec<u8>, Entry>`. Mutating calls take owned keys and values; lookup calls borrow keys as `&[u8]`. The pure store may return `&[u8]`, with the lifetime tied to the store borrow.

`Database` owns `RwLock<Store>`. Network responses must own their bytes, so `Database::get` clones only the selected value while holding the read guard. The reference never escapes the guard.

Entries optionally hold an absolute Unix expiration timestamp in milliseconds. Reads treat expired entries as absent without mutating, preserving concurrent read locks. Mutations purge expired entries opportunistically. Snapshots omit expired entries.

## Concurrency

Every connection thread owns an `Arc<Database>`. `Arc` supplies shared ownership and `RwLock` supplies interior mutability. Read commands take a read lock; mutations take a write lock. Metrics use atomics.

Persistent mutations always acquire the store write lock before the append-log mutex. No code may acquire them in reverse order. A mutation appends and synchronizes its record before changing memory. This blocks readers during disk synchronization but makes ordering understandable. Lock poisoning is returned as a structured internal error rather than unwrapped or ignored.

## Protocol

Every frame begins with a four-byte big-endian payload length, followed by a one-byte protocol version and one-byte request opcode or response tag. Operation fields use fixed-width big-endian integers and length-prefixed byte strings.

V1 commands are `SET`, `SET_EX`, `GET`, `DELETE`, `EXISTS`, and `STATS`. Responses are `OK`, optional value, boolean, fixed metrics snapshot, or structured error. The maximum payload is 1 MiB, keys are limited to 4 KiB, and values to 1,000,000 bytes. Checked arithmetic and validation occur before declared allocations. Key/value data is never interpreted as UTF-8.

Clean EOF between frames ends a connection. EOF inside a prefix or payload, invalid lengths, unknown versions/opcodes, trailing bytes, and malformed fields produce deterministic errors and close the connection.

## Persistence and recovery

The append-only file starts with an eight-byte magic/version header. Each record contains a length, operation, absolute expiry, key length, value length, bytes, and CRC32. Successful `SET`, `SET_EX`, and effective `DELETE` operations are appended with `write_all` and acknowledged only after `sync_data` succeeds.

Startup replays valid records in order. A truncated final record is discarded by truncating the file to the last valid boundary. Oversized records, invalid complete records, bad checksums, impossible field lengths, and invalid headers fail startup with an offset. They are never silently applied.

A crash after a synchronized append but before the client receives its response creates an unknown outcome: recovery may contain a write whose success the client did not observe. RustKV does not claim transactions or ACID semantics.

## Compaction

Compaction takes the database write lock, snapshots only live entries, and writes a fully synchronized replacement log. For cross-platform behavior, it closes the active file and uses recoverable sibling files (`.compact` and `.backup`) around atomic rename steps. Startup resolves interrupted states: the original is preferred when present; otherwise a complete replacement is promoted, with the backup as the final fallback.

## Error handling and observability

Each layer has a typed error enum. Library code avoids runtime `unwrap` and `expect`. Binaries render error chains and exit nonzero.

Structured logs cover startup, accepted/disconnected connections, recovery, compaction, and recoverable failures. Atomic metrics track uptime, active/total connections, requests, errors, and command counts. `STATS` adds the current live key count.

## Testing

Unit tests cover store semantics, expiry, protocol encoding/decoding, size checks, and persistence records. Integration tests use real loopback sockets on port zero, a readiness signal, real files in temporary directories, barriers for concurrency, restart recovery, malformed frames, abrupt disconnects, and corrupt/truncated logs.

Tests must avoid timing assertions where synchronization can express the condition. TTL store tests use explicit timestamps. CI runs formatting, Clippy with warnings denied, all tests, and a release build on Windows and Linux.

## Benchmarking

Criterion benchmarks measure direct GET/SET and codec throughput. A release-mode network benchmark measures sequential and concurrent clients against a running server. Results are labeled with hardware, OS, Rust version, persistence mode, payload size, client count, and date; no universal performance claim is made.

## Dependencies

Runtime dependencies are limited to `crc32fast`, `tracing`, and `tracing-subscriber`. Development dependencies are `criterion` and `tempfile`. Protocol serialization, CLI parsing, networking, synchronization, and persistence framing use the standard library.

## Explicit limitations

- Single process and single node
- No authentication or encryption; loopback bind by default
- Writers and readers pause during synchronous persistent mutations
- One OS thread per active connection
- Expired entries may occupy memory until a later mutation or compaction
- No replication, transactions, online backup, or compatibility guarantee before protocol V1 is frozen

