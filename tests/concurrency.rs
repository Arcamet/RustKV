use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use rustkv::client::Client;
use rustkv::database::Database;
use rustkv::metrics::Metrics;
use rustkv::protocol::{Command, Response, read_response};
use rustkv::server::{Server, ServerConfig};
use rustkv::store::StoreLimits;
use tempfile::tempdir;

#[test]
fn simultaneous_tcp_clients_write_and_read_independent_keys() {
    let database = Arc::new(Database::in_memory(StoreLimits::default()));
    let metrics = Arc::new(Metrics::new());
    let shutdown = Arc::new(AtomicBool::new(false));
    let server = Server::bind(
        ServerConfig {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            max_connections: 16,
        },
        Arc::clone(&database),
        Arc::clone(&metrics),
    )
    .unwrap();
    let address = server.local_addr().unwrap();
    let signal = Arc::clone(&shutdown);
    let server_thread = thread::spawn(move || server.run(signal));

    let clients = 8;
    let barrier = Arc::new(Barrier::new(clients));
    let mut workers = Vec::new();
    for index in 0..clients {
        let barrier = Arc::clone(&barrier);
        workers.push(thread::spawn(move || {
            let key = format!("key-{index}").into_bytes();
            let value = format!("value-{index}").into_bytes();
            let mut client = Client::connect(address).unwrap();
            barrier.wait();
            assert_eq!(
                client
                    .execute(&Command::Set {
                        key: key.clone(),
                        value: value.clone(),
                        expires_in: None,
                    })
                    .unwrap(),
                Response::Ok
            );
            assert_eq!(
                client.execute(&Command::Get { key }).unwrap(),
                Response::Value(Some(value))
            );
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }
    shutdown.store(true, Ordering::Release);
    server_thread.join().unwrap().unwrap();

    assert_eq!(database.key_count().unwrap(), clients);
    assert_eq!(
        metrics.snapshot(clients).total_requests,
        (clients * 2) as u64
    );
    assert_eq!(metrics.snapshot(clients).active_connections, 0);
}

#[test]
fn concurrent_persistent_writers_all_survive_restart() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("data.aof");
    let database = Arc::new(Database::open(&path, StoreLimits::default()).unwrap());
    let writers = 4;
    let writes_per_thread = 10;
    let barrier = Arc::new(Barrier::new(writers));
    let mut workers = Vec::new();
    for writer in 0..writers {
        let database = Arc::clone(&database);
        let barrier = Arc::clone(&barrier);
        workers.push(thread::spawn(move || {
            barrier.wait();
            for index in 0..writes_per_thread {
                let key = format!("writer-{writer}-key-{index}").into_bytes();
                database.set(key, b"value".to_vec(), None).unwrap();
            }
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }
    drop(database);

    let recovered = Database::open(&path, StoreLimits::default()).unwrap();
    assert_eq!(recovered.key_count().unwrap(), writers * writes_per_thread);
}

#[test]
fn connection_limit_returns_busy_without_disturbing_active_client() {
    let database = Arc::new(Database::in_memory(StoreLimits::default()));
    let metrics = Arc::new(Metrics::new());
    let shutdown = Arc::new(AtomicBool::new(false));
    let server = Server::bind(
        ServerConfig {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            max_connections: 1,
        },
        database,
        Arc::clone(&metrics),
    )
    .unwrap();
    let address = server.local_addr().unwrap();
    let signal = Arc::clone(&shutdown);
    let server_thread = thread::spawn(move || server.run(signal));

    let mut first = Client::connect(address).unwrap();
    assert!(matches!(
        first.execute(&Command::Stats).unwrap(),
        Response::Stats(_)
    ));
    let mut second = TcpStream::connect(address).unwrap();
    let response = read_response(&mut second).unwrap().unwrap();
    assert!(matches!(response, Response::Error { code: 503, .. }));
    assert!(matches!(
        first.execute(&Command::Stats).unwrap(),
        Response::Stats(_)
    ));
    drop(first);
    drop(second);

    shutdown.store(true, Ordering::Release);
    server_thread.join().unwrap().unwrap();
    assert_eq!(metrics.snapshot(0).active_connections, 0);
}
