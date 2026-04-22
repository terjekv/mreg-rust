use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::authz::{identity_key, scoped_identity_namespace};

#[library_benchmark]
fn empty_namespace() {
    let key = identity_key(black_box(&[]), black_box("alice"));
    black_box(key);
}

#[library_benchmark]
fn single_segment_namespace() {
    let ns = vec!["mreg".to_string()];
    let key = identity_key(black_box(&ns), black_box("alice"));
    black_box(key);
}

#[library_benchmark]
fn scoped_namespace() {
    let ns = scoped_identity_namespace(black_box("local"));
    let key = identity_key(black_box(&ns), black_box("alice"));
    black_box(key);
}

#[library_benchmark]
fn deep_namespace() {
    let ns = vec![
        "mreg".to_string(),
        "tenant".to_string(),
        "team".to_string(),
        "subteam".to_string(),
        "service".to_string(),
    ];
    let key = identity_key(black_box(&ns), black_box("alice.long.principal.identifier"));
    black_box(key);
}

library_benchmark_group!(
    name = principal_key_derive;
    benchmarks =
        empty_namespace,
        single_segment_namespace,
        scoped_namespace,
        deep_namespace,
);

main!(library_benchmark_groups = principal_key_derive);
