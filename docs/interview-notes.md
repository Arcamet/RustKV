# Interview Notes

## Why Rust?

Rust provides deterministic resource management, memory safety without garbage collection, explicit ownership across threads, and zero-cost access to OS networking and files. RustKV uses those properties in long-lived sockets, owned byte buffers, thread closures, locks, and file handles.

## How does ownership affect the design?

SET takes owned `Vec<u8>` values and moves them into the map. GET borrows a key and the pure store can return a slice tied to `&self`. Once the store is behind `RwLock`, that reference cannot outlive the guard, so the database copies only the selected value into an owned response. Connection threads receive cloned `Arc`s because spawned closures must own `'static` data.

## Why `Arc` and `RwLock`?

`Arc` lets connection threads share ownership; it does not permit mutation. `RwLock` permits simultaneous readers and one exclusive writer. The whole map is protected because the project favors a provable lock boundary over sharding complexity.

## How do clients communicate?

TCP carries four-byte-length-prefixed binary frames. The decoder uses exact reads because TCP may fragment or combine application messages. Fields are big-endian and length-prefixed, and every payload starts with protocol version 1.

## How are malformed inputs prevented from crashing the server?

The outer length is capped before allocation. Inner lengths have their own caps and checked arithmetic. Opcodes, versions, markers, TTLs, field exhaustion, and trailing bytes are validated. Errors are `Result` values; the connection gets code 400 when possible and is then closed.

## How does recovery work?

The server validates the AOF header, then each bounded length, body, and CRC32 before applying the record in order. A partial final record is truncated away. Complete corruption fails startup with an offset. Replay rebuilds memory and removes expired final state.

## What happens after a crash?

Valid synchronized records replay. A torn tail is ignored and removed. A client that lost its response cannot know whether a record reached disk before the crash; the write may appear after recovery. That is why the project does not claim ACID.

## What is persisted?

Effective SET/SET_EX and DELETE mutations. TTL is stored as an absolute Unix-millisecond expiration. Metrics and connection state are not persisted.

## Why not async?

The target is a compact service and interviewable systems design, not tens of thousands of idle sockets. A capped thread model keeps blocking persistence and synchronization straightforward. Tokio would be justified by measured connection-scale needs, not résumé vocabulary.

## Main bottleneck

Every durable mutation calls `sync_data` while holding the store write lock. That serializes writes and temporarily blocks reads. Disk latency, not hash-map lookup, should dominate persistent SET throughput.

## How would it scale?

First measure lock wait and sync latency. Then consider group commit or a dedicated log-writer actor, immutable read snapshots, lock sharding for independent keys, an async connection layer for idle-client scale, and only afterward replication or partitioning. Each step changes semantics and needs new failure tests.

## What did the project teach?

TCP is a stream rather than a message API; safe lifetimes can force deliberate ownership transitions; disk ordering is part of the public contract; graceful shutdown is platform-sensitive; concurrency tests need deterministic synchronization rather than sleeps; and performance claims need workload context.

## What would be redesigned?

For higher write throughput, separate mutation ordering from map read access and batch log sync. For many idle clients, replace thread-per-connection with async tasks. For stricter durability, define tested filesystem guarantees per OS and add fault injection. For bounded TTL memory, add an expiration index and background sweeper.

