mod support;

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use serde_json::json;

use mreg_rust::domain::exports::CreateExportRun;

fn dhcp_export_context_build(c: &mut Criterion) {
    let runtime = support::runtime();
    let storage = support::dhcp_export_storage(&runtime);

    c.bench_function("dhcp_export_context_build", |b| {
        b.iter_batched(
            || {
                let cmd = CreateExportRun::new(
                    "dhcp-canonical-json",
                    Some("bench".to_string()),
                    "dhcp",
                    json!({}),
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

criterion_group!(benches, dhcp_export_context_build);
criterion_main!(benches);
