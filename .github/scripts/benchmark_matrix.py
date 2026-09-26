#!/usr/bin/env python3
"""Keep benchmark repairs identical when measuring base and head application code."""

import argparse
import json
from pathlib import Path
import re
import shlex
import tomllib


# These existing targets changed their fixture or timing boundary. Running the
# old harness on base would either crash or compare different amounts of work.
SHARED_HARNESS_TARGETS = {
    "attachment_graph_delete_criterion",
    "host_delete_ptr_cascade_criterion",
    "import_batch_run_criterion",
    "record_listing_criterion",
    "wildcard_zone_match_criterion",
}


def benchmark_specs(manifest: dict, head_sha: str, criterion_args: str) -> list[dict]:
    if not re.fullmatch(r"[0-9a-f]{40}", head_sha):
        raise ValueError("head_sha must be a full Git commit SHA")
    specs = []
    for target in manifest["bench"]:
        name = target["name"]
        backend = "criterion" if name.endswith("_criterion") else "gungraun"
        spec = {"name": name, "bench": name, "backend": backend}
        if name in SHARED_HARNESS_TARGETS:
            # Only copy benchmark files. src/, migrations, and Cargo.toml stay
            # at the base revision, so this still measures the old application.
            paths = [f"benches/{name}.rs", "benches/support/mod.rs"]
            restore = shlex.join(
                ["git", "restore", f"--source={head_sha}", "--worktree", "--", *paths]
            )
            args = shlex.join(shlex.split(criterion_args))
            # The pinned action removes empty quoted arguments while normalizing
            # commands. Keep the empty default feature set attached to its flag.
            spec["base_command"] = (
                f"{restore} && cargo bench --bench {shlex.quote(name)} "
                "--features='{features}' {no_default_features_flag} "
                f"-- {args}"
            )
        specs.append(spec)
    return specs


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("head_sha")
    parser.add_argument("--criterion-args", required=True)
    args = parser.parse_args()
    manifest = tomllib.loads(Path("Cargo.toml").read_text())
    print(json.dumps(benchmark_specs(manifest, args.head_sha, args.criterion_args)))


if __name__ == "__main__":
    main()
