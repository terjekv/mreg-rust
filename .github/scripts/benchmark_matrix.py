#!/usr/bin/env python3
"""Discover benchmark targets and apply the one-time wildcard fixture repair."""

import argparse
import json
from pathlib import Path
import re
import shlex
import tomllib

# Apply the bounded fixture to both sides of this migration only. Subsequent
# PRs use their own base harness, so future API changes cannot break old code.
UNBOUNDED_WILDCARD_BASE = "fbcf2d82621993440cab994196dc355eb41ba777"


def benchmark_specs(
    manifest: dict, base_sha: str, head_sha: str, criterion_args: str
) -> list[dict]:
    if not all(re.fullmatch(r"[0-9a-f]{40}", sha) for sha in (base_sha, head_sha)):
        raise ValueError("base_sha and head_sha must be full Git commit SHAs")
    specs = []
    for target in manifest["bench"]:
        name = target["name"]
        backend = "criterion" if name.endswith("_criterion") else "gungraun"
        spec = {"name": name, "bench": name, "backend": backend}
        if (
            base_sha == UNBOUNDED_WILDCARD_BASE
            and name == "wildcard_zone_match_criterion"
        ):
            restore = shlex.join(
                [
                    "git", "restore", f"--source={head_sha}", "--worktree", "--",
                    f"benches/{name}.rs",
                ]
            )
            args = shlex.join(shlex.split(criterion_args))
            spec["base_command"] = (
                f"{restore} && cargo bench --bench {name} "
                "--features='{features}' {no_default_features_flag} "
                f"-- {args}"
            )
        specs.append(spec)
    return specs


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("base_sha")
    parser.add_argument("head_sha")
    parser.add_argument("--criterion-args", required=True)
    args = parser.parse_args()
    manifest = tomllib.loads(Path("Cargo.toml").read_text())
    print(json.dumps(
        benchmark_specs(manifest, args.base_sha, args.head_sha, args.criterion_args)
    ))


if __name__ == "__main__":
    main()
