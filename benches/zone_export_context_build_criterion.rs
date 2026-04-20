mod support;

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use serde_json::json;

use mreg_rust::domain::exports::CreateExportRun;

fn zone_export_context_build(c: &mut Criterion) {
    let runtime = support::runtime();
    let (storage, forward_zone, reverse_zone) = support::zone_export_storage(&runtime);

    c.bench_function("zone_export_context_build_forward", |b| {
        b.iter_batched(
            || {
                let cmd = CreateExportRun::new(
                    "bind-forward-zone",
                    Some("bench".to_string()),
                    "forward_zone",
                    json!({"zone_name": forward_zone.as_str()}),
                )
                .expect("export command");
                runtime
                    .block_on(storage.exports().create_export_run(cmd))
                    .expect("export run created")
                    .id()
            },
            |run_id| {
                let run = runtime
                    .block_on(storage.exports().run_export(black_box(run_id)))
                    .expect("export run executed");
                black_box(run);
            },
            BatchSize::SmallInput,
        );
    });

    c.bench_function("zone_export_context_build_reverse", |b| {
        b.iter_batched(
            || {
                let cmd = CreateExportRun::new(
                    "bind-reverse-zone",
                    Some("bench".to_string()),
                    "reverse_zone",
                    json!({"zone_name": reverse_zone.as_str()}),
                )
                .expect("export command");
                runtime
                    .block_on(storage.exports().create_export_run(cmd))
                    .expect("export run created")
                    .id()
            },
            |run_id| {
                let run = runtime
                    .block_on(storage.exports().run_export(black_box(run_id)))
                    .expect("export run executed");
                black_box(run);
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, zone_export_context_build);
criterion_main!(benches);
