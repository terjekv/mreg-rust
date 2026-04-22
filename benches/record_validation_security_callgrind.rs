mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use serde_json::json;

use mreg_rust::domain::types::DnsName;

#[library_benchmark]
fn validate_sshfp_record() {
    let definition = support::record_type("SSHFP");
    let owner = DnsName::new("host.bench.test").expect("valid owner name");
    let payload = json!({
        "algorithm": 2,
        "fp_type": 2,
        "fingerprint": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
    });

    let result = definition
        .validate_record_input(&owner, Some(&payload), None)
        .expect("SSHFP record should validate");
    black_box(result);
}

#[library_benchmark]
fn validate_caa_record() {
    let definition = support::record_type("CAA");
    let owner = DnsName::new("bench.test").expect("valid owner name");
    let payload = json!({
        "flags": 0,
        "tag": "issue",
        "value": "letsencrypt.org"
    });

    let result = definition
        .validate_record_input(&owner, Some(&payload), None)
        .expect("CAA record should validate");
    black_box(result);
}

#[library_benchmark]
fn validate_tlsa_record() {
    let definition = support::record_type("TLSA");
    let owner = DnsName::new("_443._tcp.bench.test").expect("valid owner name");
    let payload = json!({
        "usage": 3,
        "selector": 1,
        "matching_type": 1,
        "certificate_data": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
    });

    let result = definition
        .validate_record_input(&owner, Some(&payload), None)
        .expect("TLSA record should validate");
    black_box(result);
}

library_benchmark_group!(
    name = record_validation_security;
    benchmarks =
        validate_sshfp_record,
        validate_caa_record,
        validate_tlsa_record
);

main!(library_benchmark_groups = record_validation_security);
