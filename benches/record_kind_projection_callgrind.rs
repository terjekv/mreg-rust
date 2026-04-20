mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::api::v1::records::instances::RecordResponse;

#[library_benchmark]
fn project_typed_record() {
    let samples = support::record_kind_samples();
    let response = RecordResponse::from_domain(&samples[0]);
    black_box(response);
}

#[library_benchmark]
fn project_opaque_record() {
    let samples = support::record_kind_samples();
    let response = RecordResponse::from_domain(&samples[1]);
    black_box(response);
}

#[library_benchmark]
fn project_raw_rdata_record() {
    let samples = support::record_kind_samples();
    let response = RecordResponse::from_domain(&samples[2]);
    black_box(response);
}

#[library_benchmark]
fn project_malformed_record() {
    let samples = support::record_kind_samples();
    let response = RecordResponse::from_domain(&samples[3]);
    black_box(response);
}

library_benchmark_group!(
    name = record_kind_projection;
    benchmarks =
        project_typed_record,
        project_opaque_record,
        project_raw_rdata_record,
        project_malformed_record
);

main!(library_benchmark_groups = record_kind_projection);
