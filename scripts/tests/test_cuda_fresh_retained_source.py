# SPDX-License-Identifier: Apache-2.0
"""Inert reconstruction controls for a retained fresh configure source tree."""

import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import stat
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch
import zipfile

from scripts.tests import test_cuda_fresh_projection as fixtures

SCRIPTS = Path(__file__).resolve().parents[1]
BUILD = SCRIPTS / "cuda-static-runtime/build"
retained = fixtures.load(BUILD / "fresh_retained_source.py", "retained_source_controls")
source_owner = fixtures.load(BUILD / "fresh_source.py", "retained_original_source_controls")


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def make_frontend(link=b"../skills", kind=stat.S_IFLNK):
    output = io.BytesIO()
    root = "cudnn-frontend-1.27.0/"
    with zipfile.ZipFile(output, "w") as zipped:
        zipped.writestr(root, "")
        zipped.writestr(root + "include/frontend.h", b"original header")
        excluded = zipfile.ZipInfo(root + ".claude/skills")
        excluded.create_system = 3
        excluded.external_attr = (kind | 0o777) << 16
        zipped.writestr(excluded, link)
    return output.getvalue()


class RetainedSource(unittest.TestCase):
    def setUp(self):
        self.root = SCRIPTS / "inert-source-reconstruction"
        self.source = self.root / "source"
        self.frontend = self.root / "frontend"
        self.mirror = self.source / "mirror"
        self.snapshots = self.root / "patch-inputs"
        self.baseline = self.root / "source-files.json"
        self.archive = self.root / "ort.tar.gz"
        self.cpu_archive = self.root / "cpu.zip"
        self.frontend_archive = self.root / "frontend.zip"
        self.originals = SCRIPTS / "inert-originals"

        cpu_body = b"pinned offline bytes"
        frontend_body = make_frontend()
        self.cpu_dep = {
            **fixtures.pin("cpu.zip", cpu_body), "name": "cpu",
            "url": "https://github.com/example/project/archive/snapshot.zip",
            "upstream_sha1": hashlib.sha1(cpu_body).hexdigest(),
        }
        self.lock = {
            "target": "x86_64-pc-windows-msvc", "ort_version": "1.30.0", "upstream_commit": "approved-rev",
            "dependencies": [self.frontend_dep(frontend_body)],
            "excluded_link": {"path": ".claude/skills", "bytes": 9, "target_sha256": digest(b"../skills")},
        }
        deps = f'cpu;{self.cpu_dep["url"]};{self.cpu_dep["upstream_sha1"]}\n'.encode()
        base = {"VERSION_NUMBER": b"1.30.0\n", "cmake/deps.txt": deps,
                "collector.cc": b"base\n", "unchanged": b"inert"}
        self.before = sorted([fixtures.pin(name, data) for name, data in base.items()], key=lambda row: row["path"])
        baseline = json.dumps(self.before).encode()
        prefix, final = b"old\n", b"old\nnew\n"
        self.final_patch = b"--- a/collector.cc\n+++ b/collector.cc\n@@ -1,1 +1,2 @@\n old\n+new\n"
        self.files = {
            self.archive: b"reviewed source tar",
            self.cpu_archive: cpu_body, self.frontend_archive: frontend_body, self.baseline: baseline,
            **{self.source / name: data for name, data in base.items()},
            self.source / "collector.cc": final,
            self.frontend / "include/frontend.h": b"patched header",
            self.mirror / "cpu.zip": cpu_body, self.mirror / "cudnn_frontend.zip": frontend_body,
        }
        self.specs = [
            self.spec("initialized-public-api", self.source, b"previous patch", "collector.cc", base["collector.cc"], prefix),
            self.spec("final-graph", self.source, self.final_patch, "collector.cc", prefix, final),
            self.spec("frontend-arm64", self.frontend, b"frontend patch", "include/frontend.h",
                      b"original header", b"patched header"),
        ]
        self.cpu = SimpleNamespace(
            VERSION="1.30.0", COMMIT="approved-rev", MAX_JSON=16 << 20,
            LOCK={"source": {**fixtures.pin("source.tar.gz", self.files[self.archive]),
                             "inventory_sha256": digest(baseline), "files": len(self.before)},
                  "source_metadata": [fixtures.pin("cmake/deps.txt", deps)]},
            selected_dependencies=lambda target: [self.cpu_dep],
            mirror_relative=lambda row: row["name"] + ".zip", plain=lambda path, **_: Path(path),
            relative_name=self.relative_name,
        )
        self.sources = SimpleNamespace(
            MAX_PATCH_BYTES=1 << 20, TARGET="x86_64-pc-windows-msvc", CAPI="collector.cc",
            PUBLIC_API_SOURCE=digest(prefix), COLLECTOR_SOURCE=digest(final),
            patch_roster=lambda *_: self.specs,
            patch_transport=lambda name, raw: raw.replace(b"\r\n", b"\n") if name == "final-graph" else raw,
            verify_transformed_inventory=source_owner.verify_transformed_inventory,
        )
        self.events = []
        self.deadline = 720
        self.expired = False
        self.guard = Mock(side_effect=self.audit)
        self.pins = SimpleNamespace(parse_json=json.loads, pin=self.pin, deadline_check=self.check)
        self.folders = {self.source, self.source / "cmake", self.frontend, self.frontend / "include",
                        self.mirror, self.snapshots}

    def spec(self, stage, tree, raw, target, before, after):
        original = self.originals / (stage + ".patch")
        snapshot = self.snapshots / (stage + ".patch")
        self.files[original] = raw.replace(b"\n", b"\r\n") if stage == "final-graph" else raw
        self.files[snapshot] = raw
        return (stage, tree, original, digest(self.files[original]),
                [{"path": target, "before_sha256": digest(before), "after_sha256": digest(after)}])

    def relative_name(self, name):
        if name.startswith("/") or "\\" in name or any(part in ("", ".", "..") for part in name.split("/")):
            raise ValueError("unsafe ZIP path")
        return PurePosixPath(name)

    def frontend_dep(self, raw):
        with zipfile.ZipFile(io.BytesIO(raw)) as zipped:
            return {
                **fixtures.pin("frontend.zip", raw), "name": "cudnn_frontend",
                "upstream_sha1": hashlib.sha1(raw).hexdigest(),
                "entries": len(zipped.infolist()), "expanded_bytes": sum(info.file_size for info in zipped.infolist()),
            }

    def pin(self, path, deadline):
        self.check(deadline)
        self.events.append(("pin", path))
        return fixtures.pin(path, self.files[path])

    def audit(self, paths, created, deadline):
        self.assertTrue(created)
        self.check(deadline)
        self.events.append(("audit", tuple(paths)))

    def check(self, deadline):
        self.assertEqual(deadline, self.deadline)
        if self.expired:
            raise TimeoutError("original elapsed")

    def open_path(self, path, mode):
        self.assertEqual(mode, "rb")
        self.events.append(("read", path))
        return io.BytesIO(self.files[path])

    def gather(self, root, _pattern):
        return sorted(path for path in self.files.keys() | self.folders if path != root and path.is_relative_to(root))

    def reconstruct(self):
        with patch.object(Path, "stat", autospec=True, side_effect=lambda path: SimpleNamespace(st_size=len(self.files[path]))), \
                patch.object(Path, "open", autospec=True, side_effect=self.open_path), \
                patch.object(Path, "rglob", autospec=True, side_effect=self.gather), \
                patch.object(Path, "is_dir", autospec=True, side_effect=self.folders.__contains__), \
                patch.object(Path, "is_file", autospec=True, side_effect=self.files.__contains__), \
                patch.object(Path, "iterdir", autospec=True, side_effect=lambda root: self.gather(root, "*")):
            return retained.reconstruct(
                destination=self.root, source_archive=self.archive,
                dependency_archives={"cpu": self.cpu_archive, "cudnn_frontend": self.frontend_archive},
                lock=self.lock, cpu=self.cpu, r8=SimpleNamespace(), sources=self.sources, pins=self.pins,
                guard_paths=self.guard, deadline=self.deadline,
            )

    def test_reconstructs_exact_transformed_inventories_and_patch_transports(self):
        result = self.reconstruct()
        expected_source = [
            fixtures.pin(row["path"], self.files[self.source / row["path"]]) for row in self.before
        ]
        self.assertEqual(result["source_inventory"], expected_source)
        self.assertEqual(result["frontend_inventory"], [fixtures.pin("include/frontend.h", b"patched header")])
        self.assertEqual(result["mirror_inventory"], [
            fixtures.pin("cpu.zip", self.files[self.cpu_archive]),
            fixtures.pin("cudnn_frontend.zip", self.files[self.frontend_archive]),
        ])
        self.assertEqual(result["collector_source"], fixtures.pin(self.source / "collector.cc", b"old\nnew\n"))
        self.assertEqual(result["frontend"], self.frontend)
        self.assertEqual(result["patch_inputs"], [
            {"stage": name, "original": fixtures.pin(original, self.files[original]),
             "snapshot": fixtures.pin(self.snapshots / (name + ".patch"), self.files[self.snapshots / (name + ".patch")])}
            for name, _, original, _, _ in self.specs
        ])
        self.assertLess(self.events.index(("audit", (self.baseline,))), self.events.index(("read", self.baseline)))
        for root in (self.source, self.frontend):
            roster = tuple([root, *self.gather(root, "*")])
            self.assertLess(self.events.index(("audit", roster)), self.events.index(("pin", next(
                path for path in roster if path in self.files
            ))))

    def test_refuses_unpinned_base_inventory_and_unapproved_unmodified_file(self):
        prior = self.files[self.baseline]
        self.files[self.baseline] = prior + b"\n"
        with self.assertRaisesRegex(ValueError, "ORT base inventory hash"):
            self.reconstruct()
        self.files[self.baseline] = prior
        self.files[self.source / "unchanged"] = b"changed"
        with self.assertRaisesRegex(ValueError, "undeclared source file"):
            self.reconstruct()

    def test_refuses_mirror_and_frontend_input_drift(self):
        self.files[self.mirror / "extra.zip"] = b"extra"
        with self.assertRaisesRegex(ValueError, "offline mirror closure"):
            self.reconstruct()
        self.files.pop(self.mirror / "extra.zip")
        self.files[self.frontend / "extra.h"] = b"extra"
        with self.assertRaisesRegex(ValueError, "changed the approved source roster"):
            self.reconstruct()

    def test_refuses_exact_transport_and_before_hash_chain_drift(self):
        snapshot = self.snapshots / "final-graph.patch"
        self.files[snapshot] = self.final_patch + b"\n"
        with self.assertRaisesRegex(ValueError, "patch transport"):
            self.reconstruct()
        self.files[snapshot] = self.final_patch
        self.specs[1][4][0]["before_sha256"] = "f" * 64
        with self.assertRaisesRegex(ValueError, "before hash chain"):
            self.reconstruct()

    def test_terminal_hash_and_collector_append_prefix_are_independent_checks(self):
        modified = b"bad\nnew\n"
        self.files[self.source / "collector.cc"] = modified
        with self.assertRaisesRegex(ValueError, "terminal patch target"):
            self.reconstruct()
        self.specs[1][4][0]["after_sha256"] = digest(modified)
        self.sources.COLLECTOR_SOURCE = digest(modified)
        with self.assertRaisesRegex(ValueError, "authenticated placement prefix"):
            self.reconstruct()

    def test_refuses_foreign_frontend_link_even_when_archive_pin_is_updated(self):
        archive = make_frontend(b"../escape")
        self.files[self.frontend_archive] = archive
        self.lock["dependencies"] = [self.frontend_dep(archive)]
        with self.assertRaisesRegex(ValueError, "frontend ZIP link target"):
            self.reconstruct()

    def test_never_refreshes_original_deadline_or_reads_before_first_clock_check(self):
        self.expired = True
        with self.assertRaisesRegex(TimeoutError, "original elapsed"):
            self.reconstruct()
        self.assertFalse(self.events)


if __name__ == "__main__":
    unittest.main()
