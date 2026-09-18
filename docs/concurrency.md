# Concurrency

## Model

The listener runs in one thread. Each admitted client receives one named OS thread that owns its `TcpStream`. A configurable limit, default 128, prevents unbounded connection-thread creation. Completed workers are reaped during the accept loop.

This model was selected over a thread pool because persistent connections would pin pool workers, and over Tokio because expected scale does not justify async state machines or blocking-file integration.

## Shared ownership

Workers must own everything captured by `thread::spawn`. `Arc<Database>` and `Arc<Metrics>` provide reference-counted shared ownership across `'static` closures. Cloning an `Arc` does not clone the database.

## Protected state

- `RwLock<Store>`: protects the complete key map and TTL metadata
- `Mutex<AppendLog>`: protects file position and record order
- atomics: metrics that do not need a multi-field consistency transaction

GET/EXISTS/key count take a store read lock. SET/DELETE/compaction take a write lock. Persistent mutations acquire `store write -> append log` in that order. No path acquires them in reverse.

The network read and response write happen outside database locks. A selected GET value is cloned while its read guard exists, then the guard is released before network output.

## Poisoning and panics

`std::sync` locks are poisoned if a holder panics. RustKV maps poison to `DatabaseError::LockPoisoned`; it does not `unwrap` or silently assume invariants survived. Worker panics are detected when joined and become `ServerError::WorkerPanicked`.

## Shutdown

The listener polls a shared `AtomicBool`. Connection sockets have a 100 ms read timeout. An idle timeout rechecks shutdown and otherwise resumes waiting. This avoids keeping control socket clones alive and works on Windows, where shutting down a duplicated handle did not interrupt another thread’s blocking read during development.

## Limitations

- One thread and stack per client
- A client holding a partial frame can occupy a worker until the read timeout closes it
- Synchronous AOF writes hold the store write lock, blocking readers
- `RwLock` fairness is platform-dependent
- Atomics produce useful metrics but not a globally instantaneous snapshot

A higher-throughput redesign could assign all mutations to one writer actor, batch `sync_data`, and serve immutable/versioned reads separately. That would change durability latency and complexity and is not hidden behind the V1 design.

