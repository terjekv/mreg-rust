mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::domain::types::EmailAddressValue;

#[library_benchmark]
fn validate_simple_email() {
    let value = EmailAddressValue::new("ops@bench.test").expect("valid email");
    black_box(value);
}

#[library_benchmark]
fn validate_plus_tag_email() {
    let value = EmailAddressValue::new("ops+benchmark@bench.test").expect("valid plus-tag email");
    black_box(value);
}

#[library_benchmark]
fn validate_subdomain_email() {
    let value = EmailAddressValue::new("alerts@reports.bench.test").expect("valid subdomain email");
    black_box(value);
}

library_benchmark_group!(
    name = email_address_validation;
    benchmarks =
        validate_simple_email,
        validate_plus_tag_email,
        validate_subdomain_email
);

main!(library_benchmark_groups = email_address_validation);
