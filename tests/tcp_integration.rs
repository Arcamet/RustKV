use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use rustkv::client::Client;
use rustkv::database::Database;
use rustkv::metrics::Metrics;
use rustkv::protocol::{Command, Response};
use rustkv::server::{Server, ServerConfig};
use rustkv::store::StoreLimits;

#[test]
fn real_client_sets_gets_and_disconnects_cleanly() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();
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
    let server_shutdown = Arc::clone(&shutdown);
    let thread = thread::spawn(move || server.run(server_shutdown));

    let mut client = Client::connect(address).unwrap();
    assert_eq!(
        client
            .execute(&Command::Set {
                key: b"language".to_vec(),
                value: b"Rust".to_vec(),
                expires_in: None,
            })
            .unwrap(),
        Response::Ok
    );
    assert_eq!(
        client
            .execute(&Command::Get {
                key: b"language".to_vec(),
            })
            .unwrap(),
        Response::Value(Some(b"Rust".to_vec()))
    );
    drop(client);

    shutdown.store(true, Ordering::Release);
    thread.join().unwrap().unwrap();
    assert_eq!(metrics.snapshot(1).active_connections, 0);
    assert_eq!(metrics.snapshot(1).total_requests, 2);
}

#[test]
fn shutdown_closes_active_connections_and_joins_workers() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();
    let database = Arc::new(Database::in_memory(StoreLimits::default()));
    let metrics = Arc::new(Metrics::new());
    let shutdown = Arc::new(AtomicBool::new(false));
    let server = Server::bind(
        ServerConfig {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            max_connections: 4,
        },
        database,
        metrics,
    )
    .unwrap();
    let address = server.local_addr().unwrap();
    let signal = Arc::clone(&shutdown);
    let (sender, receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let result = server.run(signal);
        let _ = sender.send(());
        result
    });
    let mut client = Client::connect(address).unwrap();
    assert!(matches!(
        client.execute(&Command::Stats).unwrap(),
        Response::Stats(_)
    ));

    shutdown.store(true, Ordering::Release);
    let stopped_while_client_open = receiver.recv_timeout(Duration::from_millis(250)).is_ok();
    drop(client);
    if !stopped_while_client_open {
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    }
    server_thread.join().unwrap().unwrap();

    assert!(
        stopped_while_client_open,
        "server waited for the client to disconnect"
    );
}
