mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use serde_json::json;

use mreg_rust::domain::types::DnsName;

#[library_benchmark]
fn validate_mx_record() {
    let definition = support::record_type("MX");
    let owner = DnsName::new("bench.test").expect("valid owner name");
    let payload = json!({"preference": 10, "exchange": "mail.bench.test"});

    let result = definition
        .validate_record_input(&owner, Some(&payload), None)
        .expect("MX record should validate");
    black_box(result);
}

#[library_benchmark]
fn validate_srv_record() {
    let definition = support::record_type("SRV");
    let owner = DnsName::new("_sip._tcp.bench.test").expect("valid owner name");
    let payload = json!({
        "priority": 10,
        "weight": 5,
        "port": 5060,
        "target": "sip.bench.test"
    });

    let result = definition
        .validate_record_input(&owner, Some(&payload), None)
        .expect("SRV record should validate");
    black_box(result);
}

#[library_benchmark]
fn validate_cname_record() {
    let definition = support::record_type("CNAME");
    let owner = DnsName::new("alias.bench.test").expect("valid owner name");
    let payload = json!({"target": "primary.bench.test"});

    let result = definition
        .validate_record_input(&owner, Some(&payload), None)
        .expect("CNAME record should validate");
    black_box(result);
}

library_benchmark_group!(
    name = record_validation_mail;
    benchmarks =
        validate_mx_record,
        validate_srv_record,
        validate_cname_record
);

main!(library_benchmark_groups = record_validation_mail);
