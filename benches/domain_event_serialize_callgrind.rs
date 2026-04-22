use std::hint::black_box;

use chrono::Utc;
use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use serde_json::json;
use uuid::Uuid;

use mreg_rust::events::DomainEvent;

fn small_event() -> DomainEvent {
    DomainEvent {
        id: Uuid::new_v4(),
        actor: "alice".to_string(),
        resource_kind: "host".to_string(),
        resource_id: Some(Uuid::new_v4()),
        resource_name: "host-01.bench.test".to_string(),
        action: "create".to_string(),
        data: json!({"name": "host-01.bench.test"}),
        timestamp: Utc::now(),
    }
}

fn large_event() -> DomainEvent {
    let attachments: Vec<_> = (0..32)
        .map(|i| {
            json!({
                "id": Uuid::new_v4(),
                "network": format!("10.20.{i}.0/24"),
                "mac": format!("aa:bb:cc:dd:ee:{:02x}", i),
                "comment": format!("attachment {i}"),
            })
        })
        .collect();
    DomainEvent {
        id: Uuid::new_v4(),
        actor: "service-account".to_string(),
        resource_kind: "host".to_string(),
        resource_id: Some(Uuid::new_v4()),
        resource_name: "large.bench.test".to_string(),
        action: "update".to_string(),
        data: json!({
            "name": "large.bench.test",
            "comment": "fully populated host event",
            "attachments": attachments,
        }),
        timestamp: Utc::now(),
    }
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
    name = domain_event_serialize;
    benchmarks = serialize_small_event, serialize_large_event,
);

main!(library_benchmark_groups = domain_event_serialize);
