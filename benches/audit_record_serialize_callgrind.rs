use std::hint::black_box;

use chrono::Utc;
use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use serde_json::json;
use uuid::Uuid;

use mreg_rust::audit::HistoryEvent;

fn small_event() -> HistoryEvent {
    HistoryEvent::restore(
        Uuid::new_v4(),
        "alice".to_string(),
        "host".to_string(),
        Some(Uuid::new_v4()),
        "host-01.bench.test".to_string(),
        "create".to_string(),
        json!({"name": "host-01.bench.test"}),
        Utc::now(),
    )
}

fn large_event() -> HistoryEvent {
    let labels: Vec<_> = (0..16)
        .map(|i| json!({"id": Uuid::new_v4(), "name": format!("label-{i:02}")}))
        .collect();
    HistoryEvent::restore(
        Uuid::new_v4(),
        "service-account".to_string(),
        "host_group".to_string(),
        Some(Uuid::new_v4()),
        "audit-bench-group".to_string(),
        "update".to_string(),
        json!({
            "name": "audit-bench-group",
            "description": "audit bench group with many labels",
            "labels": labels,
        }),
        Utc::now(),
    )
}

#[library_benchmark]
fn serialize_small_event() {
    let event = small_event();
    let json = serde_json::to_string(black_box(&event)).expect("serialize");
    black_box(json);
}

#[library_benchmark]
fn serialize_large_event() {
    let event = large_event();
    let json = serde_json::to_string(black_box(&event)).expect("serialize");
    black_box(json);
}

library_benchmark_group!(
    name = audit_record_serialize;
    benchmarks = serialize_small_event, serialize_large_event,
);

main!(library_benchmark_groups = audit_record_serialize);
