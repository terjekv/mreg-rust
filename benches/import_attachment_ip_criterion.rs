mod support;

use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};

use support::IpImportScenario;

// These payloads were rejected before the attachment import fix. Keep them in
// a separate target so CI reports a new benchmark rather than a base failure.
fn import_attachment_ip(c: &mut Criterion) {
    let runtime = support::runtime();
    let mut group = c.benchmark_group("import_attachment_ip");
    for scenario in [
        IpImportScenario::AttachmentManual,
        IpImportScenario::AttachmentAutomatic,
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
                                    .expect("attachment IP import batch runs"),
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

criterion_group!(benches, import_attachment_ip);
criterion_main!(benches);
