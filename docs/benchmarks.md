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

The final local measurements are inserted only after running the release binaries on the current machine. They are evidence for this environment, not a universal performance claim.

