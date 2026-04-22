mod support;

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};

use mreg_rust::domain::exports::render_export_template;

#[library_benchmark]
fn render_bind_forward_zone() {
    let template = support::export_template("bind-forward-zone");
    let context = support::forward_zone_template_data();
    let rendered = render_export_template(&template, &context).expect("template renders");
    black_box(rendered);
}

library_benchmark_group!(
    name = export_template_bind_forward_zone;
    benchmarks = render_bind_forward_zone
);

main!(library_benchmark_groups = export_template_bind_forward_zone);
