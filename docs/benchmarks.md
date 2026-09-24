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

## Matching the harness across revisions

`.github/scripts/benchmark_matrix.py` discovers targets from `Cargo.toml`. For
the existing targets whose fixtures or timing boundaries were repaired, it tells
Rust PR Bench to restore the head revision's benchmark source and shared fixture
file into the temporary base checkout before compiling. The base application's
`src/`, migrations, and Cargo manifest remain unchanged. This prevents comparing
different workloads or re-running the known broken record-listing fixture.

These four comparisons compile on their execution runners because their base
commands include the harness overlay. Other benchmarks retain the action's
shared precompilation. New attachment-import benchmarks are not overlaid onto
old revisions.

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
