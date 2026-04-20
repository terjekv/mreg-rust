mod support;

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

fn import_batch_run(c: &mut Criterion) {
    let runtime = support::runtime();

    c.bench_function("import_batch_run_canonical", |b| {
        b.iter_batched(
            || {
                let storage = support::memory_storage();
                let summary = runtime
                    .block_on(
                        storage
                            .imports()
                            .create_import_batch(support::import_batch_command()),
                    )
                    .expect("create import batch");
                (storage, summary.id())
            },
            |(storage, id)| {
                let result = runtime
                    .block_on(storage.imports().run_import_batch(black_box(id)))
                    .expect("import batch runs");
                black_box(result);
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, import_batch_run);
criterion_main!(benches);
