mod support;

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

fn host_delete_ptr_cascade(c: &mut Criterion) {
    let runtime = support::runtime();

    c.bench_function("host_delete_ptr_cascade_24", |b| {
        b.iter_batched(
            || support::ptr_cascade_storage(&runtime, 24),
            |(storage, host)| {
                runtime
                    .block_on(storage.hosts().delete_host(black_box(&host)))
                    .expect("host delete cascades");
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, host_delete_ptr_cascade);
criterion_main!(benches);
