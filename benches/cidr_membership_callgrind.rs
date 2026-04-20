use std::hint::black_box;
use std::net::IpAddr;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::domain::types::CidrValue;

#[library_benchmark]
fn ipv4_in_cidr_hit() {
    let net = CidrValue::new("10.0.0.0/8").expect("valid v4 CIDR");
    let ip: IpAddr = "10.20.30.40".parse().expect("valid v4 ip");
    let contained = black_box(net.as_inner()).contains(black_box(&ip));
    black_box(contained);
}

#[library_benchmark]
fn ipv4_in_cidr_miss() {
    let net = CidrValue::new("10.0.0.0/8").expect("valid v4 CIDR");
    let ip: IpAddr = "192.168.1.1".parse().expect("valid v4 ip");
    let contained = black_box(net.as_inner()).contains(black_box(&ip));
    black_box(contained);
}

#[library_benchmark]
fn ipv6_in_cidr_hit() {
    let net = CidrValue::new("2001:db8::/32").expect("valid v6 CIDR");
    let ip: IpAddr = "2001:db8:1234:5678::1".parse().expect("valid v6 ip");
    let contained = black_box(net.as_inner()).contains(black_box(&ip));
    black_box(contained);
}

#[library_benchmark]
fn cidr_in_cidr_hit() {
    let outer = CidrValue::new("10.0.0.0/8").expect("valid outer");
    let inner = CidrValue::new("10.20.0.0/16").expect("valid inner");
    let contained = black_box(outer.as_inner()).contains(black_box(inner.as_inner()));
    black_box(contained);
}

#[library_benchmark]
fn cidr_in_cidr_miss() {
    let outer = CidrValue::new("10.0.0.0/8").expect("valid outer");
    let inner = CidrValue::new("192.168.0.0/16").expect("valid inner");
    let contained = black_box(outer.as_inner()).contains(black_box(inner.as_inner()));
    black_box(contained);
}

library_benchmark_group!(
    name = cidr_membership;
    benchmarks =
        ipv4_in_cidr_hit,
        ipv4_in_cidr_miss,
        ipv6_in_cidr_hit,
        cidr_in_cidr_hit,
        cidr_in_cidr_miss,
);

main!(library_benchmark_groups = cidr_membership);
