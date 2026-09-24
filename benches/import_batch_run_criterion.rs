mod support;

use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};

use support::IpImportScenario;

fn import_batch_run(c: &mut Criterion) {
    let runtime = support::runtime();

    c.bench_function("import_batch_run_canonical", |b| {
        b.iter_batched_ref(
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
                    .block_on(storage.imports().run_import_batch(black_box(*id)))
                    .expect("import batch runs");
                black_box(result)
            },
            BatchSize::SmallInput,
        );
    });

    let mut group = c.benchmark_group("import_batch_run_ip");
    for scenario in [
        IpImportScenario::DirectManual,
        IpImportScenario::DirectAutomatic,
    ] {
        for count in [2, 32] {
            group.bench_with_input(
                BenchmarkId::new(scenario.name(), count),
                &count,
                |b, &count| {
                    b.iter_batched_ref(
                        || {
                            let storage = support::memory_storage();
                            let summary = runtime
                                .block_on(
                                    storage
                                        .imports()
                                        .create_import_batch(scenario.command(count)),
                                )
                                .expect("create import batch");
                            (storage, summary.id())
                        },
                        |(storage, id)| {
                            black_box(
                                runtime
                                    .block_on(storage.imports().run_import_batch(black_box(*id)))
                                    .expect("IP import batch runs"),
                            )
                        },
                        BatchSize::SmallInput,
                    );
                },
            );
        }
    }
    group.finish();
}

criterion_group!(benches, import_batch_run);
criterion_main!(benches);
