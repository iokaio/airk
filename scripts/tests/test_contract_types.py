# SPDX-License-Identifier: Apache-2.0
"""The contract type renderer must render the supported subset and refuse the rest."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("contract_types", ROOT / "scripts" / "dev" / "contract_types.py")
TYPES = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TYPES)


class Render(unittest.TestCase):
    def test_scalars_unions_and_objects(self):
        self.assertEqual(TYPES.render({"type": "string"}), "string")
        self.assertEqual(TYPES.render({"type": "integer", "format": "uint64", "minimum": 0}), "number")
        self.assertEqual(TYPES.render({"type": ["string", "null"]}), "string | null")
        self.assertEqual(TYPES.render({"enum": ["a", "b"]}), '"a" | "b"')
        self.assertEqual(TYPES.render({"type": "string", "const": "hello"}), '"hello"')
        self.assertEqual(TYPES.render({"$ref": "#/$defs/Name"}), "Name")
        self.assertEqual(TYPES.render({"type": "array", "items": {"type": "boolean"}}), "Array<boolean>")
        self.assertEqual(TYPES.render({"description": "opaque"}), "unknown")
        rendered = TYPES.render({"type": "object", "additionalProperties": False,
                                 "properties": {"a": {"type": "string"}, "b": {"type": "integer"}},
                                 "required": ["a"]})
        self.assertIn("a: string;", rendered)
        self.assertIn("b?: number;", rendered)

    def test_unsupported_constructs_fail(self):
        for schema in ({"type": "object", "additionalProperties": True, "properties": {"known": {"type": "string"}}},
                       {"type": "object", "additionalProperties": True, "required": ["known"]},
                       {"type": "object", "additionalProperties": {"type": "string"}},
                       {"allOf": [{"type": "string"}]},
                       {"$ref": "https://example.invalid/x"},
                       {"type": "tuple"}):
            with self.assertRaises(TYPES.Unsupported, msg=json.dumps(schema)):
                TYPES.render(schema)

    def test_native_pure_json_maps_and_literal_task_keys_keep_exact_types(self):
        self.assertEqual(TYPES.render({"type": "object", "additionalProperties": True}), "Record<string, unknown>")
        key = "airk.context_next_read.v1"
        rendered = TYPES.render({"type": "object", "additionalProperties": False,
            "properties": {key: {"type": "object", "additionalProperties": True}}, "required": [key]})
        self.assertIn('"airk.context_next_read.v1": Record<string, unknown>;', rendered)
        TYPES.declaration_only('export const ARBITER_CONTRACT = 1; export type Policy = ' + rendered + ';', "arbiter")
        hostile = 'x"; console.log("never"); //'
        escaped = TYPES.render({"type": "object", "additionalProperties": False, "properties": {hostile: {"type": "string"}}})
        self.assertIn(json.dumps(hostile) + '?: string;', escaped)
        TYPES.declaration_only('export const I3_CONTRACT = 1; export type Keys = ' + escaped + ';', "i3")

    def test_check_detects_drift_and_write_repairs_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            schema = root / "x.schema.json"
            schema.write_text(json.dumps({"title": "fixture", "i2_contract": 1, "roots": ["A"],
                                          "$defs": {"A": {"enum": ["one"]}}}), encoding="utf-8")
            with mock.patch.dict(TYPES.CONTRACTS, {"i2": schema}), mock.patch.object(TYPES, "ROOT", root):
                self.assertEqual(TYPES.main(["i2", "--check"]), 1)
                self.assertEqual(TYPES.main(["i2", "--write"]), 0)
                self.assertEqual(TYPES.main(["i2", "--check"]), 0)
                self.assertIn('export type A = "one";', (root / "x.ts").read_text(encoding="utf-8"))

    def test_each_contract_names_its_own_version_constant(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            schema = root / "i3.schema.json"
            schema.write_text(json.dumps({"title": "fixture", "i3_contract": 1, "roots": ["A"],
                                          "$defs": {"A": {"type": "object", "additionalProperties": False,
                                                          "properties": {"ok": {"const": True}},
                                                          "required": ["ok"]}}}), encoding="utf-8")
            with mock.patch.dict(TYPES.CONTRACTS, {"i3": schema}), mock.patch.object(TYPES, "ROOT", root):
                rendered = TYPES.generate("i3")
        self.assertIn("export const I3_CONTRACT = 1;", rendered)
        self.assertNotIn("I2_CONTRACT", rendered)
        self.assertIn("ok: true;", rendered)
        self.assertEqual(TYPES.CONTRACTS["i3"], ROOT / "packages/contracts-gen/i3.schema.json")

    def test_checked_in_i2_types_match_their_schema(self):
        self.assertEqual(TYPES.main(["i2", "--check"]), 0)

    def test_checked_in_i3_types_match_their_schema(self):
        self.assertEqual(TYPES.main(["i3", "--check"]), 0)

    def test_editor_declarations_are_exact_canonical_bytes_and_checked(self):
        for contract in ("i3", "arbiter"):
            with self.subTest(contract=contract), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                schema = root / "fixture.schema.json"
                schema.write_text(json.dumps({"title": "fixture", contract + "_contract": 1,
                    "$defs": {"A": {"type": "array", "items": {"type": ["string", "null"]}}}}), encoding="utf-8")
                with mock.patch.dict(TYPES.CONTRACTS, {contract: schema}), mock.patch.object(TYPES, "ROOT", root):
                    self.assertEqual(TYPES.main([contract, "--write"]), 0)
                    canonical = root / "fixture.ts"
                    alias = root / f"packages/vscode/src/generated/{contract}.d.ts"
                    self.assertEqual(alias.read_bytes(), canonical.read_bytes())
                    self.assertEqual(TYPES.main([contract, "--check"]), 0)
                    alias.write_bytes(alias.read_bytes() + b"\n")
                    self.assertEqual(TYPES.main([contract, "--check"]), 1)
                    alias.unlink()
                    self.assertEqual(TYPES.main([contract, "--check"]), 1)

    def test_declaration_guard_refuses_runtime_even_inside_documentation_escape(self):
        valid = '/* doc */ export const I3_CONTRACT = 1; export type A = { x?: Array<string | null>; };'
        TYPES.declaration_only(valid, "i3")
        for suffix in (' export function run() {}', ' export const evil = 2;',
                       ' console.log("run");', ' export type B = typeof run();'):
            with self.subTest(suffix=suffix), self.assertRaises(TYPES.Unsupported):
                TYPES.declaration_only(valid + suffix, "i3")
        with self.assertRaises(TYPES.Unsupported):
            TYPES.declaration_only('export const I3_CONTRACT = 1; /** */ console.log("run"); /* */', "i3")


if __name__ == "__main__":
    unittest.main()
