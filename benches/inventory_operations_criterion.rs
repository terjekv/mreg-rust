mod support;

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

use mreg_rust::{authz::AuthorizationRequest, domain::host::AssignIpAddress};

fn inventory_operations(c: &mut Criterion) {
    let runtime = support::runtime();
    let (storage, page, filter) = support::host_listing_scenario(&runtime, 1500);

    c.bench_function("host_list_address_filter_1500", |b| {
        b.iter(|| {
            let result = runtime
                .block_on(
                    storage
                        .hosts()
                        .list_hosts(black_box(&page), black_box(&filter)),
                )
                .expect("host listing should succeed");
            black_box(result.total);
        });
    });

    c.bench_function("host_auto_allocate_dense_network_180", |b| {
        b.iter_batched(
            || support::auto_allocation_scenario(&runtime, 180),
            |(storage, pending_host, network)| {
                let assignment = runtime
                    .block_on(
                        storage.hosts().assign_ip_address(
                            AssignIpAddress::new(pending_host, None, Some(network), None)
                                .expect("valid allocation request"),
                        ),
                    )
                    .expect("automatic allocation should succeed");
                black_box(assignment.id());
            },
            BatchSize::SmallInput,
        );
    });

    let (single_storage, single_host) = support::host_auth_context_scenario(&runtime, 8, 1);
    c.bench_function("host_auth_context_lookup_8_ips_single_network", |b| {
        b.iter(|| {
            let context = runtime
                .block_on(
                    single_storage
                        .hosts()
                        .get_host_auth_context(black_box(&single_host)),
                )
                .expect("host auth context should load");
            black_box(context.networks().len());
        });
    });

    let single_context = runtime
        .block_on(single_storage.hosts().get_host_auth_context(&single_host))
        .expect("host auth context should load");
    c.bench_function("host_auth_request_build_8_ips_single_network", |b| {
        b.iter(|| {
            let request = AuthorizationRequest::builder(
                black_box(mreg_rust::authz::Principal {
                    id: "bench-user".to_string(),
                    namespace: Vec::new(),
                    groups: Vec::new(),
                }),
                black_box("host.get"),
                black_box("host"),
                black_box(single_context.host().name().as_str().to_string()),
            )
            .attrs(black_box(support::host_auth_attrs(&single_context)))
            .build();
            black_box(request.resource.attrs.len());
        });
    });

    let (multi_storage, multi_host) = support::host_auth_context_scenario(&runtime, 16, 4);
    c.bench_function("host_auth_context_lookup_16_ips_4_networks", |b| {
        b.iter(|| {
            let context = runtime
                .block_on(
                    multi_storage
                        .hosts()
                        .get_host_auth_context(black_box(&multi_host)),
                )
                .expect("host auth context should load");
            black_box(context.networks().len());
        });
    });

    let multi_context = runtime
        .block_on(multi_storage.hosts().get_host_auth_context(&multi_host))
        .expect("host auth context should load");
    c.bench_function("host_auth_request_build_16_ips_4_networks", |b| {
        b.iter(|| {
            let request = AuthorizationRequest::builder(
                black_box(mreg_rust::authz::Principal {
                    id: "bench-user".to_string(),
                    namespace: Vec::new(),
                    groups: Vec::new(),
                }),
                black_box("host.update.comment"),
                black_box("host"),
                black_box(multi_context.host().name().as_str().to_string()),
            )
            .attrs(black_box(support::host_auth_attrs(&multi_context)))
            .build();
            black_box(request.resource.attrs.len());
        });
    });
}

criterion_group!(benches, inventory_operations);
criterion_main!(benches);
