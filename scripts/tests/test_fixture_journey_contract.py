# SPDX-License-Identifier: Apache-2.0
"""Real producer-to-consumer scope contract; no runtime or editor is started."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace
import unittest
from unittest.mock import Mock


ROOT = Path(__file__).resolve().parents[2]


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


producer = load("journey_producer", "prepare-native-editor-legacy.py")
clients = load("journey_clients", "check-service-clients.py")


class FixtureJourneyContractTests(unittest.TestCase):
    def connection(self):
        # Native returned registration: argument generation must preserve it in
        # full, independently of the separately tested registration transport.
        fixture = clients.Fixture.__new__(clients.Fixture)
        fixture.compatibility = {
            "topology": "paired", "transport": "stdio", "authority": "legacy",
            "setup_owner": "raw_registration", "input_owner": "native_administrative",
            "original_scope": "service_project", "migration_provenance": "imported_legacy_snapshot",
            "migration_path": "audited_imported_legacy",
        }
        registration = {"binding": "a" * 64, "root": str(ROOT / "fictional-source"),
                        "data_root": str(ROOT / "fictional-data"), "collection": "legacy"}
        fixture.projects = {}
        fixture.scopes = {}
        fixture.control = ROOT / "fictional-control"
        fixture.run = Mock(return_value=SimpleNamespace(stdout=json.dumps(registration)))
        fixture.rpc = Mock(return_value={"project": {"registration": registration}})
        fixture.register("legacy", Path(registration["root"]), Path(registration["data_root"]))
        fixture.run.assert_called_once_with("--root", Path(registration["root"]), "--data-dir",
            Path(registration["data_root"]), "--collection", "legacy", "service", "prepare-registration")
        fixture.rpc.assert_called_once_with({"register": {"project": registration}})
        install = ROOT / "fictional-install"
        connection = producer.original_connection(fixture, "legacy", install)
        return {"install_dir": str(install), "source_root": registration["root"],
                "data_root": registration["data_root"], "original_connection": connection,
                "legacy_config": {"args": connection["args"]}}

    def consume(self, value):
        script = """
const {qualifyOriginalConnection}=require('./scripts/native-editor-legacy.cjs');
const {launchArgs}=require('./packages/vscode/out/model.js');
const assert=require('node:assert/strict');
let input='';process.stdin.setEncoding('utf8');
process.stdin.on('data',part=>input+=part);
process.stdin.on('end',()=>{
    const value=JSON.parse(input),scope=qualifyOriginalConnection(value);
    assert.deepEqual(value.original_connection.args,launchArgs({root:scope.root,dataDir:scope.data_root,collection:scope.collection,
        service:{installDir:value.install_dir,project:scope.binding}},'mcp',true));
});
"""
        return subprocess.run(["node", "-e", script], input=json.dumps(value), cwd=ROOT,
                              text=True, capture_output=True, timeout=15)

    def test_real_producer_original_scope_is_accepted_by_real_consumer(self):
        value = self.connection()
        original = value["original_connection"]
        registration = original["scope"]
        self.assertEqual(original["args"], ["runtime", "launch", "--install-dir", value["install_dir"], "--",
            "--root", registration["root"], "--data-dir", registration["data_root"], "--collection",
            registration["collection"], "--service-project", registration["binding"], "mcp"])
        self.assertEqual(original["migration_provenance"], "imported_legacy_snapshot")
        self.assertEqual(original["retrieval_authority"], "sealed_imported_snapshot")
        result = self.consume(value)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_crossed_scope_arguments_or_authority_are_refused_by_real_consumer(self):
        value = self.connection()
        mutations = [
            lambda v: v["original_connection"]["scope"].update(binding="b" * 64),
            lambda v: v["original_connection"]["scope"].update(root=str(ROOT / "foreign-source")),
            lambda v: v["original_connection"]["scope"].update(data_root=str(ROOT / "foreign-data")),
            lambda v: v["original_connection"]["scope"].update(collection="foreign"),
            lambda v: v["original_connection"].update(migration_provenance="user_reviewed_project_grant"),
            lambda v: v["original_connection"].update(retrieval_authority="catalog_session"),
            lambda v: v["legacy_config"].update(args=v["legacy_config"]["args"][5:]),
            lambda v: v["original_connection"]["args"].__setitem__(3, str(ROOT / "foreign-install")),
        ]
        for number, mutate in enumerate(mutations):
            with self.subTest(case=number):
                crossed = copy.deepcopy(value)
                mutate(crossed)
                result = self.consume(crossed)
                self.assertNotEqual(result.returncode, 0, "Crossed producer authority was accepted")

    def test_catalog_predecessor_cannot_be_substituted_for_imported_legacy(self):
        for topology in ("paired", "single_image"):
            with self.subTest(topology=topology):
                compatibility = {"topology": topology, "transport": "stdio" if topology == "paired" else "https",
                    "authority": "catalog", "setup_owner": "reviewed_catalog_setup", "input_owner": "native_administrative",
                    "original_scope": "catalog_session", "migration_provenance": "user_reviewed_project_grant"}
                with self.assertRaises((AssertionError, ValueError, RuntimeError)):
                    producer.require_imported_predecessor(compatibility)

    def test_raw_predecessor_without_verified_migration_path_is_refused(self):
        for path in (None, "unverified", "reviewed_catalog"):
            with self.subTest(path=path), self.assertRaisesRegex(AssertionError, "migration path"):
                producer.require_imported_predecessor({"topology":"paired","authority":"legacy",
                    "original_scope":"service_project","migration_path":path})


if __name__ == "__main__":
    unittest.main()
