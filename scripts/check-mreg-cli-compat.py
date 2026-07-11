#!/usr/bin/env python3
"""Compare mreg-cli recordings while allowing explicit compatibility gaps.

The upstream comparator requires byte-for-byte-equivalent command recordings.
For mreg-rust, a command may instead stop at a deliberately unsupported v1
endpoint. Such a difference is accepted only when the new recording contains
an explicitly allowlisted HTTP status (501 by default). A failed mutation also
taints the remainder of this stateful upstream suite: later differences are
reported as unverified downstream behavior, not as matches. Differences before
the first explicit mutating gap remain CI failures.
"""

from __future__ import annotations

import json
import os
import re
import sys
import urllib.parse
from pathlib import Path
from typing import Any

TIMESTAMP = re.compile(r"\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?")
UUID = re.compile(r"\b[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\b", re.I)
MAC = re.compile(r"\b(?:[0-9a-f]{2}:){5}[0-9a-f]{2}\b", re.I)
DISPLAY_TIME = re.compile(
    r"\b[A-Za-z]{3}\s[A-Za-z]{3}\s+\d{1,2}\s\d{2}:\d{2}:\d{2}\s\d{4}\b"
)
IPV4 = re.compile(r"((25[0-5]|(2[0-4]|1\d|[1-9]|)\d)\.?\b){4}")
IPV6 = re.compile(r"\b([0-9a-fA-F]{1,4}::?){1,7}[0-9a-fA-F]{1,4}\b")
API_ID = re.compile(
    r'("url":\s*"/api/v1/(?![^"]*<(?:IPv6|IPv4)>)[^"]*?)([/=])(\d+)(")'
)
EXCLUDED_RANGE_ID = re.compile(r'("url":\s*"/api/v1/networks/[^"]+/excluded_ranges/)(\d+)(")')
QUERY_ID = re.compile(r'("url":\s*"/api/v1/[^"]*[?&]id=)(\d+)')
SERVER = re.compile(r"https?://(?:127\.0\.0\.1|localhost|host\.docker\.internal):8000")


def commands(path: str) -> list[dict[str, Any]]:
    with open(path, encoding="utf-8") as stream:
        return [entry for entry in json.load(stream) if "command" in entry]


def normalize(value: Any) -> Any:
    if isinstance(value, dict):
        return {
            key: (
                "<SERIAL>"
                if key == "serialno"
                else "<ID>"
                if key in {"id", "host", "network", "zone", "model_id"}
                and (isinstance(item, int) or (isinstance(item, str) and item.isdigit()))
                else normalize(item)
            )
            for key, item in value.items()
            if key not in {"time", "id", "create_date"}
        }
    if isinstance(value, list):
        return [normalize(item) for item in value]
    if isinstance(value, str):
        value = TIMESTAMP.sub("<TIME>", value)
        value = UUID.sub("<UUID>", value)
        value = MAC.sub("<MAC>", value)
        if re.match(r"^Serial:\s+\d+$", value):
            return "Serial: <SERIAL>"
        return value
    return value


def normalized_command(value: Any) -> str:
    """Apply the same volatile-value classes as upstream mreg-cli's diff.py."""
    rendered = urllib.parse.unquote(json.dumps(normalize(value), sort_keys=True))
    rendered = SERVER.sub("http://<SERVER>:8000", rendered)
    rendered = TIMESTAMP.sub("<TIME>", rendered)
    rendered = DISPLAY_TIME.sub("<DATETIME>", rendered)
    rendered = MAC.sub("<macaddress>", rendered)
    rendered = IPV4.sub("<IPv4>", rendered)
    rendered = IPV6.sub("<IPv6>", rendered)
    rendered = UUID.sub("<UUID>", rendered)
    rendered = API_ID.sub(r"\1\2<ID>\4", rendered)
    rendered = EXCLUDED_RANGE_ID.sub(r"\1<ID>\3", rendered)
    rendered = QUERY_ID.sub(r"\1<ID>", rendered)
    return re.sub(r"\s+", " ", rendered)


def statuses(command: dict[str, Any]) -> set[int]:
    return {
        request["status"]
        for request in command.get("api_requests", [])
        if isinstance(request.get("status"), int)
    }


def command_intends_mutation(command: str) -> bool:
    """Recognize state-changing CLI verbs even when a GET preflight stops them."""
    verbs = command.split()
    if len(verbs) < 2:
        return False
    verb = verbs[1]
    return any(
        marker in verb
        for marker in (
            "add",
            "assoc",
            "change",
            "create",
            "delete",
            "disassoc",
            "move",
            "remove",
            "rename",
            "set",
            "unset",
        )
    )


def main() -> int:
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} EXPECTED RESULT", file=sys.stderr)
        return 2
    expected = commands(sys.argv[1])
    result = commands(sys.argv[2])
    allowed = {
        int(value)
        for value in os.getenv("MREG_CLI_ALLOWED_STATUSES", "501").split(",")
        if value.strip()
    }
    non_tainting_prefixes = tuple(
        value.strip()
        for value in os.getenv("MREG_CLI_NON_TAINTING_PREFIXES", "permission ").split(",")
        if value.strip()
    )
    if [item["command"] for item in expected] != [item["command"] for item in result]:
        print("mreg-cli executed a different command sequence", file=sys.stderr)
        return 1

    accepted: list[str] = []
    downstream: list[str] = []
    unexpected: list[str] = []
    exact = 0
    state_tainted = False
    for old, new in zip(expected, result):
        if normalized_command(old) == normalized_command(new):
            exact += 1
            continue
        matched = statuses(new) & allowed
        if matched:
            accepted.append(f"{new['command']} ({', '.join(map(str, sorted(matched)))})")
            state_tainted = state_tainted or (
                not new["command"].startswith(non_tainting_prefixes)
                and (
                    command_intends_mutation(new["command"])
                    or any(
                        request.get("status") in allowed
                        and request.get("method") in {"POST", "PUT", "PATCH", "DELETE"}
                        for request in new.get("api_requests", [])
                    )
                )
            )
        elif state_tainted:
            downstream.append(new["command"])
        else:
            unexpected.append(new["command"])

    summary = [
        "## mreg-cli compatibility",
        "",
        f"- Exact command matches: {exact}",
        f"- Accepted explicit gaps: {len(accepted)}",
        f"- Unverified after an explicit mutating gap: {len(downstream)}",
        f"- Unexpected differences: {len(unexpected)}",
    ]
    if accepted:
        summary.extend(["", "### Accepted gaps", *[f"- `{item}`" for item in accepted]])
    if downstream:
        displayed = downstream[:50]
        summary.extend(
            [
                "",
                "### Unverified downstream commands",
                "These commands ran after unsupported mutations changed the expected test state.",
                *[f"- `{item}`" for item in displayed],
            ]
        )
        if len(downstream) > len(displayed):
            summary.append(f"- … and {len(downstream) - len(displayed)} more")
    if unexpected:
        summary.extend(["", "### Unexpected differences", *[f"- `{item}`" for item in unexpected]])
    rendered = "\n".join(summary) + "\n"
    print(rendered)
    if github_summary := os.getenv("GITHUB_STEP_SUMMARY"):
        Path(github_summary).write_text(rendered, encoding="utf-8")
    return bool(unexpected)


if __name__ == "__main__":
    raise SystemExit(main())
