use criterion::{BatchSize, Criterion, black_box, criterion_group, criterion_main};
use rustkv::store::{Store, StoreLimits};

fn store_benchmarks(criterion: &mut Criterion) {
    let mut store = Store::new(StoreLimits::default());
    store
        .set(b"benchmark-key".to_vec(), vec![b'x'; 64], None)
        .expect("benchmark fixture must fit store limits");
    criterion.bench_function("store_get_64_bytes", |bencher| {
        bencher.iter(|| {
            black_box(
                store
                    .get(black_box(b"benchmark-key"), black_box(1))
                    .expect("benchmark key must be valid"),
            )
        });
    });

    criterion.bench_function("store_set_64_bytes", |bencher| {
        bencher.iter_batched(
            || Store::new(StoreLimits::default()),
            |mut store| {
                store
                    .set(
                        black_box(b"benchmark-key".to_vec()),
                        black_box(vec![b'x'; 64]),
                        None,
                    )
                    .expect("benchmark value must fit store limits")
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, store_benchmarks);
criterion_main!(benches);
