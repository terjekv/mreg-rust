mod support;

use std::collections::HashMap;
use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::domain::filters::RecordFilter;

fn type_only_params() -> HashMap<String, String> {
    HashMap::from([("type_name__exact".to_string(), "A".to_string())])
}

fn compound_params() -> HashMap<String, String> {
    HashMap::from([
        ("type_name__in".to_string(), "A,AAAA,CNAME".to_string()),
        ("owner_kind__exact".to_string(), "Host".to_string()),
        (
            "owner_name__endswith".to_string(),
            ".bench.test".to_string(),
        ),
    ])
}

#[library_benchmark]
fn parse_type_only_filter() {
    let params = type_only_params();
    let filter = RecordFilter::from_query_params(params).expect("filter parses");
    black_box(filter);
}

#[library_benchmark]
fn parse_compound_filter() {
    let params = compound_params();
    let filter = RecordFilter::from_query_params(params).expect("filter parses");
    black_box(filter);
}

library_benchmark_group!(
    name = record_filter_parse;
    benchmarks =
        parse_type_only_filter,
        parse_compound_filter
);

main!(library_benchmark_groups = record_filter_parse);
