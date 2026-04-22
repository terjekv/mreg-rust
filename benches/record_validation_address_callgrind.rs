mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use serde_json::json;

use mreg_rust::domain::types::DnsName;

#[library_benchmark]
fn validate_a_record() {
    let definition = support::record_type("A");
    let owner = DnsName::new("host.bench.test").expect("valid owner name");
    let payload = json!({"address": "10.0.0.1"});

    let result = definition
        .validate_record_input(&owner, Some(&payload), None)
        .expect("A record should validate");
    black_box(result);
}

#[library_benchmark]
fn validate_aaaa_record() {
    let definition = support::record_type("AAAA");
    let owner = DnsName::new("host.bench.test").expect("valid owner name");
    let payload = json!({"address": "2001:db8::1"});

    let result = definition
        .validate_record_input(&owner, Some(&payload), None)
        .expect("AAAA record should validate");
    black_box(result);
}

library_benchmark_group!(
    name = record_validation_address;
    benchmarks =
        validate_a_record,
        validate_aaaa_record
);

main!(library_benchmark_groups = record_validation_address);
