# RustKV Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and verify the complete RustKV single-node persistent TCP key-value service.

**Architecture:** A synchronous standard-library TCP server dispatches each admitted connection to an OS thread. Threads share `Arc<Database>`; the database coordinates a `RwLock<Store>` and optional `Mutex<AppendLog>`, while a binary codec provides deterministic bounded framing.

**Tech Stack:** Rust 1.98 stable, Cargo, standard library networking/threading/synchronization, crc32fast, tracing, tracing-subscriber, tempfile, Criterion, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-09-17-rustkv-design.md`

## Global Constraints

- One Cargo package; behavior lives in the library and binaries remain thin.
- No Tokio, Serde, clustering, replication, authentication, SQL, containers, or frontend.
- Maximum protocol payload is 1 MiB; maximum key is 4 KiB; maximum value is 1,000,000 bytes.
- Keys and values are arbitrary bytes.
- No runtime `unwrap` or `expect` in library code.
- Production behavior is developed test-first and every milestone finishes with formatting, Clippy, tests, and documentation.

---

### Task 1: Package bootstrap and in-memory store

**Files:** Create `Cargo.toml`, `src/lib.rs`, `src/error.rs`, `src/store/mod.rs`; test in `src/store/tests.rs`.

**Interfaces:** Produce `StoreLimits`, `Store`, `StoreError`, `SnapshotEntry`, `set`, `get`, `delete`, `exists`, `live_len`, and `snapshot` using explicit `now_ms` inputs.

- [ ] Write tests proving owned SET/overwrite, borrowed GET, DELETE/EXISTS, empty and oversized key rejection, value limits, logical expiry, and snapshot omission of expired entries.
- [ ] Run `cargo test store` and verify failure because the store API does not exist.
- [ ] Implement the smallest `HashMap<Vec<u8>, Entry>` API satisfying those cases.
- [ ] Run the focused tests and then `cargo test`.
- [ ] Refactor only while green.

### Task 2: Framed protocol

**Files:** Create `src/protocol/{mod.rs,command.rs,response.rs,codec.rs,tests.rs}`.

**Interfaces:** Produce `Command`, `Response`, `StatsSnapshot`, `ProtocolError`, `read_command`, `write_command`, `read_response`, and `write_response` over `Read`/`Write`.

- [ ] Write literal-byte tests for every command/response, clean EOF, partial prefixes/payloads, unknown tags, trailing bytes, invalid booleans, and outer/inner size boundaries.
- [ ] Verify the tests fail for missing codec behavior.
- [ ] Implement the four-byte big-endian frame and checked field parser without Serde.
- [ ] Run protocol tests and the full suite; refactor parsing helpers while green.

### Task 3: Checksummed append-only persistence and database

**Files:** Create `src/persistence/{mod.rs,record.rs,log.rs,tests.rs}` and `src/database.rs`.

**Interfaces:** Produce `AppendLog::open`, `append_set`, `append_delete`, `recover`, `compact`; `Database::in_memory`, `open`, `set`, `get`, `delete`, `exists`, `key_count`, `compact`.

- [ ] Write restart, overwrite/delete replay, expiry replay, partial-tail truncation, checksum corruption, invalid-header, and persistence-failure tests using real temporary files.
- [ ] Verify focused failures before creating implementations.
- [ ] Implement file header, length-bounded record body, CRC32, replay, and tail truncation.
- [ ] Add database tests proving append-before-memory behavior and structured lock/I/O errors.
- [ ] Run persistence/database tests and the full suite.

### Task 4: Metrics and command executor

**Files:** Create `src/metrics.rs`, `src/server/executor.rs`, `src/server/mod.rs`.

**Interfaces:** Produce `Metrics`, `MetricsSnapshot`, `CommandKind`, and `execute(&Database, &Metrics, Command) -> Response`.

- [ ] Write tests for every command mapping, TTL seconds conversion, error response mapping, key counts, and command counters.
- [ ] Verify failures, implement the minimal executor, and run focused/full tests.

### Task 5: TCP server and client library

**Files:** Create `src/server/connection.rs`, `src/server/runtime.rs`, `src/client/mod.rs`; integration tests in `tests/tcp_integration.rs` and `tests/failure_cases.rs`.

**Interfaces:** Produce `ServerConfig`, `Server::bind`, `local_addr`, `run`, `Client::connect`, and `Client::execute`.

- [ ] Write a failing real-socket test that starts on port zero, performs SET/GET, disconnects, signals shutdown, and joins the server.
- [ ] Add failing malformed-frame, abrupt-disconnect, and oversized-frame tests.
- [ ] Implement accept loop, connection cap, per-connection loops, structured errors, and client request/response exchange.
- [ ] Run all integration tests and the full suite.

### Task 6: Concurrency and TTL

**Files:** Add `tests/concurrency.rs`; expand store/database/TCP tests.

**Interfaces:** Preserve existing APIs; `SET_EX` carries a nonzero duration and persistence stores the absolute expiry.

- [ ] Write failing barrier-based tests for simultaneous unique-key writers, readers during writes, connection accounting, and expiry visibility.
- [ ] Implement only synchronization/expiry changes required by the tests.
- [ ] Run focused tests repeatedly and then the full suite.

### Task 7: Crash-conscious compaction

**Files:** Expand `src/persistence/log.rs`, `src/database.rs`, persistence integration tests.

**Interfaces:** `Database::compact()` rewrites live state and startup resolves `.compact`/`.backup` remnants.

- [ ] Write failing tests for compacted restart, removed/expired key omission, and each interrupted rename state.
- [ ] Implement synchronized replacement and recovery state resolution.
- [ ] Run focused and full tests.

### Task 8: Binaries, configuration, and structured logging

**Files:** Create `src/config.rs`, `src/bin/{rustkvd.rs,rustkv-cli.rs,rustkv-bench.rs}`; configuration tests in `src/config.rs`.

**Interfaces:** Server supports `--bind`, `--data`, `--max-connections`, and `--memory`; client supports all commands; benchmark supports address, operations, clients, key/value sizes.

- [ ] Write failing argument-parser tests for defaults, valid values, missing values, invalid addresses/numbers, and command arity.
- [ ] Implement manual parsers and thin binaries; initialize tracing in `rustkvd`.
- [ ] Run parser tests, `cargo run --bin rustkvd -- --help`, client help, and the full suite.

### Task 9: Benchmarks, CI, and documentation

**Files:** Create `benches/{store.rs,protocol.rs}`, `.github/workflows/ci.yml`, `README.md`, and all required `docs/*.md` files.

**Interfaces:** Criterion bench target `rustkv`; network benchmark emits environment and workload fields with measured operations/second.

- [ ] Add runnable store/codec benchmarks and confirm `cargo bench --no-run` builds them.
- [ ] Add Windows/Linux CI for fmt, Clippy, tests, and release build.
- [ ] Document architecture, protocol bytes, persistence/recovery, lock reasoning, design decisions, reproducible benchmarks, limitations, interview answers, and evidence categorized as implemented/tested/planned.

### Task 10: Final verification and review

**Files:** All project files.

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo clippy --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --all-targets`.
- [ ] Run `cargo build --release` and `cargo bench --no-run`.
- [ ] Run a release server/client smoke test and record its output.
- [ ] Compare every design requirement with code/tests/docs, fix gaps test-first, and inspect `git diff --check` plus `git status`.

