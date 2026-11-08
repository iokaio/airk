# SPDX-License-Identifier: Apache-2.0
import importlib.util
import copy
from contextlib import nullcontext
import json
import os
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("standalone_adoption", Path(__file__).parents[1] / "check-standalone-adoption.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class StandaloneAdoptionTests(unittest.TestCase):
    def test_both_native_input_types_use_existing_http_root_and_preserve_original_config(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory).resolve(strict=True)
            control = base / "state" / "control"
            private = control / "http-1"
            private.mkdir(parents=True, mode=0o700)
            if os.name != "nt":
                private.chmod(0o700)
            original = base / "original-config.json"
            original.write_text('{"original":"stdio declaration"}')
            before = CHECK.sha(original)
            payloads = {"locators":[{"path":str(original),"format":"mcp_servers_json","server":"airk"}],
                        "registration":{"root":"original","data_root":"retained","collection":"saved"}}
            for purpose, value in payloads.items():
                created = CHECK.write_native_input(control, purpose, value)
                self.assertTrue(created.parent.samefile(private))
                self.assertTrue(created.name.startswith("fixture-standalone-" + purpose + "-"))
                self.assertEqual(json.loads(created.read_text()), value)
                self.assertEqual(created.stat().st_nlink, 1)
                if os.name != "nt":
                    self.assertEqual(created.stat().st_mode & 0o777, 0o600)
            self.assertEqual(CHECK.sha(original), before)
            self.assertFalse((base / "locators.json").exists())
            self.assertFalse((base / "original-registration.json").exists())

    def test_native_input_refuses_missing_parent_collision_and_unbounded_metadata(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory).resolve(strict=True)
            control = base / "control"
            control.mkdir()
            with self.assertRaises(FileNotFoundError):
                CHECK.write_native_input(control, "locators", [])
            self.assertEqual(list(control.iterdir()), [])
            private = control / "http-1"
            private.mkdir(mode=0o700)
            if os.name != "nt":
                private.chmod(0o700)
            collision = private / "fixture-standalone-locators-fixed.json"
            collision.write_bytes(b"preserve unknown existing bytes")
            with patch.object(CHECK.uuid, "uuid4", return_value=SimpleNamespace(hex="fixed")):
                with self.assertRaises(FileExistsError):
                    CHECK.write_native_input(control, "locators", [])
            self.assertEqual(collision.read_bytes(), b"preserve unknown existing bytes")
            with self.assertRaises(ValueError):
                CHECK.write_native_input(control, "../outside", [])
            with self.assertRaises(ValueError):
                CHECK.write_native_input(control, "registration", "x" * (1024 * 1024))
            self.assertEqual(list(private.iterdir()), [collision])

    @unittest.skipIf(os.name == "nt", "Unix permission-boundary check; native Windows ACL gate remains authoritative")
    def test_native_input_does_not_repair_an_unprotected_parent(self):
        with tempfile.TemporaryDirectory() as directory:
            control = Path(directory).resolve(strict=True) / "control"
            private = control / "http-1"
            private.mkdir(parents=True)
            private.chmod(0o755)
            with self.assertRaisesRegex(ValueError, "not private"):
                CHECK.write_native_input(control, "locators", [])
            self.assertEqual(private.stat().st_mode & 0o777, 0o755)
            self.assertEqual(list(private.iterdir()), [])

    def test_batch_requires_one_group_and_both_exact_configuration_claims(self):
        configs = [{"locator":{"path":"codex.toml"},"sha256":"config-a"},
                   {"locator":{"path":"claude.json"},"sha256":"config-b"}]
        mappings = [{"locator":item["locator"],"config_sha256":item["sha256"],
                     "origin_sha256":str(index),"group_operation_id":"group",
                     "registration":{"binding":"new"},"original_registration":{"binding":"old"},
                     "claim":{"origin_sha256":str(index),"config_sha256":item["sha256"],"full_saved_claim":"retained"}}
                    for index, item in enumerate(configs)]
        group = {"status":"ready","operation_id":"group","batch_operation_id":"batch",
                 "registration":{"binding":"new"},"original_registration":{"binding":"old"},
                 "claims":mappings,"grant":{"grant":{"memberships":[{"membership":{"index_id":"one-index"}}]},
                 "provenance":{"kind":"imported_standalone_adoption","adoption":{"configuration_claims":[row["claim"] for row in mappings]}}},
                 "adoption_receipt":{"setup":{"index_id":"one-index"}}}
        batch = {"status":"ready","operation_id":"batch","groups":[group],"claims":mappings}
        self.assertEqual(CHECK.verify_batch(batch, configs), group)
        split = copy.deepcopy(batch)
        split["groups"].append(copy.deepcopy(group))
        with self.assertRaises(AssertionError):
            CHECK.verify_batch(split, configs)
        dropped = copy.deepcopy(batch)
        dropped["groups"][0]["grant"]["provenance"]["adoption"]["configuration_claims"].pop()
        with self.assertRaises(AssertionError):
            CHECK.verify_batch(dropped, configs)
        changed = copy.deepcopy(batch)
        changed["claims"] = copy.deepcopy(changed["claims"])
        changed["claims"][0]["claim"]["full_saved_claim"] = "different provenance"
        with self.assertRaises(AssertionError):
            CHECK.verify_batch(changed, configs)

    def test_first_case_failure_retains_stage_receipt_in_output(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            runtime, owned = base / "runtime", base / "owned"
            runtime.mkdir()
            owned.mkdir()
            (runtime / "airk-install.json").write_text("fixture identity only; never executed")
            model, inputs, output = base / "model.js", base / "inputs.json", base / "output.json"
            model.write_text("fixture renderer identity only")
            inputs.write_text(json.dumps({"candidates":[{"tag":"old","outcome":"created"}]}))
            def failed_qualification(_runtime, _historical, owner, _model):
                CHECK.write_private_json(owner / "acceptance-progress.json", {"tag":"old","stage":"native_adoption","receipt":{"original":"preserve"}})
                raise AssertionError("controlled partial failure")
            argv = ["check-standalone-adoption.py", "--runtime", str(runtime), "--fixture-report", str(inputs),
                    "--model-js", str(model), "--tag", "old", "--output", str(output)]
            with patch("sys.argv", argv), patch.object(CHECK, "module", return_value=SimpleNamespace(owned_client_fixture=lambda:nullcontext(str(owned)))), patch.object(CHECK, "qualify", side_effect=failed_qualification):
                with self.assertRaises(AssertionError):
                    CHECK.main()
            case = json.loads(output.read_text())["cases"][0]
            self.assertEqual(case["stage"], "native_adoption")
            self.assertEqual(case["receipt"], {"original":"preserve"})
            self.assertEqual(case["error"]["type"], "AssertionError")
            self.assertTrue((Path(case["preserved_owner"]) / "acceptance-progress.json").is_file())

    def test_stale_config_preimage_preserves_the_changed_file(self):
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / "config.toml"
            config.write_text("original")
            original = CHECK.sha(config)
            config.write_text("another writer's settings")
            with self.assertRaises(AssertionError):
                CHECK.replace_config(config, original, "replacement")
            self.assertEqual(config.read_text(), "another writer's settings")
            self.assertEqual(list(Path(directory).iterdir()), [config])

    def test_full_parity_keeps_nested_evidence_named_like_transport_fields(self):
        first = {"request_id":"one", "evidence":{"request_id":"actual fact", "cached":True}}
        second = {"request_id":"two", "evidence":{"request_id":"actual fact", "cached":True}}
        self.assertEqual(CHECK.comparable(first), CHECK.comparable(second))
        second["evidence"]["request_id"] = "different fact"
        self.assertNotEqual(CHECK.comparable(first), CHECK.comparable(second))
        self.assertEqual(first["request_id"], "one")

    def test_source_verification_parity_excludes_only_valid_observation_clocks(self):
        first = {"source_verification":{"observed_from_unix_ms":100, "observed_to_unix_ms":120,
                 "budget":{"max_bytes":4096}, "status":"complete", "sources":[{
                     "observed_from_unix_ms":101, "observed_to_unix_ms":110,
                     "source_id":"original", "content_hash":"abc", "status":"matched",
                     "budget":{"bytes":71}, "future":{"observed_from_unix_ms":42}}]}}
        original = copy.deepcopy(first)
        second = copy.deepcopy(first)
        for observation in [second["source_verification"], *second["source_verification"]["sources"]]:
            observation["observed_from_unix_ms"] += 1000
            observation["observed_to_unix_ms"] += 1000
        self.assertEqual(CHECK.comparable(first), CHECK.comparable(second))
        for key, replacement in [("source_id", "different"), ("content_hash", "def"),
                                 ("status", "changed"), ("budget", {"bytes":72}),
                                 ("future", {"observed_from_unix_ms":43})]:
            changed = copy.deepcopy(second)
            changed["source_verification"]["sources"][0][key] = replacement
            self.assertNotEqual(CHECK.comparable(first), CHECK.comparable(changed), key)
        for key, replacement in [("status", "incomplete"), ("budget", {"max_bytes":8192})]:
            changed = copy.deepcopy(second)
            changed["source_verification"][key] = replacement
            self.assertNotEqual(CHECK.comparable(first), CHECK.comparable(changed), key)
        for source in (False, True):
            for start, end in [(0, 10), (20, 10), (True, 10), (1, False), (1.0, 10), (1, "10"), (None, 10)]:
                with self.subTest(source=source, start=start, end=end):
                    invalid = copy.deepcopy(first)
                    observation = invalid["source_verification"]
                    if source:
                        observation = observation["sources"][0]
                    observation.update(observed_from_unix_ms=start, observed_to_unix_ms=end)
                    with self.assertRaisesRegex(AssertionError, "interval"):
                        CHECK.comparable(invalid)
        self.assertEqual(first, original, "Comparison must preserve raw diagnostic evidence")

    def test_inventory_does_not_follow_an_external_fixture_link(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            root = base / "root"
            root.mkdir()
            external = base / "external"
            external.write_text("outside the saved index")
            try:
                (root / "redirect").symlink_to(external)
            except OSError:
                self.skipTest("Host does not permit fixture symlinks")
            with self.assertRaises(AssertionError):
                CHECK.inventory(root)


if __name__ == "__main__":
    unittest.main()
