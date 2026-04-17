mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use serde_json::json;

use mreg_rust::domain::types::DnsName;

#[library_benchmark]
fn validate_txt_record() {
    let definition = support::record_type("TXT");
    let owner = DnsName::new("mail.bench.test").expect("valid owner name");
    let payload = json!({"value": "v=spf1 include:_spf.bench.test -all"});

    let result = definition
        .validate_record_input(&owner, Some(&payload), None)
        .expect("TXT record should validate");
    black_box(result);
}

#[library_benchmark]
fn validate_naptr_record() {
    let definition = support::record_type("NAPTR");
    let owner = DnsName::new("sip.bench.test").expect("valid owner name");
    let payload = json!({
        "order": 100,
        "preference": 10,
        "flags": "s",
        "services": "SIP+D2U",
        "regexp": "",
        "replacement": "sip.bench.test"
    });

    let result = definition
        .validate_record_input(&owner, Some(&payload), None)
        .expect("NAPTR record should validate");
    black_box(result);
}

#[library_benchmark]
fn validate_https_record() {
    let definition = support::record_type("HTTPS");
    let owner = DnsName::new("www.bench.test").expect("valid owner name");
    let payload = json!({
        "priority": 1,
        "target": "cdn.bench.test"
    });

    let result = definition
        .validate_record_input(&owner, Some(&payload), None)
        .expect("HTTPS record should validate");
    black_box(result);
}

library_benchmark_group!(
    name = record_validation;
    benchmarks =
        validate_txt_record,
        validate_naptr_record,
        validate_https_record
);

main!(library_benchmark_groups = record_validation);
