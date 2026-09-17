use std::net::TcpStream;
use std::sync::Arc;

use tracing::{debug, warn};

use crate::database::Database;
use crate::metrics::Metrics;
use crate::protocol::{Response, read_command, write_response};

use super::execute;

pub(crate) fn handle(mut stream: TcpStream, database: Arc<Database>, metrics: Arc<Metrics>) {
    loop {
        match read_command(&mut stream) {
            Ok(Some(command)) => {
                let response = execute(&database, &metrics, command);
                if let Err(error) = write_response(&mut stream, &response) {
                    debug!(%error, "client disconnected while a response was being written");
                    return;
                }
            }
            Ok(None) => return,
            Err(error) => {
                metrics.record_error();
                warn!(%error, "rejecting malformed client frame");
                let response = Response::Error {
                    code: 400,
                    message: error.to_string(),
                };
                if let Err(write_error) = write_response(&mut stream, &response) {
                    debug!(%write_error, "could not send protocol error to client");
                }
                return;
            }
        }
    }
}
