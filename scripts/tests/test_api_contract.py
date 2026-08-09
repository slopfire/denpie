from __future__ import annotations

import copy
import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "check_api_contract", ROOT / "scripts" / "check-api-contract.py"
)
assert SPEC is not None and SPEC.loader is not None
contract = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(contract)


SCHEMA = """
syntax = "proto3";
package denpie;

message Request {
  string request_id = 1;
  optional string note = 2;
  oneof op {
    Empty ping = 3;
  }
  reserved 4;
}

message Empty {}

enum State {
  STATE_UNSPECIFIED = 0;
  STATE_READY = 1;
}
"""


def sample_contract():
    return {
        "api_version": "v1",
        "wire": contract.ProtoParser(SCHEMA).parse(),
        "operations": {
            "ping": {
                "request": "Empty",
                "result": "Empty",
                "auth": "none",
                "scope": "none",
                "kind": "read",
            }
        },
    }


class ApiContractTests(unittest.TestCase):
    def test_parser_preserves_presence_oneof_and_reservations(self):
        parsed = sample_contract()["wire"]
        fields = parsed["messages"]["Request"]["fields"]
        self.assertEqual(fields["note"]["cardinality"], "optional")
        self.assertEqual(fields["ping"]["oneof"], "op")
        self.assertEqual(parsed["messages"]["Request"]["reserved"], ["4"])

    def test_additions_are_compatible_but_change_the_ledger(self):
        baseline = sample_contract()
        current = copy.deepcopy(baseline)
        current["wire"]["messages"]["Request"]["fields"]["new_field"] = {
            "number": 5,
            "type": "string",
            "cardinality": "singular",
            "oneof": None,
        }
        self.assertEqual(contract.compatibility_errors(baseline, current), [])
        self.assertNotEqual(contract.canonical(baseline), contract.canonical(current))

    def test_field_removal_and_type_change_are_breaking(self):
        baseline = sample_contract()
        removed = copy.deepcopy(baseline)
        del removed["wire"]["messages"]["Request"]["fields"]["request_id"]
        self.assertIn(
            "removed Request.request_id",
            contract.compatibility_errors(baseline, removed),
        )

        changed = copy.deepcopy(baseline)
        changed["wire"]["messages"]["Request"]["fields"]["request_id"]["type"] = "bytes"
        errors = contract.compatibility_errors(baseline, changed)
        self.assertTrue(any(error.startswith("changed Request.request_id") for error in errors))

    def test_operation_scope_and_kind_are_immutable(self):
        baseline = sample_contract()
        current = copy.deepcopy(baseline)
        current["operations"]["ping"]["scope"] = "diagnostics:run"
        current["operations"]["ping"]["kind"] = "mutation"
        errors = contract.compatibility_errors(baseline, current)
        self.assertTrue(any(error.startswith("changed API operation ping") for error in errors))

    def test_reservations_cannot_be_removed(self):
        baseline = sample_contract()
        current = copy.deepcopy(baseline)
        current["wire"]["messages"]["Request"]["reserved"] = []
        self.assertIn(
            "removed reservation Request: 4",
            contract.compatibility_errors(baseline, current),
        )


if __name__ == "__main__":
    unittest.main()
