# SPDX-License-Identifier: Apache-2.0
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

SPEC = importlib.util.spec_from_file_location("release_version", Path(__file__).parents[1] / "release-version.py")
version = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(version)
SPEC = importlib.util.spec_from_file_location("download_tests", Path(__file__).with_name("test_downloads_site.py"))
fixtures = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fixtures)


def entries(number="0.1.0"):
    return [dict(version=number, targetPlatform=platform, properties=[]) for platform in sorted(version.PLATFORMS)]


class VersionContracts(unittest.TestCase):
    def plan(self, destination="marketplace", market=None, releases=None, tags=None, **kwargs):
        return version.plan("0.1.0", "stable", destination, market or [], releases or [], tags or [],
                            publish=kwargs.pop("publish", True), **kwargs)

    def test_unused_manual_version_stays_and_rehearsals_never_bump(self):
        self.assertEqual(self.plan(market=entries("0.0.9"))["action"], "publish")
        self.assertEqual(self.plan(market=entries(), releases=[fixtures.release(preview=False)], publish=False)["action"], "rehearse")
        self.assertEqual(self.plan()["next_version"], "")

    def test_only_both_completed_channels_allocate_the_next_unused_patch(self):
        released = [fixtures.release(preview=False)]
        for destination in ("marketplace", "github"):
            decision = self.plan(destination, entries(), released)
            self.assertEqual((decision["action"], decision["next_version"]), ("bump", "0.1.1"))
            decision = self.plan(destination, entries() + entries("0.1.1"), released, ["v0.1.2", "v0.1.3-build.7"])
            self.assertEqual(decision["next_version"], "0.1.4")
            self.assertEqual(self.plan(destination, entries(), released, attempt=2)["action"], "skip")

    def test_partial_release_finishes_without_double_publication(self):
        released = [fixtures.release(preview=False)]
        self.assertEqual(self.plan("marketplace", entries())["action"], "skip")
        self.assertEqual(self.plan("github", entries())["action"], "publish")
        self.assertEqual(self.plan("github", releases=released)["action"], "skip")
        self.assertEqual(self.plan("marketplace", releases=released)["action"], "publish")
        self.assertEqual(self.plan(market=entries()[:2], releases=released)["action"], "publish")
        self.assertEqual(self.plan("github", entries()[:2], released)["action"], "skip")

    def test_incomplete_releases_and_conflicting_channels_fail_closed(self):
        broken = fixtures.release(preview=False)
        broken["assets"].pop()
        with self.assertRaisesRegex(ValueError, "incomplete"):
            self.plan(market=entries(), releases=[broken])
        with self.assertRaisesRegex(ValueError, "conflicting"):
            self.plan(releases=[fixtures.release()])
        with self.assertRaisesRegex(ValueError, "conflicting"):
            self.plan(market=entries() + entries()[:1])
        preview = entries()
        preview[0]["properties"] = [dict(key="Microsoft.VisualStudio.Code.PreRelease", value="true")]
        with self.assertRaisesRegex(ValueError, "conflicting"):
            self.plan(market=preview)
        with self.assertRaisesRegex(ValueError, "original run"):
            self.plan("github", entries(), [{**fixtures.release(preview=False), "draft": True}])
        with self.assertRaisesRegex(ValueError, "Existing draft/tag"):
            self.plan("github", tags=["v0.1.0"])

    def test_modern_stable_publication_is_incomplete_without_all_standalone_installers(self):
        item = fixtures.release(preview=False)
        item["tag_name"] = "v1.8.14"
        for asset in item["assets"]:
            asset["name"] = asset["name"].replace("0.1.0", "1.8.14")
            asset["browser_download_url"] = f"{fixtures.site.GITHUB}/releases/download/v1.8.14/{asset['name']}"
        with self.assertRaisesRegex(ValueError, "required standalone"):
            version.plan("1.8.14", "stable", "github", [], [item], [], publish=True)
        # The public page may still describe that legacy asset set honestly;
        # publication completion cannot treat it as the new installer release.
        self.assertEqual(fixtures.site.release_metadata(item)["installers"], {})

    def test_version_edit_keeps_dependency_versions_and_synchronizes_manifests(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "packages/vscode").mkdir(parents=True)
            (root / "Cargo.toml").write_text('[workspace.package]\nversion = "0.1.0"\n')
            (root / "Cargo.lock").write_text('[[package]]\nname = "airk-core"\nversion = "0.1.0"\n\n[[package]]\nname = "airkctl"\nversion = "0.1.0"\n\n[[package]]\nname = "dependency"\nversion = "0.1.0"\n')
            (root / "packages/vscode/package.json").write_text('{"version":"0.1.0","dependencies":{"other":"0.1.0"}}')
            (root / "packages/vscode/package-lock.json").write_text('{"version":"0.1.0","packages":{"":{"version":"0.1.0"},"node_modules/other":{"version":"0.1.0"}}}')
            edits = version.changes("0.1.0", "0.1.1", root)
            for name, text in edits.items():
                (root / name).write_text(text, encoding="utf-8")
            self.assertEqual(version.source_version(root), "0.1.1")
            self.assertIn('name = "dependency"\nversion = "0.1.0"', edits["Cargo.lock"])
            self.assertIn('name = "airkctl"\nversion = "0.1.1"', edits["Cargo.lock"])
            self.assertEqual(json.loads(edits["packages/vscode/package.json"])["dependencies"]["other"], "0.1.0")
            with self.assertRaises(ValueError):
                version.changes("0.1.0", "0.1.2", root)

    def test_existing_marketplace_package_must_match_candidate_payload(self):
        payload = b"fictional native manifest and content"
        receipt = {"payload": {"native/manifest.json": version.hashlib.sha256(payload).hexdigest()}}
        for corrupt in (False, True):
            archive = io.BytesIO()
            with zipfile.ZipFile(archive, "w") as zipped:
                zipped.writestr("extension/native/manifest.json", b"wrong source" if corrupt else payload)
                zipped.writestr("marketplace.signature", b"store signature")
            archive.seek(0)
            if corrupt:
                with self.assertRaisesRegex(ValueError, "differs"):
                    version.verify_published_payload(archive, receipt)
            else:
                version.verify_published_payload(archive, receipt)

    def test_marketplace_http_gzip_is_decoded_before_zip_verification(self):
        data = b"PK fictional VSIX bytes"
        for encoding in ("identity", "gzip"):
            response = io.BytesIO(version.gzip.compress(data) if encoding == "gzip" else data)
            response.headers = {"Content-Encoding": encoding}
            output = io.BytesIO()
            version.download_vsix(response, output)
            self.assertEqual(output.read(), data)

    def publication_fixture(self, root):
        paths = []
        for platform in sorted(version.PLATFORMS):
            parent = root / platform
            parent.mkdir()
            path = parent / f"airk-0.1.0-{platform}.vsix"
            paths.append(str(path))
            (parent / "manifest.json").write_text(json.dumps({"payload": {
                "native/image": version.hashlib.sha256(platform.encode()).hexdigest()}}))
        inventory = root / "inventory.json"
        inventory.write_text(json.dumps(paths))
        return inventory

    def test_postpublish_reads_all_served_platforms_and_allows_store_signature_metadata(self):
        with tempfile.TemporaryDirectory() as temporary:
            inventory = self.publication_fixture(Path(temporary))
            def response(url, **kwargs):
                platform = url.split("targetPlatform=")[1]
                archive = io.BytesIO()
                with zipfile.ZipFile(archive, "w") as zipped:
                    zipped.writestr("extension/native/image", platform.encode())
                    zipped.writestr("marketplace.signature", b"new store metadata")
                archive.seek(0)
                archive.headers = {}
                return archive
            with patch.object(version, "marketplace", side_effect=[entries()[:2], entries()]), \
                    patch.object(version, "urlopen", side_effect=response) as downloads, \
                    patch.object(version.time, "sleep") as sleep:
                version.verify_marketplace_publication(inventory, "0.1.0", "stable")
            self.assertEqual(downloads.call_count, 4)
            sleep.assert_called_once_with(10)
            def corrupted_response(url, **kwargs):
                archive = io.BytesIO()
                with zipfile.ZipFile(archive, "w") as zipped:
                    zipped.writestr("extension/native/image", b"wrong native build")
                archive.seek(0)
                archive.headers = {}
                return archive
            with patch.object(version, "marketplace", return_value=entries()) as catalog, \
                    patch.object(version, "urlopen", side_effect=corrupted_response) as downloads, \
                    patch.object(version.time, "sleep") as sleep:
                with self.assertRaisesRegex(ValueError, "differs"):
                    version.verify_marketplace_publication(inventory, "0.1.0", "stable")
                self.assertEqual((catalog.call_count, downloads.call_count), (1, 1))
                sleep.assert_not_called()

    def test_postpublish_missing_is_bounded_but_payload_or_channel_mismatch_never_retries(self):
        with tempfile.TemporaryDirectory() as temporary:
            inventory = self.publication_fixture(Path(temporary))
            with patch.object(version, "marketplace", return_value=entries("0.0.9")) as catalog, \
                    patch.object(version.time, "sleep") as sleep:
                with self.assertRaisesRegex(RuntimeError, "did not propagate"):
                    version.verify_marketplace_publication(inventory, "0.1.0", "stable", attempts=3)
            self.assertEqual((catalog.call_count, sleep.call_count), (3, 2))
            for error in (ValueError("payload differs"), version.HTTPError("url", 403, "denied", {}, None)):
                with patch.object(version, "marketplace", return_value=entries()) as catalog, \
                        patch.object(version, "verify_marketplace_package", side_effect=error), \
                        patch.object(version.time, "sleep") as sleep:
                    with self.assertRaises(type(error)):
                        version.verify_marketplace_publication(inventory, "0.1.0", "stable")
                self.assertEqual(catalog.call_count, 1)
                sleep.assert_not_called()
            with patch.object(version, "marketplace", return_value=entries()), \
                    patch.object(version.time, "sleep") as sleep:
                with self.assertRaisesRegex(ValueError, "channel"):
                    version.verify_marketplace_publication(inventory, "0.1.0", "preview")
                sleep.assert_not_called()

    def test_postpublish_retries_package_404_only_and_stops_at_time_deadline(self):
        with tempfile.TemporaryDirectory() as temporary:
            inventory = self.publication_fixture(Path(temporary))
            with patch.object(version, "marketplace", return_value=entries()), \
                    patch.object(version, "verify_marketplace_package", side_effect=[
                        version.HTTPError("url", 404, "missing", {}, None), None, None, None, None]) as downloads, \
                    patch.object(version.time, "sleep") as sleep:
                version.verify_marketplace_publication(inventory, "0.1.0", "stable")
                self.assertEqual(downloads.call_count, 5)
                sleep.assert_called_once_with(10)
            with patch.object(version.time, "monotonic", side_effect=[0, 181]), \
                    patch.object(version, "marketplace") as catalog:
                with self.assertRaisesRegex(RuntimeError, "time limit"):
                    version.verify_marketplace_publication(inventory, "0.1.0", "stable")
                catalog.assert_not_called()

    def test_split_versions_publish_and_read_back_one_universal_package(self):
        split = "1.8.24"
        served = [dict(version=split, properties=[])]  # Marketplace lists a universal package without a target.
        self.assertEqual(version.market_platforms(served, split, "stable"), {"universal"})
        self.assertEqual(version.plan(split, "stable", "marketplace", served, [], [], publish=True)["action"], "skip")
        self.assertEqual(version.plan(split, "stable", "marketplace", [], [], [], publish=True)["action"], "publish")
        for name, catalog, number in (("platform entry", [dict(version=split, targetPlatform="win32-x64", properties=[])], split),
                                      ("duplicate", served + served, split),
                                      ("untargeted pre-split", [dict(version="1.8.23", properties=[])], "1.8.23")):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "conflicting"):
                version.market_platforms(catalog, number, "stable")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "universal").mkdir()
            path = root / "universal" / "airk-1.8.24.vsix"
            (root / "universal" / "manifest.json").write_text(json.dumps({"payload": {
                "package.json": version.hashlib.sha256(b"fictional universal host").hexdigest()}}))
            inventory = root / "inventory.json"
            inventory.write_text(json.dumps([str(path)]))
            self.assertEqual(version.package_inventory(inventory, split), {"universal": str(path)})
            platforms = root / "platforms.json"
            platforms.write_text(json.dumps([str(root / f"airk-1.8.24-{p}.vsix") for p in sorted(version.PLATFORMS)]))
            with self.assertRaisesRegex(ValueError, "universal VSIX inventory"):
                version.package_inventory(platforms, split)
            urls = []
            def response(url, **kwargs):
                urls.append(url)
                archive = io.BytesIO()
                with zipfile.ZipFile(archive, "w") as zipped:
                    zipped.writestr("extension/package.json", b"fictional universal host")
                    zipped.writestr("marketplace.signature", b"store metadata")
                archive.seek(0)
                archive.headers = {}
                return archive
            with patch.object(version, "marketplace", return_value=served), \
                    patch.object(version, "urlopen", side_effect=response), patch.object(version.time, "sleep") as sleep:
                version.verify_marketplace_publication(inventory, split, "stable")
                self.assertEqual(version.pending_packages(inventory, split, "stable", served), [])
                self.assertEqual(version.pending_packages(inventory, split, "stable", []), [str(path)])
            sleep.assert_not_called()
            self.assertEqual(urls, ["https://marketplace.visualstudio.com/_apis/public/gallery/publishers/iokaio/"
                                    "vsextensions/airk/1.8.24/vspackage"] * 2)

    def test_postpublish_workflow_uses_authenticated_inventory_after_upload(self):
        workflow = (version.ROOT / ".github/workflows/extension-release.yml").read_text()
        self.assertLess(workflow.index("node packages/vscode/scripts/publish.cjs"),
                        workflow.index("scripts/release-version.py verify-published"))
        self.assertIn('--publish --inventory artifacts/verified-vsix.json', workflow)

    def test_new_main_version_wins_over_a_stale_dispatch(self):
        with patch.object(version, "api", return_value={"object": {"sha": "b" * 40}}) as api:
            with self.assertRaisesRegex(ValueError, "Main changed"):
                version.prepare_pr(dict(action="bump", version="0.1.0", next_version="0.1.1"), "a" * 40)
            self.assertEqual(api.call_count, 1)

    def test_version_pr_is_idempotent_and_has_no_invented_dco(self):
        calls = []
        def api(path, payload=None, **kwargs):
            calls.append((path, payload))
            if path == "git/ref/heads/main": return {"object": {"sha": "a" * 40}}
            if path.startswith("pulls?"): return []
            if path.startswith("git/ref/heads/release/"): return None
            if path.startswith("git/commits/"): return {"tree": {"sha": "b" * 40}}
            if path in ("git/trees", "git/commits"): return {"sha": "c" * 40}
            if path == "git/refs": return {}
            if path == "pulls": return {"html_url": "https://github.com/iokaio/airk/pull/123"}
            raise AssertionError(path)
        decision = dict(action="bump", version="0.1.0", next_version="0.1.1")
        with patch.object(version, "api", side_effect=api), patch.object(version, "changes", return_value={"Cargo.toml": "fixture"}):
            self.assertTrue(version.prepare_pr(decision, "a" * 40).endswith("/123"))
        commit = next(payload for path, payload in calls if path == "git/commits")
        self.assertNotIn("Signed-off-by:", commit["message"])
        self.assertNotIn("author", commit)
        pull = next(payload for path, payload in calls if path == "pulls")
        self.assertTrue(pull["draft"])
        self.assertIn("signed-off commit", pull["body"])
        with patch.object(version, "api", side_effect=[{"object": {"sha": "a" * 40}}, [{"html_url": "existing"}]]) as api:
            self.assertEqual(version.prepare_pr(decision, "a" * 40), "existing")
            self.assertEqual(api.call_count, 2)


if __name__ == "__main__":
    unittest.main()
