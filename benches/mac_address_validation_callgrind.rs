mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::domain::types::MacAddressValue;

#[library_benchmark]
fn validate_colon_form() {
    let value = MacAddressValue::new("aa:bb:cc:dd:ee:ff").expect("valid colon mac");
    black_box(value);
}

#[library_benchmark]
fn validate_dash_form() {
    let value = MacAddressValue::new("AA-BB-CC-DD-EE-FF").expect("valid dash mac");
    black_box(value);
}

#[library_benchmark]
fn validate_dotted_form() {
    let value = MacAddressValue::new("aabb.ccdd.eeff").expect("valid dotted mac");
    black_box(value);
}

library_benchmark_group!(
    name = mac_address_validation;
    benchmarks =
        validate_colon_form,
        validate_dash_form,
        validate_dotted_form
);

main!(library_benchmark_groups = mac_address_validation);
