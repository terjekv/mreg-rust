mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::domain::types::CidrValue;

#[library_benchmark]
fn validate_v4_24() {
    let value = CidrValue::new("10.0.0.0/24").expect("valid /24");
    black_box(value);
}

#[library_benchmark]
fn validate_v4_20() {
    let value = CidrValue::new("10.10.0.0/20").expect("valid /20");
    black_box(value);
}

#[library_benchmark]
fn validate_v6_64() {
    let value = CidrValue::new("2001:db8:abcd::/64").expect("valid v6 /64");
    black_box(value);
}

library_benchmark_group!(
    name = cidr_value_validation;
    benchmarks =
        validate_v4_24,
        validate_v4_20,
        validate_v6_64
);

main!(library_benchmark_groups = cidr_value_validation);
