mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::authn::LocalJwtValidator;

#[library_benchmark]
fn validate_access_token() {
    let token = support::signed_token();
    let validator = LocalJwtValidator::new(support::BENCH_JWT_KEY, support::BENCH_JWT_ISSUER);
    let context = validator.validate(&token).expect("token should validate");
    black_box(context);
}

library_benchmark_group!(
    name = jwt_validate;
    benchmarks = validate_access_token
);

main!(library_benchmark_groups = jwt_validate);
