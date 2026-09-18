# RustKV Protocol Version 1

RustKV uses binary-safe, length-prefixed TCP frames. TCP is a byte stream; one read is not assumed to equal one message.

## Outer frame

```text
+----------------------+------------------------------+
| payload length: u32  | payload: exactly N bytes     |
| big-endian           |                              |
+----------------------+------------------------------+
```

- Maximum payload: 1,048,576 bytes
- Zero-length frames are invalid
- An oversized length is rejected before its payload is allocated
- EOF before any prefix byte is a clean disconnect
- EOF after a partial prefix or payload is an error

Every payload starts with `version: u8` (`1`) and a request opcode or response tag.

Byte strings are `[length: u32 big-endian][bytes]`. Keys must contain 1–4,096 bytes. Values may contain 0–1,000,000 bytes. Key/value bytes are never decoded as UTF-8.

## Requests

| Opcode | Command | Remaining payload |
|---:|---|---|
| `1` | `SET` | key, value |
| `2` | `GET` | key |
| `3` | `DELETE` | key |
| `4` | `EXISTS` | key |
| `5` | `STATS` | empty |
| `6` | `SET_EX` | key, value, TTL seconds as `u64` |

TTL uses positive whole seconds; programmatic clients reject subsecond durations rather than truncating them. Fields must consume the complete frame; trailing bytes are rejected.

## Responses

| Tag | Response | Remaining payload |
|---:|---|---|
| `0` | OK | empty |
| `1` | value | present marker `u8`; if `1`, a byte string |
| `2` | boolean | exactly `0` or `1` |
| `3` | stats | eleven `u64` counters in the order below |
| `255` | error | code `u16`, UTF-8 message byte string |

Stats order:

1. uptime milliseconds
2. active connections
3. total connections
4. total requests
5. total errors
6. GET commands
7. SET/SET_EX commands
8. DELETE commands
9. EXISTS commands
10. STATS commands
11. live key count

Current error codes:

- `400`: malformed protocol frame
- `422`: valid frame whose command violates database validation
- `500`: persistence, clock, or poisoned-lock failure
- `503`: connection admission limit reached

Errors are deterministic by category. Text explains the instance but clients should branch on the numeric code.

## Defensive behavior

All integer parsing is big-endian. Inner lengths use checked arithmetic and must match the containing frame. Unknown versions, request opcodes, response tags, invalid booleans, invalid optional markers, impossible field lengths, and trailing bytes are errors. A malformed request receives one error response when possible, then the server closes that connection.

The server’s read timeout is a shutdown-control mechanism. A timeout before any frame byte is treated as idle. A timeout after part of a frame is treated as a failed frame and closes the connection, limiting slow partial-request resource use.
