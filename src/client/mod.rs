use std::error::Error;
use std::fmt;
use std::io;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};

use crate::protocol::{Command, ProtocolError, Response, read_response, write_command};

#[derive(Debug)]
pub enum ClientError {
    Io(io::Error),
    Protocol(ProtocolError),
    ServerClosed,
    AddressNotResolved,
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "client I/O error: {error}"),
            Self::Protocol(error) => write!(f, "client protocol error: {error}"),
            Self::ServerClosed => f.write_str("server closed before sending a response"),
            Self::AddressNotResolved => f.write_str("server address did not resolve"),
        }
    }
}

impl Error for ClientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Protocol(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ClientError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ProtocolError> for ClientError {
    fn from(value: ProtocolError) -> Self {
        Self::Protocol(value)
    }
}

#[derive(Debug)]
pub struct Client {
    stream: TcpStream,
}

impl Client {
    pub fn connect(address: impl ToSocketAddrs) -> Result<Self, ClientError> {
        let stream = TcpStream::connect(address)?;
        stream.set_nodelay(true)?;
        Ok(Self { stream })
    }

    pub fn connect_addr(address: SocketAddr) -> Result<Self, ClientError> {
        Self::connect(address)
    }

    pub fn execute(&mut self, command: &Command) -> Result<Response, ClientError> {
        write_command(&mut self.stream, command)?;
        read_response(&mut self.stream)?.ok_or(ClientError::ServerClosed)
    }
}
