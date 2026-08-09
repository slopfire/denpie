#!/usr/bin/env python3
"""Enforce Denpie API v1's additive-only wire and operation contract."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
PROTO = ROOT / "proto" / "denpie.proto"
OPERATIONS = ROOT / "api" / "operations-v1.json"
BASELINE = ROOT / "api" / "contract-v1.json"

TOKEN = re.compile(
    r'"(?:\\.|[^"\\])*"|[A-Za-z_][A-Za-z0-9_.]*|-?\d+|[{}\[\]()=;,<>]'
)


class ProtoParser:
    def __init__(self, source: str):
        source = re.sub(r"/\*.*?\*/", "", source, flags=re.DOTALL)
        source = re.sub(r"//[^\n]*", "", source)
        self.tokens = TOKEN.findall(source)
        self.index = 0
        self.messages: dict[str, Any] = {}
        self.enums: dict[str, Any] = {}

    def peek(self) -> str | None:
        return self.tokens[self.index] if self.index < len(self.tokens) else None

    def take(self, expected: str | None = None) -> str:
        token = self.peek()
        if token is None:
            raise ValueError("unexpected end of protobuf schema")
        if expected is not None and token != expected:
            raise ValueError(f"expected {expected!r}, found {token!r}")
        self.index += 1
        return token

    def skip_statement(self) -> None:
        while self.take() != ";":
            pass

    def parse(self) -> dict[str, Any]:
        syntax = None
        package = None
        while self.peek() is not None:
            token = self.take()
            if token == "syntax":
                self.take("=")
                syntax = json.loads(self.take())
                self.take(";")
            elif token == "package":
                package = self.take()
                self.take(";")
            elif token == "message":
                self.parse_message("")
            elif token == "enum":
                self.parse_enum("")
            elif token == "option":
                self.skip_statement()
            elif token == "import":
                raise ValueError("public denpie.proto must remain self-contained; imports are forbidden")
            else:
                raise ValueError(f"unsupported top-level protobuf declaration {token!r}")
        if syntax != "proto3" or package != "denpie":
            raise ValueError("denpie.proto must keep syntax proto3 and package denpie")
        return {
            "syntax": syntax,
            "package": package,
            "messages": self.messages,
            "enums": self.enums,
        }

    def qualified(self, parent: str, name: str) -> str:
        return f"{parent}.{name}" if parent else name

    def parse_message(self, parent: str) -> None:
        name = self.qualified(parent, self.take())
        self.take("{")
        fields: dict[str, Any] = {}
        reserved: list[str] = []
        while self.peek() != "}":
            token = self.peek()
            if token == "message":
                self.take()
                self.parse_message(name)
            elif token == "enum":
                self.take()
                self.parse_enum(name)
            elif token == "oneof":
                self.take()
                self.parse_oneof(fields)
            elif token == "reserved":
                self.take()
                reserved.append(self.parse_reserved())
            elif token == "option":
                self.take()
                self.skip_statement()
            else:
                field_name, field = self.parse_field(None)
                if field_name in fields:
                    raise ValueError(f"duplicate field {name}.{field_name}")
                fields[field_name] = field
        self.take("}")
        self.messages[name] = {
            "fields": fields,
            "reserved": sorted(reserved),
        }

    def parse_oneof(self, fields: dict[str, Any]) -> None:
        oneof = self.take()
        self.take("{")
        while self.peek() != "}":
            if self.peek() == "option":
                self.take()
                self.skip_statement()
                continue
            field_name, field = self.parse_field(oneof)
            if field_name in fields:
                raise ValueError(f"duplicate oneof field {field_name}")
            fields[field_name] = field
        self.take("}")

    def parse_type(self) -> str:
        field_type = self.take()
        if field_type != "map":
            return field_type
        self.take("<")
        key_type = self.take()
        self.take(",")
        value_type = self.take()
        self.take(">")
        return f"map<{key_type},{value_type}>"

    def parse_field(self, oneof: str | None) -> tuple[str, dict[str, Any]]:
        cardinality = "singular"
        if self.peek() in {"optional", "repeated"}:
            cardinality = self.take()
        if oneof is not None and cardinality != "singular":
            raise ValueError("oneof fields cannot be optional or repeated")
        field_type = self.parse_type()
        field_name = self.take()
        self.take("=")
        number = int(self.take())
        if self.peek() == "[":
            depth = 0
            while True:
                token = self.take()
                depth += token == "["
                depth -= token == "]"
                if depth == 0:
                    break
        self.take(";")
        return field_name, {
            "number": number,
            "type": field_type,
            "cardinality": cardinality,
            "oneof": oneof,
        }

    def parse_reserved(self) -> str:
        tokens = []
        while self.peek() != ";":
            tokens.append(self.take())
        self.take(";")
        return " ".join(tokens)

    def parse_enum(self, parent: str) -> None:
        name = self.qualified(parent, self.take())
        self.take("{")
        values: dict[str, int] = {}
        reserved: list[str] = []
        while self.peek() != "}":
            if self.peek() == "reserved":
                self.take()
                reserved.append(self.parse_reserved())
                continue
            if self.peek() == "option":
                self.take()
                self.skip_statement()
                continue
            value_name = self.take()
            self.take("=")
            number = int(self.take())
            if self.peek() == "[":
                depth = 0
                while True:
                    token = self.take()
                    depth += token == "["
                    depth -= token == "]"
                    if depth == 0:
                        break
            self.take(";")
            values[value_name] = number
        self.take("}")
        self.enums[name] = {"values": values, "reserved": sorted(reserved)}


def operation_contract(source: dict[str, Any]) -> dict[str, Any]:
    operations = source.get("operations")
    if not isinstance(operations, list):
        raise ValueError("api/operations-v1.json must contain an operations list")
    result_fields = source.get("result_fields")
    if not isinstance(result_fields, dict):
        raise ValueError("api/operations-v1.json must contain a result_fields map")
    immutable = ("request", "result", "auth", "scope", "kind")
    result = {}
    for operation in operations:
        name = operation.get("operation")
        if not isinstance(name, str) or name in result:
            raise ValueError(f"invalid or duplicate operation name: {name!r}")
        result[name] = {
            **{key: operation.get(key) for key in immutable},
            "result_field": result_fields.get(name),
        }
    if set(result_fields) != set(result):
        raise ValueError("result_fields must cover every operation exactly")
    return result


def current_contract() -> dict[str, Any]:
    wire = ProtoParser(PROTO.read_text()).parse()
    operations = operation_contract(json.loads(OPERATIONS.read_text()))
    return {"api_version": "v1", "wire": wire, "operations": operations}


def compatibility_errors(baseline: dict[str, Any], current: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    for key in ("api_version",):
        if baseline.get(key) != current.get(key):
            errors.append(f"{key} changed from {baseline.get(key)!r} to {current.get(key)!r}")

    old_wire = baseline.get("wire", {})
    new_wire = current.get("wire", {})
    for key in ("syntax", "package"):
        if old_wire.get(key) != new_wire.get(key):
            errors.append(f"protobuf {key} changed")

    for category in ("messages", "enums"):
        old_items = old_wire.get(category, {})
        new_items = new_wire.get(category, {})
        for name, old_item in old_items.items():
            new_item = new_items.get(name)
            if new_item is None:
                errors.append(f"removed {category[:-1]} {name}")
                continue
            member_key = "fields" if category == "messages" else "values"
            for member, old_value in old_item.get(member_key, {}).items():
                new_value = new_item.get(member_key, {}).get(member)
                if new_value is None:
                    errors.append(f"removed {name}.{member}")
                elif new_value != old_value:
                    errors.append(
                        f"changed {name}.{member}: {old_value!r} -> {new_value!r}"
                    )
            old_reserved = set(old_item.get("reserved", []))
            new_reserved = set(new_item.get("reserved", []))
            for reservation in sorted(old_reserved - new_reserved):
                errors.append(f"removed reservation {name}: {reservation}")

    old_operations = baseline.get("operations", {})
    new_operations = current.get("operations", {})
    for name, old_policy in old_operations.items():
        new_policy = new_operations.get(name)
        if new_policy is None:
            errors.append(f"removed API operation {name}")
        elif new_policy != old_policy:
            errors.append(
                f"changed API operation {name}: {old_policy!r} -> {new_policy!r}"
            )
    return errors


def canonical(value: dict[str, Any]) -> str:
    return json.dumps(value, indent=2, sort_keys=True) + "\n"


def baseline_from_git(revision: str) -> dict[str, Any] | None:
    verified = subprocess.run(
        ["git", "rev-parse", "--verify", f"{revision}^{{commit}}"],
        cwd=ROOT,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    if verified.returncode != 0:
        raise ValueError(f"Git contract baseline revision does not exist: {revision}")
    result = subprocess.run(
        ["git", "show", f"{revision}:api/contract-v1.json"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        return None
    return json.loads(result.stdout)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--update",
        action="store_true",
        help="record additive contract members after compatibility validation",
    )
    parser.add_argument(
        "--against-git",
        default="HEAD",
        metavar="REVISION",
        help="also prove the ledger is additive relative to this Git revision",
    )
    args = parser.parse_args()
    current = current_contract()

    if not BASELINE.exists():
        if not args.update:
            print(f"missing {BASELINE.relative_to(ROOT)}", file=sys.stderr)
            return 1
        BASELINE.write_text(canonical(current))
        print(f"created {BASELINE.relative_to(ROOT)}")
        return 0

    baseline = json.loads(BASELINE.read_text())
    historical = baseline_from_git(args.against_git)
    if historical is not None:
        history_errors = compatibility_errors(historical, baseline)
        if history_errors:
            print(
                f"API v1 ledger rewrites history from {args.against_git}:",
                file=sys.stderr,
            )
            for error in history_errors:
                print(f"  - {error}", file=sys.stderr)
            print(
                "Restore the historical v1 entries and create a new API major for breaking changes.",
                file=sys.stderr,
            )
            return 1
    errors = compatibility_errors(baseline, current)
    if errors:
        print("API v1 contains breaking contract changes:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        print(
            "Breaking changes require a new API major version; do not refresh the v1 ledger.",
            file=sys.stderr,
        )
        return 1

    if args.update:
        BASELINE.write_text(canonical(current))
        print(f"recorded additive changes in {BASELINE.relative_to(ROOT)}")
        return 0

    expected = canonical(current)
    if BASELINE.read_text() != expected:
        print(
            "API v1 has compatible additions that are not recorded; "
            "run `just api-contract-update` and commit the ledger diff.",
            file=sys.stderr,
        )
        return 1

    messages = len(current["wire"]["messages"])
    enums = len(current["wire"]["enums"])
    operations = len(current["operations"])
    print(
        f"API v1 contract is additive-only and current "
        f"({messages} messages, {enums} enums, {operations} operations)"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, json.JSONDecodeError) as error:
        print(f"API contract check failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
