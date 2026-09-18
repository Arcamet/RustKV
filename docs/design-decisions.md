# Design Decisions

## DD-001: One package, library-first

One Cargo package keeps navigation and CI compact. Library modules own behavior; three binaries remain adapters. A workspace would add ceremony without independent versioning needs.

## DD-002: Bytes instead of strings

Keys and values use `Vec<u8>`. This prevents protocol/storage coupling to UTF-8 and makes invalid UTF-8 a non-event. The shell CLI is intentionally a UTF-8 convenience layer.

## DD-003: Inherent store API instead of a backend trait

There is one storage implementation. Adding a trait now would create unused abstraction and complicate borrowed GET lifetimes. `Database` is the service boundary if another backend is ever justified.

## DD-004: Length-prefixed binary protocol

A line protocol is inspectable but needs escaping and cannot naturally carry arbitrary values. JSON adds UTF-8, schema, allocation, and base64 costs. Explicit binary framing demonstrates TCP message boundaries and defensive parsing directly.

## DD-005: Capped thread per connection

This makes OS scheduling and shared-state synchronization visible at the intended scale. The connection cap addresses resource exhaustion. Async is deferred until measurement demonstrates many idle connections are a real requirement.

## DD-006: `RwLock` around one map

The lock boundary is easy to prove and test. Sharded locks would improve some workloads but complicate snapshots, key counts, compaction, and multi-key evolution. V1 has no multi-key operations.

## DD-007: Synchronous append before memory

Acknowledged mutations call `sync_data` before changing memory. This creates a clear failure invariant at the cost of throughput and reader stalls. Group commit is planned, not claimed.

## DD-008: CRC32 detects corruption, not attackers

CRC32 is fast and sufficient for accidental corruption detection. It is not authentication. RustKV has no security boundary and binds loopback by default.

## DD-009: Lazy logical expiry

Reads compare absolute timestamps but do not mutate, preserving the read-lock path. Mutations purge expired entries and compaction omits them. A read-only workload can retain expired allocations; an active sweeper is deferred.

## DD-010: STATS instead of KEYS

STATS has bounded output and exposes engineering evidence. KEYS would copy and transmit an unbounded portion of the database while holding or coordinating a global view.

