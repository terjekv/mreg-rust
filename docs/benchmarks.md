# Benchmark comparisons

PR benchmarks compare Criterion wall-clock measurements and Gungraun instruction
counts against the PR base. The regression thresholds remain 8% and 3%,
respectively. A noisy timing result should be investigated with repeated,
alternating base/head measurements before attributing it to application code.

Deletion and import benchmarks use `iter_batched_ref`: fixture construction,
storage teardown, and dropping the returned import summary happen outside the
timed operation. The record-listing fixture creates its authoritative zone
before inserting A records.

The import workloads include:

- The original mixed-entity canonical batch.
- Direct manual and automatic IP imports, with 2 and 32 hosts and both IP families.
- Attachment-based manual and automatic IP imports, with the same sizes and IP
  families, MAC addresses, and staged references. IP items omit `host_name`.

Attachment imports are a separate benchmark target because these payloads failed
before the importer fix. Rust PR Bench reports that target as new when it does
not exist on the base revision; it cannot provide a meaningful timing comparison
against an implementation that rejects the workload.

## Profiler compatibility

Benchmark builds and measurements use `ubuntu-26.04` through Rust PR Bench's
`runs_on` input. Ubuntu 24.04's Valgrind 3.22 can miss symbols in Rust executables,
causing Callgrind to report zero instructions for a workload that executed.
Replaying the affected CI binary with Valgrind 3.26 restored instruction collection.
Treat an unexpected zero count as an invalid measurement, not an improvement.

## Matching the harness across revisions

`.github/scripts/benchmark_matrix.py` discovers targets from `Cargo.toml`. Rust
PR Bench compiles each revision's harness against that revision's application
using shared precompilation. Keep equivalent workloads and timing boundaries
when adapting a harness to a changed Rust API.

The temporary harness overlay used to compare the fixture repairs in PR #11 has
been removed now that those repairs are on `main`. Copying current harness code
onto older application code breaks comparisons when APIs change, such as the
introduction of `PageLimit` and private `PageRequest` fields. Future fixture or
timing-boundary repairs need an explicit comparison strategy that works with
both revisions; do not leave migration overlays enabled for subsequent PRs.

## Local checks

CI runs the affected benchmark fixtures once in addition to compiling them:

```sh
cargo bench --bench record_listing_criterion \
  --bench attachment_graph_delete_criterion \
  --bench host_delete_ptr_cascade_criterion \
  --bench import_batch_run_criterion \
  --bench import_attachment_ip_criterion -- --test
```

For timing measurements, replace `--test` with
`--noplot --sample-size 80 --measurement-time 6`. Keep workload definitions,
sampling arguments, toolchain, and dependencies consistent between revisions.
