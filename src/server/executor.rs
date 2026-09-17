use crate::database::{Database, DatabaseError};
use crate::metrics::{CommandKind, Metrics};
use crate::protocol::{Command, Response};

pub fn execute(database: &Database, metrics: &Metrics, command: Command) -> Response {
    let kind = command_kind(&command);
    metrics.record_request(kind);
    let result = match command {
        Command::Set {
            key,
            value,
            expires_in,
        } => database.set(key, value, expires_in).map(|()| Response::Ok),
        Command::Get { key } => database.get(&key).map(Response::Value),
        Command::Delete { key } => database.delete(&key).map(Response::Boolean),
        Command::Exists { key } => database.exists(&key).map(Response::Boolean),
        Command::Stats => database
            .key_count()
            .map(|key_count| Response::Stats(metrics.snapshot(key_count))),
    };
    match result {
        Ok(response) => response,
        Err(error) => {
            metrics.record_error();
            database_error_response(error)
        }
    }
}

fn command_kind(command: &Command) -> CommandKind {
    match command {
        Command::Set { .. } => CommandKind::Set,
        Command::Get { .. } => CommandKind::Get,
        Command::Delete { .. } => CommandKind::Delete,
        Command::Exists { .. } => CommandKind::Exists,
        Command::Stats => CommandKind::Stats,
    }
}

fn database_error_response(error: DatabaseError) -> Response {
    let code = match error {
        DatabaseError::Store(_) | DatabaseError::InvalidTtl | DatabaseError::ExpirationOverflow => {
            422
        }
        DatabaseError::Persistence(_)
        | DatabaseError::LockPoisoned(_)
        | DatabaseError::ClockBeforeUnixEpoch => 500,
    };
    Response::Error {
        code,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::database::Database;
    use crate::metrics::Metrics;
    use crate::protocol::{Command, Response};
    use crate::store::StoreLimits;

    use super::execute;

    #[test]
    fn executor_maps_all_data_commands_to_responses() {
        let database = Database::in_memory(StoreLimits::default());
        let metrics = Metrics::new();

        assert_eq!(
            execute(
                &database,
                &metrics,
                Command::Set {
                    key: b"key".to_vec(),
                    value: b"value".to_vec(),
                    expires_in: None,
                },
            ),
            Response::Ok
        );
        assert_eq!(
            execute(
                &database,
                &metrics,
                Command::Get {
                    key: b"key".to_vec()
                }
            ),
            Response::Value(Some(b"value".to_vec()))
        );
        assert_eq!(
            execute(
                &database,
                &metrics,
                Command::Exists {
                    key: b"key".to_vec()
                }
            ),
            Response::Boolean(true)
        );
        assert_eq!(
            execute(
                &database,
                &metrics,
                Command::Delete {
                    key: b"key".to_vec()
                }
            ),
            Response::Boolean(true)
        );
        assert_eq!(
            execute(
                &database,
                &metrics,
                Command::Get {
                    key: b"key".to_vec()
                }
            ),
            Response::Value(None)
        );
    }

    #[test]
    fn set_ex_and_stats_are_counted() {
        let database = Database::in_memory(StoreLimits::default());
        let metrics = Metrics::new();
        execute(
            &database,
            &metrics,
            Command::Set {
                key: b"ttl".to_vec(),
                value: b"value".to_vec(),
                expires_in: Some(Duration::from_secs(30)),
            },
        );

        let response = execute(&database, &metrics, Command::Stats);
        let Response::Stats(stats) = response else {
            panic!("expected STATS response");
        };
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.set_commands, 1);
        assert_eq!(stats.stats_commands, 1);
        assert_eq!(stats.key_count, 1);
    }

    #[test]
    fn database_errors_become_deterministic_error_responses() {
        let database = Database::in_memory(StoreLimits::default());
        let metrics = Metrics::new();

        let response = execute(&database, &metrics, Command::Get { key: Vec::new() });

        assert!(matches!(response, Response::Error { code: 422, .. }));
        assert_eq!(metrics.snapshot(0).total_errors, 1);
    }
}
