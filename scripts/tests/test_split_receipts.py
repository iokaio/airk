# SPDX-License-Identifier: Apache-2.0
"""Split C7 receipt and release-catalog contract over reviewed fixtures.

Every expectation below and in fixtures/split/receipts is a reviewed value,
written independently of scripts/split_receipts.py. Pre-split decoders are
exercised unchanged to show that no split receipt enters a pre-split path.
"""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1]
FIXTURES = SCRIPTS.parent / "fixtures/split/receipts"


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), SCRIPTS / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


receipts = load("split_receipts")

FIXTURE_VALIDATORS = {
    "pair_metadata_3.json": "pair_metadata_3",
    "build_receipt_2.json": "build_receipt_2",
    "admin_assets_1.json": "admin_assets_1",
    "native_candidate_3.json": "native_candidate_3",
    "ci_runtime_2.json": "ci_runtime_2",
    "vsix_universal.json": "vsix_universal",
    "release_catalog.json": "release_catalog",
    "native-signing-local.json": "native_signing_local",
    "service-pair-signing-local.json": "service_pair_signing_local",
    "standalone-installer-local.json": "standalone_installer_local",
}
SPLIT_SCHEMAS = {
    "pair_metadata_3": 3,
    "build_receipt_2": 2,
    "admin_assets_1": "airk-admin-assets/1",
    "native_candidate_3": "airk-native-candidate/3",
    "ci_runtime_2": "airk-ci-runtime/2",
    "vsix_universal": "airk-extension-candidate/2",
    "release_catalog": "airk-release-catalog/1",
    "native_signing_local": "airk-native-signing-local/1",
    "service_pair_signing_local": "airk-service-pair-signing-local/1",
    "standalone_installer_local": "airk-standalone-installer-local/1",
}
PRE_SPLIT_NAMES = ("airk-native-candidate/1", "airk-native-candidate/2", "airk-ci-runtime/1",
                   "airk-extension-candidate/1", "airk-extension-stage/1", "airk-native-signing/1",
                   "airk-service-pair-signing/1", "airk-standalone-installer/1", "airk-github-release/1")
# Integer schemas: pair metadata 1 (historical pair) and 2 (single image); build receipt 1.
PRE_SPLIT_INTEGERS = {"pair_metadata_3": (1, 2), "build_receipt_2": (1,)}
REQUIRED_CODES = {"ok", "schema_mismatch", "unknown_field", "digest_invalid"}
LOCAL_CODES = {"hosted_provenance", "path_absolute"}
LOCAL_FIXTURES = {"native-signing-local.json", "service-pair-signing-local.json", "standalone-installer-local.json"}


def fixture(name):
    return json.loads((FIXTURES / name).read_text(encoding="utf-8"))


def materialize(value):
    if isinstance(value, dict) and set(value) == {"$repeat"}:
        text, count = value["$repeat"]
        return text * count
    return value


def build(base, case):
    if "replace" in case:
        return case["replace"]
    document = copy.deepcopy(base)
    for path in case.get("delete", []):
        node = document
        for key in path[:-1]:
            node = node[key]
        del node[path[-1]]
    for path, value in case.get("set", []):
        node = document
        for key in path[:-1]:
            node = node[key]
        node[path[-1]] = materialize(copy.deepcopy(value))
    return document


def outcome(kind, document):
    try:
        accepted = receipts.VALIDATORS[kind](document)
    except receipts.ReceiptError as error:
        return error.code
    return "ok" if accepted == SPLIT_SCHEMAS[kind] else f"accepted {accepted!r}"


def digest(data):
    return hashlib.sha256(data).hexdigest()


class SplitReceiptFixtureTests(unittest.TestCase):
    def test_fixture_roster_and_schema_identities_are_reviewed(self):
        self.assertEqual({path.name for path in FIXTURES.glob("*.json")}, set(FIXTURE_VALIDATORS))
        self.assertEqual(set(receipts.VALIDATORS), set(FIXTURE_VALIDATORS.values()))
        for name, kind in FIXTURE_VALIDATORS.items():
            data = fixture(name)
            self.assertEqual((data["schema"], data["validator"]), ("airk.split-fixture.receipts/1", kind))
            self.assertEqual(data["base"]["schema"], SPLIT_SCHEMAS[kind])
        self.assertFalse(set(PRE_SPLIT_NAMES) & {v for v in SPLIT_SCHEMAS.values() if isinstance(v, str)})

    def test_every_case_matches_its_reviewed_expectation(self):
        selected = 0
        for name, kind in FIXTURE_VALIDATORS.items():
            data = fixture(name)
            codes = {case["expect"] for case in data["cases"]}
            self.assertTrue(REQUIRED_CODES | (LOCAL_CODES if name in LOCAL_FIXTURES else set()) <= codes, name)
            self.assertTrue(codes - {"ok"} <= receipts.CODES, name)
            self.assertEqual(len({case["name"] for case in data["cases"]}), len(data["cases"]), name)
            for case in data["cases"]:
                with self.subTest(fixture=name, case=case["name"]):
                    self.assertEqual(outcome(kind, build(data["base"], case)), case["expect"])
                selected += 1
        self.assertGreater(selected, 0)
        print(f"split receipt fixture cases: {selected}")

    def test_release_catalog_counts_come_from_the_reviewed_fixture(self):
        data = fixture("release_catalog.json")
        base = data["base"]
        assets = base["assets"]
        self.assertEqual(len(base["native_targets"]), 4)
        self.assertEqual([name for name in assets if name.endswith(".vsix")], ["airk-1.9.0.vsix"])
        self.assertEqual(len([name for name in assets if name.endswith(".zip")]), 4)
        self.assertEqual(len([name for name in assets if "-installer." in name and not name.endswith(".json")]), 4)
        self.assertEqual(len(assets), 18)
        self.assertEqual(len(base["release_inputs"]), 9)
        self.assertEqual(len(base["producer_artifacts"]), 5)
        self.assertFalse([name for name in assets if any(p in name for p in ("win32-", "darwin-x64", "darwin-arm64", "linux-x64"))])
        preview = next(case for case in data["cases"] if case["name"] == "valid preview release without installers")
        self.assertEqual(len(build(base, preview)["assets"]), 10)

    def test_split_validators_refuse_every_pre_split_schema(self):
        for name, kind in FIXTURE_VALIDATORS.items():
            base = fixture(name)["base"]
            for schema in PRE_SPLIT_NAMES + PRE_SPLIT_INTEGERS.get(kind, (1, 2)):
                with self.subTest(kind=kind, schema=schema):
                    self.assertEqual(outcome(kind, {**base, "schema": schema}), "schema_mismatch")


class PreSplitDecoderTests(unittest.TestCase):
    """Unchanged pre-split decoders refuse split receipts at their schema check."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def write(self, directory, name, value):
        directory.mkdir(parents=True, exist_ok=True)
        data = value if isinstance(value, bytes) else (json.dumps(value, indent=2) + "\n").encode("utf-8")
        (directory / name).write_bytes(data)
        return digest(data)

    def test_runtime_metadata_decoder_refuses_schema_3(self):
        builder = load("build-service-runtime")
        split = {**fixture("pair_metadata_3.json")["base"], "version": "1.8.23"}
        files = {"airk.exe": split["airk_sha256"], "airkctl.exe": split["airkctl_sha256"]}
        self.assertFalse(builder.runtime_metadata_matches(split, "1.8.23", "windows", "x86_64", files))
        single = {"schema": 2, "version": "1.8.23", "os": "windows", "arch": "x86_64", "airk_sha256": split["airk_sha256"]}
        self.assertTrue(builder.runtime_metadata_matches(single, "1.8.23", "windows", "x86_64", files))

    def test_hosted_signing_readers_refuse_local_receipts(self):
        signer = load("sign-native")
        component = fixture("native-signing-local.json")["base"]
        self.write(self.root / "component", "signing.json", component)
        with self.assertRaisesRegex(ValueError, "Signing receipt differs from the selected build"):
            signer.read_verified(self.root / "component", self.root / "missing.exe", component["target"],
                                 component["commit"], component["cargo_lock_sha256"])
        pair = fixture("service-pair-signing-local.json")["base"]
        self.write(self.root / "pair", "signing.json", pair)
        with self.assertRaisesRegex(ValueError, "Pair signing receipt differs from tested source"):
            signer.read_verified_pair(self.root / "pair", self.root / "missing", pair["target"],
                                      pair["commit"], pair["cargo_lock_sha256"])

    def test_ci_runtime_restore_refuses_transfer_schema_2(self):
        transfer_module = load("ci-runtime")
        receipt = fixture("ci_runtime_2.json")["base"]
        identity = {key: receipt[key] for key in ("commit", "tree", "run_id", "target", "qualification")}
        transfer = self.root / "transfer"
        self.write(transfer, "runtime.zip", b"PK\x05\x06" + bytes(18))
        self.write(transfer, "transfer.json", receipt)
        # A pre-split decoder: pinned, so the case does not follow the checkout's version.
        with patch.object(transfer_module, "identity", return_value=identity), \
                patch.object(transfer_module, "current_version", return_value="1.8.23"):
            with self.assertRaisesRegex(ValueError, "Runtime transfer belongs to another source"):
                transfer_module.restore(transfer, self.root / "runtime", receipt["target"])

    def test_release_assembly_refuses_split_candidate_and_local_installer(self):
        release = load("github-release")
        candidate = fixture("native_candidate_3.json")["base"]
        directory = self.root / "native"
        archive = self.write(directory, candidate["archive"], b"fixture archive")
        manifest = self.write(directory, "manifest.json", {**candidate, "archive_sha256": archive})
        self.write(directory, "SHA256SUMS", f"{archive}  {candidate['archive']}\n{manifest}  manifest.json\n".encode())
        with self.assertRaisesRegex(ValueError, "Native candidate provenance or signing state differs"):
            release.verify_native(directory, candidate["target"], candidate["commit"], candidate["version"], False)
        installer = fixture("standalone-installer-local.json")["base"]
        directory = self.root / "installer"
        self.write(directory, installer["artifact"], b"fixture installer")
        self.write(directory, "installer.json", installer)
        self.write(directory, "SHA256SUMS", b"")
        with self.assertRaisesRegex(ValueError, "Standalone installer source, runtime or artifact identity differs"):
            release.verify_installer(directory, installer["target"], installer["commit"], installer["version"], {})


if __name__ == "__main__":
    unittest.main()
