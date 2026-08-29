#!/usr/bin/env python3
"""Generate and validate the API v1 operation reference."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "api" / "operations-v1.json"
PROTO = ROOT / "proto" / "denpie.proto"
OUTPUT = ROOT / "docs" / "api-v1-reference.md"
TRANSPORT = ROOT / "src" / "api" / "transport.rs"


def block_body(source: str, declaration: str) -> str:
    match = re.search(rf"\b{declaration}\s*\{{", source)
    if not match:
        raise ValueError(f"{declaration} not found")
    opening = source.index("{", match.start())
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[opening + 1 : index]
    raise ValueError(f"unterminated {declaration}")


def oneof_fields(source: str, message: str, oneof: str) -> dict[str, str]:
    message_body = block_body(source, rf"message\s+{re.escape(message)}")
    oneof_body = block_body(message_body, rf"oneof\s+{re.escape(oneof)}")
    return {
        name: field_type
        for field_type, name in re.findall(
            r"^\s*([A-Za-z][A-Za-z0-9_]*)\s+([a-z][a-z0-9_]*)\s*=\s*\d+\s*;",
            oneof_body,
            re.MULTILINE,
        )
    }


def validate(manifest: dict[str, object], proto_source: str) -> list[dict[str, str]]:
    operations = manifest.get("operations")
    if not isinstance(operations, list):
        raise ValueError("manifest operations must be a list")
    result_fields = manifest.get("result_fields")
    if not isinstance(result_fields, dict) or not all(
        isinstance(key, str) and isinstance(value, str)
        for key, value in result_fields.items()
    ):
        raise ValueError("manifest result_fields must be a string map")

    required = {"operation", "request", "result", "auth", "scope", "kind", "summary"}
    normalized: list[dict[str, str]] = []
    for index, raw in enumerate(operations):
        if not isinstance(raw, dict) or set(raw) != required:
            raise ValueError(
                f"operation {index} must contain exactly {sorted(required)}"
            )
        if not all(isinstance(value, str) and value for value in raw.values()):
            raise ValueError(f"operation {index} has an empty or non-string value")
        normalized.append(raw)

    proto_operations = oneof_fields(proto_source, "ApiRequest", "op")
    manifest_operations = {item["operation"]: item["request"] for item in normalized}
    if manifest_operations != proto_operations:
        missing = sorted(set(proto_operations) - set(manifest_operations))
        extra = sorted(set(manifest_operations) - set(proto_operations))
        mismatched = sorted(
            name
            for name in set(proto_operations) & set(manifest_operations)
            if proto_operations[name] != manifest_operations[name]
        )
        raise ValueError(
            "operation manifest differs from ApiRequest.op: "
            f"missing={missing}, extra={extra}, request_type_mismatch={mismatched}"
        )

    if set(result_fields) != set(manifest_operations):
        missing = sorted(set(manifest_operations) - set(result_fields))
        extra = sorted(set(result_fields) - set(manifest_operations))
        raise ValueError(
            f"result_fields must cover every operation exactly: missing={missing}, extra={extra}"
        )

    response_fields = oneof_fields(proto_source, "ApiResponse", "result")
    response_types = set(response_fields.values())
    invalid_results = sorted(
        {item["result"] for item in normalized} - response_types
    )
    if invalid_results:
        raise ValueError(f"results absent from ApiResponse.result: {invalid_results}")
    mismatched_results = sorted(
        item["operation"]
        for item in normalized
        if response_fields.get(result_fields[item["operation"]]) != item["result"]
    )
    if mismatched_results:
        raise ValueError(
            "operation result field/message mismatch: " + ", ".join(mismatched_results)
        )
    normalized = [
        {**item, "result_field": result_fields[item["operation"]]}
        for item in normalized
    ]
    validate_transport_policy(normalized, TRANSPORT.read_text())
    return normalized


def rust_variant(operation: str) -> str:
    return "".join(part.capitalize() for part in operation.split("_"))


def arm_value(function_body: str, variant: str, value_pattern: str) -> str:
    match = re.search(
        rf"api_request::Op::{re.escape(variant)}\(_\).*?=>\s*{value_pattern}",
        function_body,
        re.DOTALL,
    )
    if not match:
        raise ValueError(f"could not read transport policy for {variant}")
    return match.group(1)


def validate_transport_policy(
    operations: list[dict[str, str]], transport_source: str
) -> None:
    mutation_body = block_body(transport_source, r"fn\s+mutation_policy[^\{]*")
    scope_body = block_body(transport_source, r"fn\s+required_scope[^\{]*")
    rust_kinds = {
        "ReadOnly": "read",
        "Replayable": "mutation",
        "OneTimeSecret": "one-time secret mutation",
    }
    mismatches = []
    for item in operations:
        variant = rust_variant(item["operation"])
        policy = arm_value(mutation_body, variant, r"MutationPolicy::(\w+)")
        scope = arm_value(scope_body, variant, r'(?:\{\s*)?"([^"]+)"')
        expected_scope = "*" if item["scope"] == "none" else item["scope"]
        if rust_kinds.get(policy) != item["kind"] or scope != expected_scope:
            mismatches.append(
                f"{item['operation']} (manifest {item['kind']}/{item['scope']}, "
                f"Rust {rust_kinds.get(policy, policy)}/{scope})"
            )
    if mismatches:
        raise ValueError("transport policy mismatch: " + "; ".join(mismatches))


def cell(value: str) -> str:
    return value.replace("|", "\\|").replace("\n", " ")


def render(version: str, operations: list[dict[str, str]]) -> str:
    reads = sum(item["kind"] == "read" for item in operations)
    mutations = len(operations) - reads
    rows = []
    for item in operations:
        idempotency = {
            "read": "not required",
            "mutation": "required; replayable",
            "one-time secret mutation": "required; success is not replayable",
        }[item["kind"]]
        rows.append(
            "| `{operation}` | `{request}` | `{result_field}` (`{result}`) | {auth} | `{scope}` | "
            "{kind} | {idempotency} | {summary} |".format(
                **{key: cell(value) for key, value in item.items()},
                idempotency=idempotency,
            )
        )

    return f"""# API {version} operation reference

This file is generated from [`api/operations-v1.json`](../api/operations-v1.json)
and checked against `ApiRequest.op` and `ApiResponse.result` in
[`proto/denpie.proto`](../proto/denpie.proto). Do not edit it by hand; run
`just api-reference` after changing the manifest or protobuf operation set.

The API currently exposes **{len(operations)} operations**: **{reads} reads** and
**{mutations} mutations**. Every mutation requires an idempotency key. See the
[API v1 guide](api-v1.md) for transport behavior and the
[field semantics](api-v1-semantics.md) for representation details.

| Operation | Request message | Success result | Authentication | Required scope | Kind | Idempotency | Purpose |
|---|---|---|---|---|---|---|---|
{chr(10).join(rows)}

## Interpreting the table

- `scope: none` is limited to discovery and first-key bootstrap. Every other
  operation authenticates the key first, then authorizes the listed scope.
- A full-access `*` key satisfies every scope. `secrets:read` is an additional
  disclosure gate for credential fields returned by `get_settings`.
- A replayable mutation returns the stored HTTP status and protobuf response
  for the same credential, idempotency key, and exact operation payload.
- Successful key creation is deliberately not replayable because Denpie never
  persists raw generated credentials in the idempotency store.
- `explore_link`, `test_vision_model`, and `enhance_prompt_template` are
  classified as reads for transport idempotency even though they may perform
  external I/O.
"""


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--check", action="store_true", help="fail if the generated file is stale"
    )
    args = parser.parse_args()

    manifest = json.loads(MANIFEST.read_text())
    operations = validate(manifest, PROTO.read_text())
    generated = render(str(manifest["api_version"]), operations)

    if args.check:
        actual = OUTPUT.read_text() if OUTPUT.exists() else ""
        if actual != generated:
            print(
                f"{OUTPUT.relative_to(ROOT)} is stale; run just api-reference",
                file=sys.stderr,
            )
            return 1
        print(f"checked {len(operations)} documented API operations")
        return 0

    OUTPUT.write_text(generated)
    print(f"wrote {OUTPUT.relative_to(ROOT)} with {len(operations)} operations")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
