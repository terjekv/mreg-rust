mod support;

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

use mreg_rust::domain::host::AssignIpAddress;

fn ip_allocation_matrix(c: &mut Criterion) {
    let runtime = support::runtime();

    let mut group = c.benchmark_group("ip_allocation_matrix");

    for (label, cidr, existing) in [
        ("slash28_dense", "10.20.0.0/28", 12usize),
        ("slash24_medium", "10.20.1.0/24", 64),
        ("slash24_dense", "10.20.2.0/24", 250),
        ("slash16_sparse", "10.21.0.0/16", 100),
    ] {
        group.bench_function(label, |b| {
            b.iter_batched(
                || support::parametrized_allocation_scenario(&runtime, cidr, existing),
                |(storage, pending, network)| {
                    let cmd = AssignIpAddress::new(pending, None, Some(network), None)
                        .expect("auto-allocate command");
                    let result = runtime
                        .block_on(storage.hosts().assign_ip_address(black_box(cmd)))
                        .expect("auto allocation succeeds");
                    black_box(result);
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(benches, ip_allocation_matrix);
criterion_main!(benches);
