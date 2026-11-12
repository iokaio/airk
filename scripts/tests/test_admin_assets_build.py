# SPDX-License-Identifier: Apache-2.0
"""The admin bundle owner: committed-bundle verification, framed digests, C7 inventory and command order."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location("build_admin_assets", Path(__file__).parents[1] / "build-admin-assets.py")
admin = importlib.util.module_from_spec(spec)
spec.loader.exec_module(admin)

FILES = {"index.html": (b"<!doctype html>fixture", "text/html; charset=utf-8"),
         "app.0000.js": (b"console.log(1)", "text/javascript; charset=utf-8"),
         "app.0000.css": (b"body{}", "text/css; charset=utf-8")}


def framed(domain, fields):
    """Independent restatement of the I4 framing: magic, then u64 big-endian length + bytes per field."""
    data = b"AIRK-DIGEST-1\0"
    for field in (domain, *fields):
        data += len(field).to_bytes(8, "big") + field
    return hashlib.sha256(data).hexdigest()


class AdminAssetsTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.assets = Path(self.temporary.name) / "admin-assets"
        self.assets.mkdir()
        for name, (data, _) in FILES.items():
            (self.assets / name).write_bytes(data)
        self.write_manifest()

    def write_manifest(self, schema="airk-admin-assets/1"):
        rows = {name: {"content_type": kind, "sha256": hashlib.sha256(data).hexdigest()} for name, (data, kind) in FILES.items()}
        (self.assets / "manifest.json").write_text(json.dumps({"files": rows, "schema": schema}), encoding="utf-8")

    def test_framed_digests_follow_name_order(self):
        names = sorted(FILES)
        inventory = [part for name in names for part in (name.encode(), hashlib.sha256(FILES[name][0]).hexdigest().encode(), FILES[name][1].encode())]
        bundle = [part for name in names for part in (name.encode(), FILES[name][0])]
        self.assertEqual(admin.digests(self.assets), {
            "inventory_sha256": framed(b"airk/admin-asset-inventory/1", inventory),
            "bundle_sha256": framed(b"airk/admin-asset-bundle/1", bundle)})

    def test_committed_bundle_refuses_stale_unlisted_missing_and_foreign_schema(self):
        (self.assets / "app.0000.js").write_bytes(b"edited after the build")
        with self.assertRaisesRegex(RuntimeError, "Stale admin asset app.0000.js"):
            admin.verify_committed(self.assets)
        (self.assets / "app.0000.js").write_bytes(FILES["app.0000.js"][0])
        (self.assets / "extra.js").write_bytes(b"unlisted")
        with self.assertRaisesRegex(RuntimeError, "differ from the manifest: extra.js"):
            admin.verify_committed(self.assets)
        (self.assets / "extra.js").unlink()
        (self.assets / "index.html").unlink()
        with self.assertRaisesRegex(RuntimeError, "differ from the manifest: index.html"):
            admin.verify_committed(self.assets)
        (self.assets / "index.html").write_bytes(FILES["index.html"][0])
        self.write_manifest("airk-admin-assets/2")
        with self.assertRaisesRegex(RuntimeError, "manifest schema"):
            admin.verify_committed(self.assets)

    def test_inventory_document_is_the_c7_list_shape_and_rebinds_only_unchanged_bundles(self):
        document = admin.inventory_document("v24.15.0", "11.6.2", self.assets)
        self.assertEqual(document["schema"], "airk-admin-assets/1")
        self.assertEqual([row["name"] for row in document["files"]], ["app.0000.css", "app.0000.js", "index.html"])
        self.assertEqual([row["bytes"] for row in document["files"]], [6, 14, 22])
        self.assertEqual(document["toolchain"], {"node": "v24.15.0", "npm": "11.6.2"})
        block = admin.block_from_inventory(document, self.assets)
        self.assertEqual(list(block), ["bundle_sha256", "inventory_sha256", "node", "npm", "locks"])
        self.assertEqual(set(block["locks"]), {"packages/admin/package-lock.json"})
        self.assertEqual(block["bundle_sha256"], document["bundle_sha256"])
        with self.assertRaisesRegex(ValueError, "value_invalid"):
            admin.inventory_document("24.15.0", "11.6.2", self.assets)
        (self.assets / "app.0000.css").write_bytes(b"body{color:red}")
        FILES["app.0000.css"] = (b"body{color:red}", FILES["app.0000.css"][1])
        try:
            self.write_manifest()
            with self.assertRaisesRegex(RuntimeError, "differs from the committed bundle"):
                admin.block_from_inventory(document, self.assets)
        finally:
            FILES["app.0000.css"] = (b"body{}", FILES["app.0000.css"][1])

    def test_check_runs_locked_install_then_drift_check_and_stops_on_failure(self):
        output = Path(self.temporary.name) / "out" / "admin-assets.json"
        calls = []
        capture = mock.Mock(side_effect=["v24.15.0", "11.6.2"])
        with mock.patch.object(admin, "ASSETS", self.assets):
            admin.check(output, runner=lambda command, cwd: calls.append((command, cwd)), capturer=capture)
        self.assertEqual(calls, [(["npm", "ci", "--ignore-scripts", "--no-audit", "--no-fund"], admin.PACKAGE),
                                 (["npm", "run", "check"], admin.PACKAGE)])
        self.assertEqual(json.loads(output.read_text(encoding="utf-8"))["toolchain"], {"node": "v24.15.0", "npm": "11.6.2"})
        calls.clear()
        with mock.patch.object(admin, "ASSETS", self.assets):
            admin.check(output, skip_install=True, runner=lambda command, cwd: calls.append(command),
                        capturer=mock.Mock(side_effect=["v24.15.0", "11.6.2"]))
        self.assertEqual(calls, [["npm", "run", "check"]])
        failed = Path(self.temporary.name) / "failed.json"

        def fail(command, cwd):
            raise SystemExit("npm ci failed (1)")
        with self.assertRaises(SystemExit), mock.patch.object(admin, "ASSETS", self.assets):
            admin.check(failed, runner=fail, capturer=mock.Mock(side_effect=["v24.15.0", "11.6.2"]))
        self.assertFalse(failed.exists())

    def test_the_committed_bundle_is_current(self):
        rows = admin.verify_committed()
        self.assertTrue(rows)
        self.assertEqual(set(admin.digests()), {"inventory_sha256", "bundle_sha256"})


if __name__ == "__main__":
    unittest.main()
