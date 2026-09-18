use std::env;
use std::process::ExitCode;

use rustkv::client::Client;
use rustkv::config::{client_help, parse_client_args};
use rustkv::protocol::{Response, StatsSnapshot};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("{}", client_help());
        return ExitCode::SUCCESS;
    }
    let options = match parse_client_args(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("rustkv-cli: {error}\n{}", client_help());
            return ExitCode::from(2);
        }
    };
    let mut client = match Client::connect(options.address) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("rustkv-cli: {error}");
            return ExitCode::FAILURE;
        }
    };
    match client.execute(&options.command) {
        Ok(Response::Error { code, message }) => {
            eprintln!("server error {code}: {message}");
            ExitCode::from(2)
        }
        Ok(response) => {
            print_response(response);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("rustkv-cli: {error}");
            ExitCode::FAILURE
        }
    }
}

fn print_response(response: Response) {
    match response {
        Response::Ok => println!("OK"),
        Response::Value(Some(value)) => println!("{}", display_bytes(&value)),
        Response::Value(None) => println!("(nil)"),
        Response::Boolean(value) => println!("{}", u8::from(value)),
        Response::Stats(stats) => print_stats(&stats),
        Response::Error { .. } => {}
    }
}

fn display_bytes(value: &[u8]) -> String {
    match std::str::from_utf8(value) {
        Ok(text) => text.to_owned(),
        Err(_) => {
            let mut output = String::from("0x");
            for byte in value {
                use std::fmt::Write;
                let _ = write!(output, "{byte:02x}");
            }
            output
        }
    }
}

fn print_stats(stats: &StatsSnapshot) {
    println!("uptime_ms={}", stats.uptime_ms);
    println!("active_connections={}", stats.active_connections);
    println!("total_connections={}", stats.total_connections);
    println!("total_requests={}", stats.total_requests);
    println!("total_errors={}", stats.total_errors);
    println!("get_commands={}", stats.get_commands);
    println!("set_commands={}", stats.set_commands);
    println!("delete_commands={}", stats.delete_commands);
    println!("exists_commands={}", stats.exists_commands);
    println!("stats_commands={}", stats.stats_commands);
    println!("key_count={}", stats.key_count);
}
