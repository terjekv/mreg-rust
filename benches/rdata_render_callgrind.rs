mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use minijinja::Environment;
use serde_json::{Value, json};

use mreg_rust::domain::resource_records::RecordTypeDefinition;

fn render(definition: &RecordTypeDefinition, payload: &Value) -> String {
    let template = definition
        .schema()
        .render_template()
        .expect("built-in type has render template");
    let mut env = Environment::new();
    env.add_template("record", template).expect("add template");
    env.get_template("record")
        .expect("template exists")
        .render(minijinja::value::Value::from_serialize(payload))
        .expect("render succeeds")
}

#[library_benchmark]
fn render_a() {
    let def = support::record_type("A");
    let payload = json!({"address": "10.10.0.1"});
    let rendered = render(black_box(&def), black_box(&payload));
    black_box(rendered);
}

#[library_benchmark]
fn render_mx() {
    let def = support::record_type("MX");
    let payload = json!({"preference": 10, "exchange": "mail.bench.test."});
    let rendered = render(black_box(&def), black_box(&payload));
    black_box(rendered);
}

#[library_benchmark]
fn render_srv() {
    let def = support::record_type("SRV");
    let payload = json!({
        "priority": 0,
        "weight": 5,
        "port": 5060,
        "target": "sip.bench.test.",
    });
    let rendered = render(black_box(&def), black_box(&payload));
    black_box(rendered);
}

library_benchmark_group!(
    name = rdata_render;
    benchmarks = render_a, render_mx, render_srv,
);

main!(library_benchmark_groups = rdata_render);
