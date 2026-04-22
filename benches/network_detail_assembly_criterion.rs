mod support;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

use mreg_rust::api::v1::networks::build_network_response;

fn network_detail_assembly(c: &mut Criterion) {
    let runtime = support::runtime();
    let (storage, cidr) = support::network_detail_fixture(&runtime);
    let state = support::app_state_for(storage);

    let network = runtime
        .block_on(state.services.networks().get(&cidr))
        .expect("network fetched");

    c.bench_function("network_detail_assembly", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(build_network_response(&state, black_box(&network), true))
                .expect("response builds");
            black_box(response);
        });
    });
}

criterion_group!(benches, network_detail_assembly);
criterion_main!(benches);
