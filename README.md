# RustKV

RustKV is a compact persistent network key-value server written in Rust. It is a portfolio systems project, not a Redis replacement: one process, one node, a deterministic binary protocol, concurrent TCP clients, synchronized shared state, an append-only log, TTLs, compaction, metrics, tests, and reproducible benchmarks.

## Architecture

```text
rustkv-cli / custom client
          │ length-prefixed TCP frames
          ▼
  connection thread ──► codec ──► executor
                                      │
                                      ▼
                               Arc<Database>
                         ┌────────────┼────────────┐
                         ▼            ▼            ▼
                  RwLock<Store>  Mutex<AppendLog>  atomic Metrics
                         │            │
                         └────────────┴──► disk
```

The server deliberately uses one capped OS thread per connection instead of async Rust. This keeps ownership, `Arc`, `RwLock`, blocking I/O, durability ordering, and their limitations visible. See [architecture](docs/architecture.md), [concurrency](docs/concurrency.md), and [design decisions](docs/design-decisions.md).

## Features

- Binary-safe `Vec<u8>` keys and values
- `SET`, `SET ... EX`, `GET`, `DELETE`, `EXISTS`, and `STATS`
- 1 MiB framed binary protocol with defensive parsing
- Concurrent clients with a configurable admission limit
- CRC32-protected append-only persistence and startup replay
- Safe recovery of a truncated final log record
- Fail-closed handling of interior corruption and checksum errors
- Absolute-timestamp TTL recovery
- Crash-conscious log compaction with Windows rename recovery
- Structured tracing logs and atomic counters
- Unit, integration, failure, restart, and concurrency tests
- Criterion microbenchmarks plus a network workload tool
- Windows and Linux CI

## Build

Requirements: stable Rust, Cargo, and the platform C/C++ linker required by the Rust toolchain.

```bash
cargo build --release
```

## Run the server

Persistent mode is the default and creates `rustkv.aof`:

```bash
cargo run --release --bin rustkvd
```

Useful options:

```text
rustkvd [--bind IP:PORT] [--data PATH | --memory]
        [--max-connections N] [--compact-on-start]
```

The default bind is `127.0.0.1:7878`; RustKV has no authentication or TLS and should not be exposed to an untrusted network. Ctrl+C stops admission and lets connection workers exit at the next 100 ms control check.

## Use the CLI

In another terminal:

```bash
cargo run --release --bin rustkv-cli -- SET language Rust
cargo run --release --bin rustkv-cli -- GET language
cargo run --release --bin rustkv-cli -- EXISTS language
cargo run --release --bin rustkv-cli -- SET session active EX 60
cargo run --release --bin rustkv-cli -- DELETE language
cargo run --release --bin rustkv-cli -- STATS
```

Pass `--addr 127.0.0.1:9000` before the command for a nondefault server address. The CLI accepts UTF-8 shell arguments, while the client library and protocol remain binary-safe.

## Test and lint

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

Integration tests bind port zero, communicate through real sockets, signal shutdown, and join server threads. Persistence tests use real temporary files and reopen the database.

## Benchmark

Microbenchmarks:

```bash
cargo bench --bench store
cargo bench --bench protocol
```

Network workload against a running in-memory server:

```bash
cargo run --release --bin rustkvd -- --memory
cargo run --release --bin rustkv-bench -- --mode set --operations 10000 --clients 1
cargo run --release --bin rustkv-bench -- --mode get --operations 10000 --clients 8
```

The tool reports its workload and local operations per second. Results apply only to the named machine, OS, build, server mode, value size, and client count. See [benchmarking](docs/benchmarks.md).

## Persistence summary

Each effective mutation is length-prefixed, checksummed, written, and `sync_data`-ed before memory changes and before success is returned. Recovery replays validated records in order. A final incomplete record is discarded at the last known-good boundary; complete corrupt records stop startup. See [persistence](docs/persistence.md).

RustKV does not claim ACID. A crash after the record reaches disk but before the response reaches the client creates an unknown client-visible outcome.

## Limitations

- Single process and single node
- No authentication, TLS, replication, clustering, or transactions
- One OS thread per active connection
- Synchronous persistence serializes mutations and pauses readers during disk sync
- Expired entries can occupy memory until a mutation, recovery, or compaction
- CLI arguments are UTF-8 even though the library protocol is binary-safe
- Directory metadata synchronization is available on Unix; Windows recovery relies on validated sidecar rename states

## Documentation

- [Architecture](docs/architecture.md)
- [Protocol](docs/protocol.md)
- [Persistence and recovery](docs/persistence.md)
- [Concurrency](docs/concurrency.md)
- [Design decisions](docs/design-decisions.md)
- [Benchmarks](docs/benchmarks.md)
- [Interview notes](docs/interview-notes.md)
- [Resume evidence](docs/resume-evidence.md)

## Roadmap

The core scope is complete. Reasonable future work is property-based parser testing, fuzzing, configurable group commit, background expiration, and an explicit online admin compaction command. Replication, Raft, sharding, and SQL remain non-goals.

## License

MIT

