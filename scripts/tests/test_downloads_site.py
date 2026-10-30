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
    def test_clean_version_tags_and_legacy_downloads(self):
        current = release(preview=False, published="2026-10-17T12:00:00Z")
        old_tag = current["tag_name"]
        current["tag_name"] = "v0.1.0"
        for asset in current["assets"]:
            asset["browser_download_url"] = asset["browser_download_url"].replace(old_tag, "v0.1.0")
        records = site.select_releases([release(preview=False), current])
        self.assertEqual([record["tag"] for record in records], ["v0.1.0"])
        html = site.render_downloads(records)
        self.assertIn("/v0.1.0/airk-0.1.0-win32-x64.vsix", html)
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
            for platform, *_ in site.PLATFORMS:
                self.assertIn(f"/v0.1.0-build.{number}/airk-0.1.0-{platform}.vsix", html)
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

            self.assertEqual(html.count(">Download VSIX "), 4)
            with self.assertRaises(FileExistsError):
                site.build([], output)


if __name__ == "__main__":
    unittest.main()
