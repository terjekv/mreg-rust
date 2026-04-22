mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use serde_json::Value;

use mreg_rust::domain::imports::{ImportBatch, ImportItem, ImportKind, ImportOperation};

fn raw_items() -> Vec<(String, ImportKind, ImportOperation, Value)> {
    let payload = support::import_batch_payload();
    let raw = payload
        .get("items")
        .and_then(Value::as_array)
        .expect("items array")
        .clone();
    raw.into_iter()
        .map(|item| {
            let reference = item
                .get("ref")
                .and_then(Value::as_str)
                .expect("ref")
                .to_string();
            let kind: ImportKind = serde_json::from_value(item.get("kind").cloned().expect("kind"))
                .expect("kind enum");
            let operation: ImportOperation =
                serde_json::from_value(item.get("operation").cloned().expect("operation"))
                    .expect("operation enum");
            let attributes = item.get("attributes").cloned().unwrap_or(Value::Null);
            (reference, kind, operation, attributes)
        })
        .collect()
}

// Measures parsing JSON items into ImportItems and constructing an
// ImportBatch. The actual `_ref` resolution and attribute validation
// happens inside the storage backend's `run_import_batch` and is covered by
// the `import_batch_run_criterion` benchmark.
#[library_benchmark]
fn parse_import_items() {
    let raw = raw_items();
    let items: Vec<ImportItem> = raw
        .into_iter()
        .map(|(r, k, o, a)| ImportItem::new(r, k, o, a).expect("item"))
        .collect();
    let batch = ImportBatch::new(items).expect("batch");
    black_box(batch);
}

library_benchmark_group!(
    name = import_batch_parse;
    benchmarks = parse_import_items
);

main!(library_benchmark_groups = import_batch_parse);
