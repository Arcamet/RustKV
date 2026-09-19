# Network Benchmark Evidence — 2026-09-19

This is a fresh run of the documented network workload in [docs/benchmarks.md](../benchmarks.md#network-workload),
captured as CI/GitHub-remote setup verification for RustKV. No workload, client count, or
value size was changed from that procedure.

## Environment

- Date: 2026-09-19
- OS: Microsoft Windows 11 Home, build `10.0.26200`, 64-bit
- CPU: 11th Gen Intel(R) Core(TM) i5-11260H @ 2.60GHz, 6 cores / 12 logical processors
- Memory: 8,316,043,264 bytes total physical (~7.74 GiB)
- Rust/Cargo: `rustc 1.98.0 (88d9e12ae 2026-08-18)`, `cargo 1.98.0 (797e8a9bc 2026-08-05)`, `x86_64-pc-windows-msvc`
- Git commit: `73c16146e9a701cee17502916e34cd221f4ea431` (`73c1614`)
- Profile: `cargo build --release`
- Server mode: in-memory (`--memory`), listening on `127.0.0.1:7878`

## Exact commands

```bash
cargo build --release
target/release/rustkvd --memory
target/release/rustkv-bench --mode set --operations 10000 --clients 1
target/release/rustkv-bench --mode set --operations 10000 --clients 8
target/release/rustkv-bench --mode get --operations 10000 --clients 1
target/release/rustkv-bench --mode get --operations 10000 --clients 8
```

`operations` is total across all clients, as documented. Value size is the binary's default (64 bytes).

## Raw output

```
=== SET, 1 client ===
os=windows arch=x86_64
address=127.0.0.1:7878
mode=Set
clients=1
operations=10000
value_bytes=64
elapsed_seconds=0.827160
operations_per_second=12089.56
=== SET, 8 clients ===
os=windows arch=x86_64
address=127.0.0.1:7878
mode=Set
clients=8
operations=10000
value_bytes=64
elapsed_seconds=0.315234
operations_per_second=31722.49
=== GET, 1 client ===
os=windows arch=x86_64
address=127.0.0.1:7878
mode=Get
clients=1
operations=10000
value_bytes=64
elapsed_seconds=0.737012
operations_per_second=13568.31
=== GET, 8 clients ===
os=windows arch=x86_64
address=127.0.0.1:7878
mode=Get
clients=8
operations=10000
value_bytes=64
elapsed_seconds=0.172289
operations_per_second=58042.01
=== SET, 8 clients (rerun 2) ===
os=windows arch=x86_64
address=127.0.0.1:7878
mode=Set
clients=8
operations=10000
value_bytes=64
elapsed_seconds=0.298927
operations_per_second=33453.02
=== SET, 8 clients (rerun 3) ===
os=windows arch=x86_64
address=127.0.0.1:7878
mode=Set
clients=8
operations=10000
value_bytes=64
elapsed_seconds=0.305526
operations_per_second=32730.39
```

## Summary

| Operation | Clients | Elapsed | Operations/second |
|---|---:|---:|---:|
| SET | 1 | 0.827160 s | 12,089.56 |
| SET | 8 | 0.315234 s | 31,722.49 |
| GET | 1 | 0.737012 s | 13,568.31 |
| GET | 8 | 0.172289 s | 58,042.01 |

SET-8-clients was rerun twice more (33,453.02 and 32,730.39 ops/sec) to rule out a one-off
fluctuation; all three runs land in the same 31.7k–33.5k band.

## Comparison with previously documented figures

The previously documented run (`docs/benchmarks.md`, commit `6766eee`) reported approximately
12.7k ops/sec single-client and a 52k–59k ops/sec eight-client range, using 5,000 operations
instead of the 10,000 used here.

- Single-client SET (12,089.56) and GET (13,568.31) are consistent with the ~12.7k figure,
  within normal run-to-run variance.
- Eight-client GET (58,042.01) falls within the previously documented 52k–59k range.
- Eight-client SET (31,722.49, confirmed by two reruns at 33,453.02 and 32,730.39) is
  **below** the previously documented 52k–59k range. This machine was also running other
  foreground workloads (this verification session, an active GitHub Actions watch process)
  during this benchmark, unlike the original isolated run. This is reported as measured,
  without adjustment — it is a real discrepancy against the earlier eight-client SET figure
  and should be treated as a data point for future investigation, not corrected here.
