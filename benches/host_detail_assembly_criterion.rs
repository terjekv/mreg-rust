mod support;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

use mreg_rust::{
    api::v1::hosts::build_host_response,
    domain::{filters::HostFilter, host_view::HostViewExpansions, pagination::PageRequest},
};

fn host_detail_assembly(c: &mut Criterion) {
    let runtime = support::runtime();
    let (storage, small_host, large_host) = support::host_detail_fixtures(&runtime);
    let state = support::app_state_for(storage);

    let small = runtime
        .block_on(state.services.hosts().get(&small_host))
        .expect("small host fetched");
    let large = runtime
        .block_on(state.services.hosts().get(&large_host))
        .expect("large host fetched");

    c.bench_function("host_detail_assembly_small", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(build_host_response(&state, black_box(&small), true))
                .expect("response builds");
            black_box(response);
        });
    });

    c.bench_function("host_detail_assembly_large", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(build_host_response(&state, black_box(&large), true))
                .expect("response builds");
            black_box(response);
        });
    });

    let page = PageRequest::all();
    let filter = HostFilter::default();
    c.bench_function("host_detail_list_full_fixture", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(state.services.host_views().list(
                    black_box(&page),
                    black_box(&filter),
                    black_box(HostViewExpansions::detail()),
                ))
                .expect("detail list builds");
            black_box(response.total);
        });
    });
}

criterion_group!(benches, host_detail_assembly);
criterion_main!(benches);
