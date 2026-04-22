mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::domain::exports::render_export_template;

#[library_benchmark]
fn render_dhcp_canonical_json() {
    let template = support::export_template("dhcp-canonical-json");
    let context = support::dhcp_template_data();
    let rendered = render_export_template(&template, &context).expect("template renders");
    black_box(rendered);
}

library_benchmark_group!(
    name = export_template_dhcp_canonical_json;
    benchmarks = render_dhcp_canonical_json
);

main!(library_benchmark_groups = export_template_dhcp_canonical_json);
