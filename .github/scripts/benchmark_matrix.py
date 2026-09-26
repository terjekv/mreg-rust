#!/usr/bin/env python3
"""Discover benchmark targets, compiling each harness against its own revision."""

import json
from pathlib import Path
import tomllib


def benchmark_specs(manifest: dict) -> list[dict]:
    specs = []
    for target in manifest["bench"]:
        name = target["name"]
        backend = "criterion" if name.endswith("_criterion") else "gungraun"
        spec = {"name": name, "bench": name, "backend": backend}
        specs.append(spec)
    return specs


def main() -> None:
    manifest = tomllib.loads(Path("Cargo.toml").read_text())
    print(json.dumps(benchmark_specs(manifest)))


if __name__ == "__main__":
    main()
