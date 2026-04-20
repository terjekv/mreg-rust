mod support;

use std::collections::HashMap;
use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::domain::filters::HostFilter;

fn equality_params() -> HashMap<String, String> {
    HashMap::from([
        ("name__exact".to_string(), "host01.bench.test".to_string()),
        ("zone__exact".to_string(), "bench.test".to_string()),
    ])
}

fn ranges_params() -> HashMap<String, String> {
    HashMap::from([
        ("name__contains".to_string(), "host".to_string()),
        ("address__startswith".to_string(), "10.0.".to_string()),
        (
            "created_at__gt".to_string(),
            "2026-01-01T00:00:00Z".to_string(),
        ),
        ("comment__icontains".to_string(), "Datacenter".to_string()),
    ])
}

#[library_benchmark]
fn parse_equality_filter() {
    let params = equality_params();
    let filter = HostFilter::from_query_params(params).expect("filter parses");
    black_box(filter);
}

#[library_benchmark]
fn parse_compound_filter() {
    let params = ranges_params();
    let filter = HostFilter::from_query_params(params).expect("filter parses");
    black_box(filter);
}

library_benchmark_group!(
    name = host_filter_parse;
    benchmarks =
        parse_equality_filter,
        parse_compound_filter
);

main!(library_benchmark_groups = host_filter_parse);
