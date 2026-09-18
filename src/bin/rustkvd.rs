use std::env;
use std::error::Error;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rustkv::config::{parse_server_args, server_help};
use rustkv::database::Database;
use rustkv::metrics::Metrics;
use rustkv::server::{Server, ServerConfig};
use rustkv::store::StoreLimits;
use tracing::info;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("{}", server_help());
        return ExitCode::SUCCESS;
    }
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rustkvd: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_target(false)
        .try_init()
        .map_err(|error| format!("could not initialize logging: {error}"))?;
    let options = parse_server_args(args)?;
    let database = match &options.data_path {
        Some(path) => {
            info!(path = %path.display(), "opening persistent database");
            Database::open(path, StoreLimits::default())?
        }
        None => {
            info!("starting in-memory database");
            Database::in_memory(StoreLimits::default())
        }
    };
    if options.compact_on_start {
        info!("compacting append log before accepting clients");
        database.compact()?;
    }
    let database = Arc::new(database);
    let metrics = Arc::new(Metrics::new());
    let server = Server::bind(
        ServerConfig {
            bind_addr: options.bind_addr,
            max_connections: options.max_connections,
        },
        database,
        metrics,
    )?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&shutdown);
    ctrlc::set_handler(move || signal.store(true, Ordering::Release))?;
    server.run(shutdown)?;
    Ok(())
}
