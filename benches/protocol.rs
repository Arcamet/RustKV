use std::io::Cursor;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rustkv::protocol::{Command, read_command, write_command};

fn protocol_benchmarks(criterion: &mut Criterion) {
    let command = Command::Set {
        key: b"benchmark-key".to_vec(),
        value: vec![b'x'; 64],
        expires_in: None,
    };
    let mut encoded = Vec::new();
    write_command(&mut encoded, &command).expect("benchmark frame must encode");

    criterion.bench_function("protocol_decode_set_64_bytes", |bencher| {
        bencher.iter(|| {
            let mut cursor = Cursor::new(black_box(encoded.as_slice()));
            black_box(read_command(&mut cursor).expect("benchmark frame must decode"))
        });
    });

    criterion.bench_function("protocol_encode_set_64_bytes", |bencher| {
        bencher.iter(|| {
            let mut output = Vec::with_capacity(128);
            write_command(&mut output, black_box(&command)).expect("benchmark frame must encode");
            black_box(output)
        });
    });
}

criterion_group!(benches, protocol_benchmarks);
criterion_main!(benches);
