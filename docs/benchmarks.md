# Benchmarking

RustKV has two complementary benchmark paths.

## Microbenchmarks

Criterion measures the store and codec without TCP or disk:

```bash
cargo bench --bench store
cargo bench --bench protocol
```

Current cases:

- borrowed GET of a 64-byte value
- SET of a 64-byte value into a fresh store
- encode a 64-byte SET frame
- decode a 64-byte SET frame

## Network workload

Build release binaries, run an in-memory server, then compare one and several clients:

```bash
cargo build --release
target/release/rustkvd --memory
target/release/rustkv-bench --mode set --operations 10000 --clients 1
target/release/rustkv-bench --mode set --operations 10000 --clients 8
target/release/rustkv-bench --mode get --operations 10000 --clients 1
target/release/rustkv-bench --mode get --operations 10000 --clients 8
```

GET workloads preload the requested keys before timing. `operations` is total across all clients. Persistent-mode SET includes one `sync_data` per mutation and must be reported separately from in-memory SET.

## Required result metadata

Record:

- date
- CPU and memory
- OS and architecture
- Rust/Cargo version
- Git commit
- release/debug profile
- in-memory or persistent server
- operation, value bytes, total operations, client count
- elapsed time and operations/second

## Local verification run

Recorded 2026-09-17 on implementation commit `6766eee`:

- CPU: 11th Gen Intel Core i5-11260H, 6 cores / 12 logical processors
- Memory: 7.7 GiB visible
- OS: Windows 11 Home 64-bit, build `10.0.26200`
- Rust/Cargo: 1.98.0, `x86_64-pc-windows-msvc`
- Profile: Cargo release/benchmark profile

Criterion used 20 samples, a 1-second warm-up, and a 2-second measurement window:

| Case | 95% estimate interval |
|---|---:|
| store GET, 64-byte value | 21.911–23.875 ns |
| store SET, 64-byte value | 147.68–155.08 ns |
| protocol SET decode, 64-byte value | 99.046–103.65 ns |
| protocol SET encode, 64-byte value | 295.22–325.63 ns |

The network server ran in memory-only mode at `127.0.0.1:7879`. Each case used 5,000 operations and a 64-byte value:

| Operation | Clients | Elapsed | Operations/second |
|---|---:|---:|---:|
| SET | 1 | 0.392005 s | 12,754.95 |
| SET | 8 | 0.095755 s | 52,216.70 |
| GET | 1 | 0.391803 s | 12,761.51 |
| GET | 8 | 0.085239 s | 58,658.32 |

The client waits for each response before issuing its next request, so the one-client cases include one local TCP round trip per operation. The multi-client figures show this specific laptop benefiting from concurrent outstanding work; they are not server capacity guarantees. Persistent SET performance is intentionally omitted from this table because per-mutation `sync_data` measures storage behavior and must be reported as a separate workload.
