use std::collections::HashMap;
use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::domain::filters::{HostFilter, RecordFilter};

fn host_filter() -> HostFilter {
    HostFilter::from_query_params(HashMap::from([
        ("name__contains".to_string(), "bench".to_string()),
        ("zone__endswith".to_string(), ".test".to_string()),
        ("comment__contains".to_string(), "owned".to_string()),
        ("address__contains".to_string(), "10.10.".to_string()),
        ("created_at__gt".to_string(), "2026-01-01".to_string()),
    ]))
    .expect("host filter parses")
}

fn record_filter() -> RecordFilter {
    RecordFilter::from_query_params(HashMap::from([
        ("type_name__exact".to_string(), "A".to_string()),
        ("owner_kind__exact".to_string(), "host".to_string()),
        ("owner_name__contains".to_string(), "bench".to_string()),
    ]))
    .expect("record filter parses")
}

#[library_benchmark]
fn host_filter_sql_compile() {
    let filter = host_filter();
    let result = black_box(&filter).sql_conditions();
    black_box(result);
}

#[library_benchmark]
fn record_filter_sql_compile() {
    let filter = record_filter();
    let result = black_box(&filter).sql_conditions();
    black_box(result);
}

library_benchmark_group!(
    name = filter_sql_compile;
    benchmarks = host_filter_sql_compile, record_filter_sql_compile,
);

main!(library_benchmark_groups = filter_sql_compile);
