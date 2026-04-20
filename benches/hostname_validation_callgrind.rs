mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::domain::types::Hostname;

#[library_benchmark]
fn validate_short_hostname() {
    let value = Hostname::new("alpha.bench.test").expect("valid hostname");
    black_box(value);
}

#[library_benchmark]
fn validate_fqdn_hostname() {
    let value = Hostname::new("alpha.bravo.bench.test.").expect("valid trailing-dot hostname");
    black_box(value);
}

#[library_benchmark]
fn validate_deep_label_chain() {
    let value = Hostname::new("alpha.bravo.charlie.delta.echo.foxtrot.bench.test")
        .expect("valid deep hostname");
    black_box(value);
}

library_benchmark_group!(
    name = hostname_validation;
    benchmarks =
        validate_short_hostname,
        validate_fqdn_hostname,
        validate_deep_label_chain
);

main!(library_benchmark_groups = hostname_validation);
