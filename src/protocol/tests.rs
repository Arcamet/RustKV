use std::io::Cursor;
use std::io::{self, Read};
use std::time::Duration;

use super::{
    Command, ProtocolError, Response, StatsSnapshot, read_command, read_response, write_command,
    write_response,
};

fn framed(payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(4 + payload.len());
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

#[test]
fn get_command_has_stable_literal_encoding() {
    let mut bytes = Vec::new();
    write_command(&mut bytes, &Command::Get { key: b"k".to_vec() }).unwrap();

    assert_eq!(bytes, vec![0, 0, 0, 7, 1, 2, 0, 0, 0, 1, b'k']);
}

#[test]
fn every_command_round_trips() {
    let commands = [
        Command::Set {
            key: b"a".to_vec(),
            value: vec![0, 255, b'\n'],
            expires_in: None,
        },
        Command::Set {
            key: b"ttl".to_vec(),
            value: b"value".to_vec(),
            expires_in: Some(Duration::from_secs(30)),
        },
        Command::Get { key: b"a".to_vec() },
        Command::Delete { key: b"a".to_vec() },
        Command::Exists { key: b"a".to_vec() },
        Command::Stats,
    ];

    for command in commands {
        let mut bytes = Vec::new();
        write_command(&mut bytes, &command).unwrap();
        let decoded = read_command(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(decoded, Some(command));
    }
}

#[test]
fn every_response_round_trips() {
    let stats = StatsSnapshot {
        uptime_ms: 1,
        active_connections: 2,
        total_connections: 3,
        total_requests: 4,
        total_errors: 5,
        get_commands: 6,
        set_commands: 7,
        delete_commands: 8,
        exists_commands: 9,
        stats_commands: 10,
        key_count: 11,
    };
    let responses = [
        Response::Ok,
        Response::Value(Some(vec![0, 255])),
        Response::Value(None),
        Response::Boolean(true),
        Response::Boolean(false),
        Response::Stats(stats),
        Response::Error {
            code: 400,
            message: "bad request".to_owned(),
        },
    ];

    for response in responses {
        let mut bytes = Vec::new();
        write_response(&mut bytes, &response).unwrap();
        let decoded = read_response(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(decoded, Some(response));
    }
}

#[test]
fn eof_between_frames_is_clean_but_partial_prefix_is_an_error() {
    assert_eq!(
        read_command(&mut Cursor::new(Vec::<u8>::new())).unwrap(),
        None
    );

    let error = read_command(&mut Cursor::new(vec![0, 0])).unwrap_err();
    assert!(matches!(error, ProtocolError::UnexpectedEof));
}

#[test]
fn partial_payload_is_an_error() {
    let error = read_command(&mut Cursor::new(vec![0, 0, 0, 5, 1, 5])).unwrap_err();
    assert!(matches!(error, ProtocolError::UnexpectedEof));
}

#[test]
fn oversized_frame_is_rejected_before_reading_a_payload() {
    let error = read_command(&mut Cursor::new(vec![0, 16, 0, 1])).unwrap_err();
    assert!(matches!(
        error,
        ProtocolError::FrameTooLarge {
            actual: 1_048_577,
            max: 1_048_576
        }
    ));
}

#[test]
fn unknown_version_opcode_and_trailing_bytes_are_rejected() {
    let version = read_command(&mut Cursor::new(framed(&[2, 5]))).unwrap_err();
    assert!(matches!(version, ProtocolError::UnsupportedVersion(2)));

    let opcode = read_command(&mut Cursor::new(framed(&[1, 99]))).unwrap_err();
    assert!(matches!(opcode, ProtocolError::UnknownTag(99)));

    let trailing = read_command(&mut Cursor::new(framed(&[1, 5, 0]))).unwrap_err();
    assert!(matches!(trailing, ProtocolError::TrailingBytes));
}

#[test]
fn invalid_boolean_response_is_rejected() {
    let error = read_response(&mut Cursor::new(framed(&[1, 2, 7]))).unwrap_err();
    assert!(matches!(error, ProtocolError::Malformed("invalid boolean")));
}

#[test]
fn writer_rejects_fields_beyond_the_protocol_limits() {
    let error = write_command(&mut Vec::new(), &Command::Get { key: vec![0; 4097] }).unwrap_err();
    assert!(matches!(error, ProtocolError::KeyTooLarge { actual: 4097 }));

    let error = write_command(
        &mut Vec::new(),
        &Command::Set {
            key: b"k".to_vec(),
            value: vec![0; 1_000_001],
            expires_in: None,
        },
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ProtocolError::ValueTooLarge { actual: 1_000_001 }
    ));
}

#[test]
fn timeout_before_a_frame_is_reported_as_idle_not_malformed_input() {
    struct IdleReader;

    impl Read for IdleReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::WouldBlock, "idle"))
        }
    }

    let error = read_command(&mut IdleReader).unwrap_err();
    assert!(matches!(error, ProtocolError::Idle));
}
