mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::authz::{AuthorizationRequest, Principal};

fn benchmark_principal() -> Principal {
    Principal {
        id: "bench-user".to_string(),
        namespace: Vec::new(),
        groups: Vec::new(),
    }
}

#[library_benchmark]
fn build_host_auth_request_empty_networks() {
    let context = support::sample_host_auth_context(0, 0);
    let request = AuthorizationRequest::builder(
        benchmark_principal(),
        "host.get",
        "host",
        context.host().name().as_str().to_string(),
    )
    .attrs(support::host_auth_attrs(&context))
    .build();

    black_box(request);
}

#[library_benchmark]
fn build_host_auth_request_single_network() {
    let context = support::sample_host_auth_context(8, 1);
    let request = AuthorizationRequest::builder(
        benchmark_principal(),
        "host.get",
        "host",
        context.host().name().as_str().to_string(),
    )
    .attrs(support::host_auth_attrs(&context))
    .build();

    black_box(request);
}

#[library_benchmark]
fn build_host_auth_request_multi_network() {
    let context = support::sample_host_auth_context(16, 4);
    let request = AuthorizationRequest::builder(
        benchmark_principal(),
        "host.update.comment",
        "host",
        context.host().name().as_str().to_string(),
    )
    .attrs(support::host_auth_attrs(&context))
    .build();

    black_box(request);
}

library_benchmark_group!(
    name = authz_request_build;
    benchmarks =
        build_host_auth_request_empty_networks,
        build_host_auth_request_single_network,
        build_host_auth_request_multi_network
);

main!(library_benchmark_groups = authz_request_build);
