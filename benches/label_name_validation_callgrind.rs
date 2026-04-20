mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::domain::types::LabelName;

#[library_benchmark]
fn validate_short_label() {
    let value = LabelName::new("ops").expect("valid short label");
    black_box(value);
}

#[library_benchmark]
fn validate_label_with_dashes() {
    let value = LabelName::new("dev-team-east").expect("valid hyphenated label");
    black_box(value);
}

#[library_benchmark]
fn validate_long_label() {
    let raw = "a".repeat(60);
    let value = LabelName::new(raw).expect("valid long label");
    black_box(value);
}

library_benchmark_group!(
    name = label_name_validation;
    benchmarks =
        validate_short_label,
        validate_label_with_dashes,
        validate_long_label
);

main!(library_benchmark_groups = label_name_validation);
