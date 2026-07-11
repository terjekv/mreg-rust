mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::domain::types::DnsName;

#[library_benchmark]
fn validate_simple_fqdn() -> DnsName {
    let value = DnsName::new("host.bench.test").expect("valid fqdn");
    black_box(value)
}

#[library_benchmark]
fn validate_trailing_dot_fqdn() -> DnsName {
    let value = DnsName::new("host.bench.test.").expect("valid trailing-dot fqdn");
    black_box(value)
}

#[library_benchmark]
fn validate_mixed_case_fqdn() -> DnsName {
    let value = DnsName::new("Host.Bench.Test").expect("valid mixed-case fqdn");
    black_box(value)
}

#[library_benchmark]
fn validate_max_label_fqdn() -> DnsName {
    let label = "a".repeat(63);
    let raw = format!("{label}.bench.test");
    let value = DnsName::new(raw).expect("valid max-label fqdn");
    black_box(value)
}

library_benchmark_group!(
    name = dns_name_validation;
    benchmarks =
        validate_simple_fqdn,
        validate_trailing_dot_fqdn,
        validate_mixed_case_fqdn,
        validate_max_label_fqdn
);

main!(library_benchmark_groups = dns_name_validation);
