use std::error::Error;
use std::fmt;
use std::io;
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tracing::{debug, info, warn};

use crate::database::Database;
use crate::metrics::Metrics;
use crate::protocol::{Response, write_response};

use super::connection;

#[derive(Debug, Clone, Copy)]
pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub max_connections: usize,
}

#[derive(Debug)]
pub enum ServerError {
    Io(io::Error),
    InvalidConfiguration(&'static str),
    WorkerPanicked,
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "server I/O error: {error}"),
            Self::InvalidConfiguration(message) => {
                write!(f, "invalid server configuration: {message}")
            }
            Self::WorkerPanicked => f.write_str("a client worker thread panicked"),
        }
    }
}

impl Error for ServerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ServerError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug)]
pub struct Server {
    listener: TcpListener,
    max_connections: u64,
    database: Arc<Database>,
    metrics: Arc<Metrics>,
}

impl Server {
    pub fn bind(
        config: ServerConfig,
        database: Arc<Database>,
        metrics: Arc<Metrics>,
    ) -> Result<Self, ServerError> {
        if config.max_connections == 0 {
            return Err(ServerError::InvalidConfiguration(
                "max_connections must be greater than zero",
            ));
        }
        let listener = TcpListener::bind(config.bind_addr)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            max_connections: u64::try_from(config.max_connections).unwrap_or(u64::MAX),
            database,
            metrics,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, ServerError> {
        Ok(self.listener.local_addr()?)
    }

    pub fn run(self, shutdown: Arc<AtomicBool>) -> Result<(), ServerError> {
        let address = self.listener.local_addr()?;
        info!(%address, max_connections = self.max_connections, "RustKV server listening");
        let mut workers = Vec::new();
        while !shutdown.load(Ordering::Acquire) {
            reap_finished(&mut workers)?;
            match self.listener.accept() {
                Ok((mut stream, peer)) => {
                    if self.metrics.active_connections() >= self.max_connections {
                        warn!(%peer, "connection limit reached");
                        let response = Response::Error {
                            code: 503,
                            message: "connection limit reached".to_owned(),
                        };
                        if let Err(error) = write_response(&mut stream, &response) {
                            debug!(%error, %peer, "could not send connection-limit response");
                        }
                        continue;
                    }
                    stream.set_nonblocking(false)?;
                    stream.set_nodelay(true)?;
                    stream.set_read_timeout(Some(Duration::from_millis(100)))?;
                    self.metrics.connection_opened();
                    let database = Arc::clone(&self.database);
                    let metrics = Arc::clone(&self.metrics);
                    let worker_shutdown = Arc::clone(&shutdown);
                    let worker = match thread::Builder::new()
                        .name(format!("rustkv-client-{peer}"))
                        .spawn(move || {
                            let _guard = ConnectionGuard::new(Arc::clone(&metrics), peer);
                            connection::handle(stream, database, metrics, worker_shutdown);
                        }) {
                        Ok(handle) => Worker { handle },
                        Err(error) => {
                            self.metrics.connection_closed();
                            return Err(ServerError::Io(error));
                        }
                    };
                    workers.push(worker);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(ServerError::Io(error)),
            }
        }
        debug!(workers = workers.len(), "shutdown signal observed");
        join_workers(workers)?;
        info!("RustKV server stopped");
        Ok(())
    }
}

struct Worker {
    handle: JoinHandle<()>,
}

fn reap_finished(workers: &mut Vec<Worker>) -> Result<(), ServerError> {
    let mut index = 0;
    while index < workers.len() {
        if workers[index].handle.is_finished() {
            let worker = workers.swap_remove(index);
            if worker.handle.join().is_err() {
                return Err(ServerError::WorkerPanicked);
            }
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn join_workers(workers: Vec<Worker>) -> Result<(), ServerError> {
    for worker in workers {
        if worker.handle.join().is_err() {
            return Err(ServerError::WorkerPanicked);
        }
    }
    Ok(())
}

struct ConnectionGuard {
    metrics: Arc<Metrics>,
    peer: SocketAddr,
}

impl ConnectionGuard {
    fn new(metrics: Arc<Metrics>, peer: SocketAddr) -> Self {
        debug!(%peer, "client connected");
        Self { metrics, peer }
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.metrics.connection_closed();
        debug!(peer = %self.peer, "client disconnected");
    }
}
