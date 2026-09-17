use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use rustkv::database::Database;
use rustkv::metrics::Metrics;
use rustkv::protocol::{Response, read_response};
use rustkv::server::{Server, ServerConfig, ServerError};
use rustkv::store::StoreLimits;

struct RunningServer {
    address: SocketAddr,
    shutdown: Arc<AtomicBool>,
    metrics: Arc<Metrics>,
    handle: thread::JoinHandle<Result<(), ServerError>>,
}

impl RunningServer {
    fn stop(self) {
        self.shutdown.store(true, Ordering::Release);
        self.handle.join().unwrap().unwrap();
    }
}

fn start_server() -> RunningServer {
    let database = Arc::new(Database::in_memory(StoreLimits::default()));
    let metrics = Arc::new(Metrics::new());
    let shutdown = Arc::new(AtomicBool::new(false));
    let server = Server::bind(
        ServerConfig {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            max_connections: 16,
        },
        database,
        Arc::clone(&metrics),
    )
    .unwrap();
    let address = server.local_addr().unwrap();
    let signal = Arc::clone(&shutdown);
    let handle = thread::spawn(move || server.run(signal));
    RunningServer {
        address,
        shutdown,
        metrics,
        handle,
    }
}

#[test]
fn malformed_command_receives_deterministic_error() {
    let server = start_server();
    let mut stream = TcpStream::connect(server.address).unwrap();
    stream.write_all(&[0, 0, 0, 2, 1, 99]).unwrap();

    let response = read_response(&mut stream).unwrap().unwrap();
    assert!(matches!(response, Response::Error { code: 400, .. }));
    drop(stream);

    server.stop();
}

#[test]
fn oversized_frame_is_rejected_without_sending_its_payload() {
    let server = start_server();
    let mut stream = TcpStream::connect(server.address).unwrap();
    stream.write_all(&1_048_577_u32.to_be_bytes()).unwrap();

    let response = read_response(&mut stream).unwrap().unwrap();
    assert!(matches!(response, Response::Error { code: 400, .. }));
    drop(stream);

    server.stop();
}

#[test]
fn abrupt_disconnect_does_not_leave_an_active_connection() {
    let server = start_server();
    let mut stream = TcpStream::connect(server.address).unwrap();
    stream.write_all(&[0, 0]).unwrap();
    drop(stream);

    let metrics = Arc::clone(&server.metrics);
    server.stop();
    assert_eq!(metrics.snapshot(0).active_connections, 0);
}
