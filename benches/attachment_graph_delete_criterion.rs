mod support;

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

fn attachment_graph_delete(c: &mut Criterion) {
    let runtime = support::runtime();

    c.bench_function("attachment_graph_delete", |b| {
        // Borrow the fixture so dropping the remaining storage is not timed.
        b.iter_batched_ref(
            || support::attachment_graph_storage(&runtime),
            |(storage, host)| {
                runtime
                    .block_on(storage.hosts().delete_host(black_box(&*host)))
                    .expect("attachment graph delete cascades");
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, attachment_graph_delete);
criterion_main!(benches);
