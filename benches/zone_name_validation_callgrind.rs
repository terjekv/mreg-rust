mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::domain::types::ZoneName;

#[library_benchmark]
fn validate_forward_zone() {
    let value = ZoneName::new("bench.test").expect("valid forward zone");
    black_box(value);
}

#[library_benchmark]
fn validate_in_addr_arpa_zone() {
    let value = ZoneName::new("0.10.in-addr.arpa").expect("valid in-addr.arpa zone");
    black_box(value);
}

#[library_benchmark]
fn validate_ip6_arpa_zone() {
    let value = ZoneName::new("0.0.0.0.8.b.d.0.1.0.0.2.ip6.arpa").expect("valid ip6.arpa zone");
    black_box(value);
}

library_benchmark_group!(
    name = zone_name_validation;
    benchmarks =
        validate_forward_zone,
        validate_in_addr_arpa_zone,
        validate_ip6_arpa_zone
);

main!(library_benchmark_groups = zone_name_validation);
