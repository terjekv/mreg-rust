mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

#[library_benchmark]
fn issue_access_token() {
    let issuer = support::jwt_issuer();
    let principal = support::benchmark_principal();
    let (token, expires_at) = issuer
        .issue_access_token(&principal, "bench-user", "local", "local", None)
        .expect("issue benchmark token");
    black_box((token, expires_at));
}

library_benchmark_group!(
    name = jwt_issue;
    benchmarks = issue_access_token
);

main!(library_benchmark_groups = jwt_issue);
