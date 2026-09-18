# Resume Evidence

Claims move categories only when the repository contains the implementation and, for the strongest category, automated behavioral evidence.

## IMPLEMENTED + TESTED

- Binary-safe in-memory key-value store with size limits, overwrite/delete semantics, borrowed lookup, and TTL visibility
- Versioned 1 MiB length-prefixed protocol with deterministic malformed/oversized/partial-frame handling
- Real TCP server/client communication with persistent connections, capped thread-per-connection admission, and graceful shutdown
- Concurrent readers/writers using `Arc`, `RwLock`, `Mutex`, atomics, and a documented lock order
- CRC32 append-only persistence with `sync_data`, ordered restart replay, truncated-tail repair, and fail-closed corruption handling
- Absolute-expiry persistence that does not resurrect expired overwrites
- Crash-conscious log compaction with replacement/backup recovery states
- Structured metrics and STATS response
- Unit, real-socket integration, concurrent-client, failure, and restart tests
- Windows-compatible regression coverage for accepted-socket mode and worker shutdown behavior

## IMPLEMENTED

- Structured `tracing` logs for server lifecycle, connections, malformed frames, and errors
- Server, one-shot client, and sequential/concurrent network benchmark binaries
- Criterion store and protocol benchmarks
- Windows/Linux GitHub Actions workflow for formatting, Clippy, tests, release build, and benchmark compilation
- Architecture, protocol, persistence, concurrency, design-decision, benchmark, interview, and limitations documentation

## PLANNED

- Parser fuzzing and property-based tests
- Configurable group commit / dedicated persistence writer
- Background active expiration
- Authentication and TLS if RustKV ever binds beyond a trusted host
- Replication or sharding only as a separate, explicitly scoped project

## Résumé bullet candidates

- Built a persistent key-value service in Rust with a bounded binary TCP protocol, concurrent connection threads, `Arc`/`RwLock` synchronization, TTLs, and structured runtime metrics.
- Engineered a CRC32 append-only log with synchronous mutation ordering, restart replay, torn-tail repair, corruption detection, and crash-conscious compaction across Windows rename states.
- Developed unit and real-socket integration coverage for malformed frames, concurrent clients, graceful shutdown, persistence recovery, and failure cases; automated format, Clippy, test, build, and benchmark compilation checks on Windows and Linux.

Do not add measured throughput numbers to a résumé bullet without recording the machine, server mode, build profile, value size, client count, command mix, and exact run in `docs/benchmarks.md`.

