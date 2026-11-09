# SPDX-License-Identifier: Apache-2.0
"""Download links must select published AIRK packages without exposing other files."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "downloads_site", Path(__file__).resolve().parents[1] / "build-downloads.py")
site = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(site)


def release(build=1, *, preview=True, published="2026-10-14T10:00:00Z"):
    version = "0.1.0"
    tag = f"v{version}-build.{build}"
    names = ["release.json", "release-notes.md", "SHA256SUMS"]
    for platform, target, *_ in site.PLATFORMS:
        names.extend((f"airk-{version}-{platform}.vsix", f"airk-{version}-{target}.zip",
                      f"airk-{version}-{platform}.vsix-manifest.json",
                      f"airk-{version}-{target}.native-manifest.json"))
    return {"tag_name": tag, "draft": False, "prerelease": preview, "published_at": published,
            "assets": [{"name": name, "state": "uploaded", "size": 1234, "digest": "sha256:" + "a" * 64,
                        "browser_download_url": f"{site.GITHUB}/releases/download/{tag}/{name}"}
                       for name in names]}


class DownloadSiteTests(unittest.TestCase):
    def with_installers(self, item, platforms=None):
        for platform, name in site.installer_names(item["tag_name"].split("-build.")[0][1:]).items():
            if platforms is None or platform in platforms:
                item["assets"].append({"name": name, "state": "uploaded", "size": 1234,
                    "digest": "sha256:" + "b" * 64,
                    "browser_download_url": f"{site.GITHUB}/releases/download/{item['tag_name']}/{name}"})
                manifest = site.installer_manifest_names(item["tag_name"].split("-build.")[0][1:])[platform]
                item["assets"].append({"name": manifest, "state": "uploaded", "size": 512,
                    "digest": "sha256:" + "c" * 64,
                    "browser_download_url": f"{site.GITHUB}/releases/download/{item['tag_name']}/{manifest}"})
        return item

    def test_unpublished_installers_are_unavailable_without_guessed_download_links(self):
        html = site.render_downloads(site.select_releases([release(preview=False)]))
        self.assertEqual(html.count('aria-disabled="true">Installer not published'), 4)
        self.assertNotIn("Download installer", html)
        self.assertNotIn(".vsix\"", html)
        self.assertNotIn("Verify build provenance", html)
        self.assertIn("Portable runtime ZIP", html)

    def test_published_installer_assets_select_exact_platform_architecture_and_signing(self):
        item = self.with_installers(release(preview=False))
        html = site.render_downloads(site.select_releases([item]))
        self.assertEqual(html.count(">Download installer "), 4)
        expected = {
            "win32-x64": "airk-0.1.0-x86_64-pc-windows-msvc-installer.exe",
            "darwin-arm64": "airk-0.1.0-aarch64-apple-darwin-installer.dmg",
            "darwin-x64": "airk-0.1.0-x86_64-apple-darwin-installer.dmg",
            "linux-x64": "airk-0.1.0-x86_64-unknown-linux-gnu-installer.tar.gz",
        }
        self.assertEqual(site.installer_names("0.1.0"), expected)
        for platform, name in expected.items():
            self.assertIn(f"{site.GITHUB}/releases/download/{item['tag_name']}/{name}", html)
        for label in ("Windows", "Apple silicon", "Intel x64", "Linux", "Signed", "Unsigned"):
            self.assertIn(label, html)
        self.assertNotIn(".vsix\"", html)
        self.assertIn("Unsigned · <a", html)
        self.assertIn("Verify build provenance", html)
        self.assertIn("release-verification.md#artifact-attestations", html)

    def test_installer_presence_never_borrows_an_asset_from_another_release_or_platform(self):
        item = self.with_installers(release(3, preview=False, published="2026-10-15T10:00:00Z"), {"win32-x64"})
        html = site.render_downloads(site.select_releases([self.with_installers(release(1, preview=False)), item]))
        self.assertEqual(html.count(">Download installer "), 1)
        self.assertEqual(html.count('aria-disabled="true">Installer not published'), 3)
        self.assertNotIn("/v0.1.0-build.1/", html)

    def test_installer_requires_its_uploaded_provenance_pair_and_exact_release_url(self):
        for missing in (-1, -2):
            item = self.with_installers(release(), {"win32-x64"})
            item["assets"].pop(missing)
            with self.assertRaisesRegex(ValueError, "incomplete standalone"):
                site.select_releases([item])
        for field, value in (("browser_download_url", "https://example.com/installer.exe"),
                             ("state", "starter"), ("digest", None), ("size", 0)):
            item = self.with_installers(release(), {"win32-x64"})
            item["assets"][-2][field] = value
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "invalid asset"):
                site.select_releases([item])

    def test_clean_version_tags_and_legacy_downloads(self):
        current = release(preview=False, published="2026-10-17T12:00:00Z")
        old_tag = current["tag_name"]
        current["tag_name"] = "v0.1.0"
        for asset in current["assets"]:
            asset["browser_download_url"] = asset["browser_download_url"].replace(old_tag, "v0.1.0")
        records = site.select_releases([release(preview=False), current])
        self.assertEqual([record["tag"] for record in records], ["v0.1.0"])
        html = site.render_downloads(records)
        self.assertIn("/v0.1.0/airk-0.1.0-x86_64-pc-windows-msvc.zip", html)
        self.assertNotIn("build number", html)
        current["assets"].pop()
        with self.assertRaisesRegex(ValueError, "incomplete"):
            site.select_releases([current])

    def test_latest_per_channel_independent_of_api_order(self):
        catalog = [release(3, published="2026-10-14T12:00:00Z"), release(1),
                   release(2, preview=False), {**release(4), "draft": True},
                   {"tag_name": "unrelated-tag", "draft": False}]
        records = site.select_releases(catalog)
        self.assertEqual([r["tag"] for r in records], ["v0.1.0-build.2", "v0.1.0-build.3"])
        html = site.render_downloads(records)
        for number in (2, 3):
            for _, target, *_ in site.PLATFORMS:
                self.assertIn(f"/v0.1.0-build.{number}/airk-0.1.0-{target}.zip", html)
        self.assertNotIn("build.1", html)
        self.assertNotIn("build.4", html)

    def test_empty_or_draft_only_has_no_download_links(self):
        for catalog in ([], [{**release(), "draft": True}]):
            html = site.render_downloads(site.select_releases(catalog))
            self.assertIn("after the first GitHub release", html)
            self.assertNotIn("/releases/download/", html)

    def test_incomplete_numbered_release_fails_instead_of_hiding_failure(self):
        complete = release()
        incomplete = release(2, published="2026-10-14T11:00:00Z")
        incomplete["assets"].pop()
        with self.assertRaisesRegex(ValueError, "incomplete"):
            site.select_releases([complete, incomplete])

    def test_external_or_non_uploaded_assets_fail(self):
        for field, value in (("browser_download_url", "https://example.com/fake.vsix"),
                             ("state", "starter"), ("digest", None), ("digest", "sha256:bad"),
                             ("size", 0)):
            with self.subTest(field=field, value=value):
                item = release()
                item["assets"][0][field] = value
                with self.assertRaisesRegex(ValueError, "invalid asset"):
                    site.select_releases([item])

    def test_unexpected_and_duplicate_assets_fail(self):
        for asset in ({"name": "private.zip"}, copy.deepcopy(release()["assets"][0])):
            item = release()
            item["assets"].append(asset)
            with self.assertRaisesRegex(ValueError, "unexpected release assets"):
                site.select_releases([item])

    def test_output_contains_only_the_public_page_assets(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "site"
            site.build([release()], output)
            self.assertEqual({p.name for p in output.iterdir()}, {"index.html", "style.css", "favicon.svg", "images"})
            html = (output / "index.html").read_text(encoding="utf-8")
            self.assertNotIn("<!-- RELEASE_DOWNLOADS -->", html)
            self.assertIn("https://airk.ioka.io/", html)
            self.assertEqual({p.name for p in (output / "images").iterdir()}, set(site.PRODUCT_SCREENSHOTS))
            for name, component in site.PRODUCT_SCREENSHOTS.items():
                self.assertEqual((output / "images" / name).read_bytes(), (site.ROOT / "docs" / component / "images" / name).read_bytes())
                self.assertIn(f'./images/{name}', html)

            self.assertEqual(html.count('aria-disabled="true">Installer not published'), 4)
            self.assertNotIn(">Download VSIX ", html)
            self.assertIn("marketplace.visualstudio.com/items?itemName=iokaio.airk", html)
            with self.assertRaises(FileExistsError):
                site.build([], output)


if __name__ == "__main__":
    unittest.main()
