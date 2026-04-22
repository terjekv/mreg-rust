use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::domain::pagination::PageRequest;

const SIMPLE_QUERY: &str = "limit=100&sort_dir=asc";
const FULL_QUERY: &str =
    "after=11111111-2222-3333-4444-555555555555&limit=250&sort_by=name&sort_dir=desc";

#[library_benchmark]
fn deserialize_simple_query() {
    let parsed: PageRequest =
        serde_urlencoded::from_str(black_box(SIMPLE_QUERY)).expect("simple query parses");
    black_box(parsed);
}

#[library_benchmark]
fn deserialize_full_query() {
    let parsed: PageRequest =
        serde_urlencoded::from_str(black_box(FULL_QUERY)).expect("full query parses");
    black_box(parsed);
}

library_benchmark_group!(
    name = page_request_deserialize;
    benchmarks = deserialize_simple_query, deserialize_full_query,
);

main!(library_benchmark_groups = page_request_deserialize);
