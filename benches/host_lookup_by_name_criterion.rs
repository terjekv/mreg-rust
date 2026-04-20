mod support;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

use mreg_rust::domain::types::Hostname;

fn host_lookup_by_name(c: &mut Criterion) {
    let runtime = support::runtime();

    let mut group = c.benchmark_group("host_lookup_by_name");
    for &count in &[100usize, 1000, 3000] {
        let (storage, _page, _filter) = support::host_listing_scenario(&runtime, count);
        // Pick a name in the middle of the seeded range so the lookup is
        // representative of an arbitrary hit.
        let target =
            Hostname::new(format!("host-{:04}.bench.test", count / 2)).expect("seeded hostname");

        group.bench_function(format!("hits_{count}"), |b| {
            b.iter(|| {
                let host = runtime
                    .block_on(storage.hosts().get_host_by_name(black_box(&target)))
                    .expect("host should be found");
                black_box(host);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, host_lookup_by_name);
criterion_main!(benches);
