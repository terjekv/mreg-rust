mod support;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

use mreg_rust::domain::filters::RecordFilter;

fn record_listing(c: &mut Criterion) {
    let runtime = support::runtime();
    let (storage, page) = support::record_listing_storage(&runtime, 2000);
    let filter = RecordFilter::default();

    c.bench_function("record_listing_first_page_2000", |b| {
        b.iter(|| {
            let result = runtime
                .block_on(
                    storage
                        .records()
                        .list_records(black_box(&page), black_box(&filter)),
                )
                .expect("list_records succeeds");
            black_box(result.total);
        });
    });
}

criterion_group!(benches, record_listing);
criterion_main!(benches);
