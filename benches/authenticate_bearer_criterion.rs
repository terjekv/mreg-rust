mod support;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

fn authenticate_bearer(c: &mut Criterion) {
    let runtime = support::runtime();
    let client = support::scoped_authn_client();
    let token = support::signed_token();

    c.bench_function("authenticate_bearer_valid", |b| {
        b.iter(|| {
            let context = runtime
                .block_on(client.authenticate_bearer(black_box(&token)))
                .expect("bearer should authenticate");
            black_box(context);
        });
    });
}

criterion_group!(benches, authenticate_bearer);
criterion_main!(benches);
