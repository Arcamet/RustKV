use std::error::Error;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use crate::protocol::{Command, MAX_VALUE_BYTES};

const DEFAULT_PORT: u16 = 7878;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    message: String,
}

impl ConfigError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for ConfigError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerOptions {
    pub bind_addr: SocketAddr,
    pub data_path: Option<PathBuf>,
    pub max_connections: usize,
    pub compact_on_start: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientOptions {
    pub address: SocketAddr,
    pub command: Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkMode {
    Get,
    Set,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkOptions {
    pub address: SocketAddr,
    pub clients: usize,
    pub operations: usize,
    pub value_bytes: usize,
    pub mode: BenchmarkMode,
}

pub fn parse_server_args(args: Vec<String>) -> Result<ServerOptions, ConfigError> {
    let mut options = ServerOptions {
        bind_addr: default_address(),
        data_path: Some(PathBuf::from("rustkv.aof")),
        max_connections: 128,
        compact_on_start: false,
    };
    let mut memory_requested = false;
    let mut data_requested = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--bind" => {
                options.bind_addr = parse_socket(value_after(&args, index, "--bind")?)?;
                index += 2;
            }
            "--data" => {
                options.data_path = Some(PathBuf::from(value_after(&args, index, "--data")?));
                data_requested = true;
                index += 2;
            }
            "--max-connections" => {
                options.max_connections = parse_positive_usize(
                    value_after(&args, index, "--max-connections")?,
                    "max connections",
                )?;
                index += 2;
            }
            "--memory" => {
                options.data_path = None;
                memory_requested = true;
                index += 1;
            }
            "--compact-on-start" => {
                options.compact_on_start = true;
                index += 1;
            }
            unknown => {
                return Err(ConfigError::new(format!(
                    "unknown server argument: {unknown}"
                )));
            }
        }
    }
    if memory_requested && data_requested {
        return Err(ConfigError::new("--memory conflicts with --data"));
    }
    if options.compact_on_start && options.data_path.is_none() {
        return Err(ConfigError::new(
            "--compact-on-start requires persistent mode",
        ));
    }
    Ok(options)
}

pub fn parse_client_args(args: Vec<String>) -> Result<ClientOptions, ConfigError> {
    let mut address = default_address();
    let mut index = 0;
    if args.first().is_some_and(|argument| argument == "--addr") {
        address = parse_socket(value_after(&args, 0, "--addr")?)?;
        index = 2;
    }
    let command_name = args
        .get(index)
        .ok_or_else(|| ConfigError::new("missing command"))?
        .to_ascii_uppercase();
    let fields = &args[index + 1..];
    let command = match command_name.as_str() {
        "SET" => parse_set(fields)?,
        "GET" => Command::Get {
            key: single_key(fields, "GET")?,
        },
        "DELETE" => Command::Delete {
            key: single_key(fields, "DELETE")?,
        },
        "EXISTS" => Command::Exists {
            key: single_key(fields, "EXISTS")?,
        },
        "STATS" if fields.is_empty() => Command::Stats,
        "STATS" => return Err(ConfigError::new("STATS takes no arguments")),
        unknown => return Err(ConfigError::new(format!("unknown command: {unknown}"))),
    };
    Ok(ClientOptions { address, command })
}

pub fn parse_benchmark_args(args: Vec<String>) -> Result<BenchmarkOptions, ConfigError> {
    let mut options = BenchmarkOptions {
        address: default_address(),
        clients: 1,
        operations: 10_000,
        value_bytes: 64,
        mode: BenchmarkMode::Get,
    };
    let mut index = 0;
    while index < args.len() {
        let name = args[index].as_str();
        let value = value_after(&args, index, name)?;
        match name {
            "--addr" => options.address = parse_socket(value)?,
            "--clients" => options.clients = parse_positive_usize(value, "clients")?,
            "--operations" => {
                options.operations = parse_positive_usize(value, "operations")?;
            }
            "--value-bytes" => {
                options.value_bytes = parse_positive_usize(value, "value bytes")?;
                if options.value_bytes > MAX_VALUE_BYTES {
                    return Err(ConfigError::new(format!(
                        "value bytes exceeds protocol maximum {MAX_VALUE_BYTES}"
                    )));
                }
            }
            "--mode" => {
                options.mode = match value.to_ascii_lowercase().as_str() {
                    "get" => BenchmarkMode::Get,
                    "set" => BenchmarkMode::Set,
                    _ => return Err(ConfigError::new("benchmark mode must be get or set")),
                };
            }
            unknown => {
                return Err(ConfigError::new(format!(
                    "unknown benchmark argument: {unknown}"
                )));
            }
        }
        index += 2;
    }
    if options.clients > options.operations {
        return Err(ConfigError::new("clients must not exceed total operations"));
    }
    Ok(options)
}

pub fn server_help() -> &'static str {
    "rustkvd [--bind IP:PORT] [--data PATH | --memory] [--max-connections N] [--compact-on-start]"
}

pub fn client_help() -> &'static str {
    "rustkv-cli [--addr IP:PORT] <SET key value [EX seconds] | GET key | DELETE key | EXISTS key | STATS>"
}

pub fn benchmark_help() -> &'static str {
    "rustkv-bench [--addr IP:PORT] [--clients N] [--operations N] [--value-bytes N] [--mode get|set]"
}

fn parse_set(fields: &[String]) -> Result<Command, ConfigError> {
    if fields.len() != 2 && fields.len() != 4 {
        return Err(ConfigError::new("SET requires key value [EX seconds]"));
    }
    let key = key_bytes(&fields[0])?;
    let value = fields[1].as_bytes().to_vec();
    let expires_in = if fields.len() == 4 {
        if !fields[2].eq_ignore_ascii_case("EX") {
            return Err(ConfigError::new("SET optional clause must be EX seconds"));
        }
        let seconds = fields[3]
            .parse::<u64>()
            .map_err(|_| ConfigError::new("TTL seconds must be a positive integer"))?;
        if seconds == 0 {
            return Err(ConfigError::new("TTL seconds must be greater than zero"));
        }
        Some(Duration::from_secs(seconds))
    } else {
        None
    };
    Ok(Command::Set {
        key,
        value,
        expires_in,
    })
}

fn single_key(fields: &[String], command: &str) -> Result<Vec<u8>, ConfigError> {
    if fields.len() != 1 {
        return Err(ConfigError::new(format!(
            "{command} requires exactly one key"
        )));
    }
    key_bytes(&fields[0])
}

fn key_bytes(value: &str) -> Result<Vec<u8>, ConfigError> {
    if value.is_empty() {
        return Err(ConfigError::new("key must not be empty"));
    }
    Ok(value.as_bytes().to_vec())
}

fn value_after<'a>(args: &'a [String], index: usize, name: &str) -> Result<&'a str, ConfigError> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| ConfigError::new(format!("{name} requires a value")))
}

fn parse_socket(value: &str) -> Result<SocketAddr, ConfigError> {
    value
        .parse()
        .map_err(|_| ConfigError::new(format!("invalid socket address: {value}")))
}

fn parse_positive_usize(value: &str, name: &str) -> Result<usize, ConfigError> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| ConfigError::new(format!("{name} must be a positive integer")))?;
    if parsed == 0 {
        return Err(ConfigError::new(format!(
            "{name} must be greater than zero"
        )));
    }
    Ok(parsed)
}

fn default_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_PORT)
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::path::PathBuf;
    use std::time::Duration;

    use crate::protocol::Command;

    use super::{BenchmarkMode, parse_benchmark_args, parse_client_args, parse_server_args};

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn server_defaults_are_loopback_persistent_and_bounded() {
        let options = parse_server_args(strings(&[])).unwrap();

        assert_eq!(
            options.bind_addr,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7878)
        );
        assert_eq!(options.data_path, Some(PathBuf::from("rustkv.aof")));
        assert_eq!(options.max_connections, 128);
        assert!(!options.compact_on_start);
    }

    #[test]
    fn server_accepts_explicit_memory_bind_limit_and_persistent_compaction() {
        let options = parse_server_args(strings(&[
            "--bind",
            "127.0.0.1:9000",
            "--max-connections",
            "32",
            "--memory",
        ]))
        .unwrap();

        assert_eq!(options.bind_addr.port(), 9000);
        assert_eq!(options.max_connections, 32);
        assert_eq!(options.data_path, None);
        assert!(!options.compact_on_start);

        let persistent = parse_server_args(strings(&["--compact-on-start"])).unwrap();
        assert!(persistent.data_path.is_some());
        assert!(persistent.compact_on_start);
    }

    #[test]
    fn server_rejects_conflicting_or_invalid_arguments() {
        assert!(parse_server_args(strings(&["--memory", "--data", "custom.aof"])).is_err());
        assert!(parse_server_args(strings(&["--max-connections", "0"])).is_err());
        assert!(parse_server_args(strings(&["--bind", "not-an-address"])).is_err());
        assert!(parse_server_args(strings(&["--unknown"])).is_err());
    }

    #[test]
    fn client_parses_set_ex_and_stats() {
        let options = parse_client_args(strings(&[
            "--addr",
            "127.0.0.1:9000",
            "SET",
            "session",
            "value",
            "EX",
            "60",
        ]))
        .unwrap();
        assert_eq!(options.address.port(), 9000);
        assert_eq!(
            options.command,
            Command::Set {
                key: b"session".to_vec(),
                value: b"value".to_vec(),
                expires_in: Some(Duration::from_secs(60)),
            }
        );

        assert_eq!(
            parse_client_args(strings(&["STATS"])).unwrap().command,
            Command::Stats
        );
    }

    #[test]
    fn client_rejects_invalid_arity_unknown_commands_and_zero_ttl() {
        assert!(parse_client_args(strings(&["GET"])).is_err());
        assert!(parse_client_args(strings(&["NOPE", "key"])).is_err());
        assert!(parse_client_args(strings(&["SET", "key", "value", "EX", "0"])).is_err());
    }

    #[test]
    fn benchmark_options_support_sequential_and_concurrent_workloads() {
        let defaults = parse_benchmark_args(strings(&[])).unwrap();
        assert_eq!(defaults.clients, 1);
        assert_eq!(defaults.operations, 10_000);
        assert_eq!(defaults.value_bytes, 64);
        assert_eq!(defaults.mode, BenchmarkMode::Get);

        let options = parse_benchmark_args(strings(&[
            "--clients",
            "8",
            "--operations",
            "4000",
            "--value-bytes",
            "256",
            "--mode",
            "set",
        ]))
        .unwrap();
        assert_eq!(options.clients, 8);
        assert_eq!(options.operations, 4_000);
        assert_eq!(options.value_bytes, 256);
        assert_eq!(options.mode, BenchmarkMode::Set);
    }
}
