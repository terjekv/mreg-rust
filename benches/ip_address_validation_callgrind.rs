mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::domain::types::IpAddressValue;

#[library_benchmark]
fn validate_ipv4() {
    let value = IpAddressValue::new("192.168.42.10").expect("valid ipv4");
    black_box(value);
}

#[library_benchmark]
fn validate_ipv6_compressed() {
    let value = IpAddressValue::new("2001:db8::1").expect("valid compressed ipv6");
    black_box(value);
}

#[library_benchmark]
fn validate_ipv6_full() {
    let value = IpAddressValue::new("2001:0db8:0000:0000:0000:0000:abcd:1234")
        .expect("valid expanded ipv6");
    black_box(value);
}

library_benchmark_group!(
    name = ip_address_validation;
    benchmarks =
        validate_ipv4,
        validate_ipv6_compressed,
        validate_ipv6_full
);

main!(library_benchmark_groups = ip_address_validation);
