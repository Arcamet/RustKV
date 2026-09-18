use std::env;
use std::process::ExitCode;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

use rustkv::client::Client;
use rustkv::config::{BenchmarkMode, BenchmarkOptions, benchmark_help, parse_benchmark_args};
use rustkv::protocol::{Command, Response};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("{}", benchmark_help());
        return ExitCode::SUCCESS;
    }
    let options = match parse_benchmark_args(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("rustkv-bench: {error}\n{}", benchmark_help());
            return ExitCode::from(2);
        }
    };
    match run(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rustkv-bench: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(options: BenchmarkOptions) -> Result<(), String> {
    let value = vec![b'x'; options.value_bytes];
    if options.mode == BenchmarkMode::Get {
        preload(&options, &value)?;
    }

    let barrier = Arc::new(Barrier::new(options.clients + 1));
    let mut workers = Vec::with_capacity(options.clients);
    let mut start_index = 0;
    for client_index in 0..options.clients {
        let extra = usize::from(client_index < options.operations % options.clients);
        let count = options.operations / options.clients + extra;
        let range_start = start_index;
        start_index += count;
        let barrier = Arc::clone(&barrier);
        let value = value.clone();
        let address = options.address;
        let mode = options.mode;
        workers.push(thread::spawn(move || -> Result<usize, String> {
            let mut client = Client::connect(address).map_err(|error| error.to_string())?;
            barrier.wait();
            for operation in range_start..range_start + count {
                execute_operation(&mut client, mode, operation, &value)?;
            }
            Ok(count)
        }));
    }

    barrier.wait();
    let started = Instant::now();
    let mut completed = 0_usize;
    for worker in workers {
        completed += worker
            .join()
            .map_err(|_| "benchmark worker panicked".to_owned())??;
    }
    let elapsed = started.elapsed();
    let throughput = completed as f64 / elapsed.as_secs_f64();
    println!("os={} arch={}", env::consts::OS, env::consts::ARCH);
    println!("address={}", options.address);
    println!("mode={:?}", options.mode);
    println!("clients={}", options.clients);
    println!("operations={completed}");
    println!("value_bytes={}", options.value_bytes);
    println!("elapsed_seconds={:.6}", elapsed.as_secs_f64());
    println!("operations_per_second={throughput:.2}");
    Ok(())
}

fn preload(options: &BenchmarkOptions, value: &[u8]) -> Result<(), String> {
    let mut client = Client::connect(options.address).map_err(|error| error.to_string())?;
    for operation in 0..options.operations {
        let response = client
            .execute(&Command::Set {
                key: benchmark_key(operation),
                value: value.to_vec(),
                expires_in: None,
            })
            .map_err(|error| error.to_string())?;
        if response != Response::Ok {
            return Err(format!("preload SET returned {response:?}"));
        }
    }
    Ok(())
}

fn execute_operation(
    client: &mut Client,
    mode: BenchmarkMode,
    operation: usize,
    value: &[u8],
) -> Result<(), String> {
    let command = match mode {
        BenchmarkMode::Get => Command::Get {
            key: benchmark_key(operation),
        },
        BenchmarkMode::Set => Command::Set {
            key: benchmark_key(operation),
            value: value.to_vec(),
            expires_in: None,
        },
    };
    let response = client
        .execute(&command)
        .map_err(|error| error.to_string())?;
    let valid = match mode {
        BenchmarkMode::Get => matches!(response, Response::Value(Some(_))),
        BenchmarkMode::Set => response == Response::Ok,
    };
    if !valid {
        return Err(format!("unexpected response: {response:?}"));
    }
    Ok(())
}

fn benchmark_key(operation: usize) -> Vec<u8> {
    format!("benchmark-key-{operation}").into_bytes()
}
