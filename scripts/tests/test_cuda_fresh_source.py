# SPDX-License-Identifier: Apache-2.0
"""Pure controls for the guarded fresh CUDA input materialization stage."""

import hashlib
import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, call, patch


ROOT = Path(__file__).resolve().parents[2]
BUILD = ROOT / "scripts/cuda-static-runtime/build"
SPEC = importlib.util.spec_from_file_location("fresh_cuda_source", BUILD / "fresh_source.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
LOCK = json.loads((BUILD / "r8/cuda-runtime-producer-lock.json").read_text(encoding="utf-8"))
CAPI = "onnxruntime/core/session/onnxruntime_c_api.cc"


def pin(relative, value):
    return {"path": relative, "bytes": len(value), "sha256": hashlib.sha256(value).hexdigest()}


def mock_extract(archive, destination, expected):
    # The real extractor calls these by module name; deliberately no IO here.
    files = source_inventory(destination / "source")
    write_json(destination / "source-files.json", files)


class FreshCudaSourceTests(unittest.TestCase):
    def test_public_api_and_collector_chain_keeps_both_exports(self):
        source, frontend = ROOT / "literal/source", ROOT / "literal/frontend"
        stages = MODULE.patch_roster(source, frontend, LOCK)
        self.assertEqual([row[0] for row in stages], [
            "loader", "offline", "final-placement", "initialized-public-api", "final-graph",
            "frontend-arm64", "frontend-authenticated-imports",
        ])
        self.assertEqual(stages[3][4][0], {
            "path": CAPI,
            "before_sha256": "b8fb1b76bfb49b9d6963e20b1aa44d1f657782db1fd12c604e72d0762381aa2d",
            "after_sha256": "7acabdbefcd7ee3936305ff150fc2369c3d341b6838e10113435ebab03378ab8",
        })
        self.assertEqual(stages[4][4][0], {
            "path": CAPI,
            "before_sha256": "7acabdbefcd7ee3936305ff150fc2369c3d341b6838e10113435ebab03378ab8",
            "after_sha256": "db00479a06d0a1e8fadf4cc834ffe5a6da0b745ee2820b06a0821fb1dbeca97b",
        })
        self.assertEqual(stages[5][2], source / "cmake/patches/cudnn_frontend/cudnn_frontend_win_arm64_compat.patch")

    def test_msvc_cxx20_patch_is_separate_and_pinned_to_the_provider_target(self):
        source, frontend = ROOT / "literal/source", ROOT / "literal/frontend"
        original = MODULE.patch_roster(source, frontend, LOCK)
        corrected = MODULE.patch_roster(source, frontend, LOCK, msvc_ciso646_compat=True)
        self.assertEqual(original, corrected[:-1])
        stage, tree, path, expected, targets = corrected[-1]
        self.assertEqual((stage, tree), ("msvc-cuda-ciso646", source))
        body = path.read_bytes()
        self.assertEqual(hashlib.sha256(body).hexdigest(), expected)
        self.assertEqual([target["path"] for target in targets], ["cmake/onnxruntime_providers_cuda.cmake"])
        added = [line for line in body.splitlines()[3:] if line.startswith(b"+") and b"target_" in line]
        self.assertEqual(added, [
            b'+      target_compile_definitions(${target} PRIVATE '
            b'"$<$<COMPILE_LANGUAGE:CXX,CUDA>:_SILENCE_CXX20_CISO646_REMOVED_WARNING>")',
        ])

    def test_collector_uses_exact_reviewed_transport_only(self):
        raw = (BUILD.parent / "final-graph-metadata/collector.patch").read_bytes()
        self.assertEqual(hashlib.sha256(raw).hexdigest(),
                         "a1a95c9b4401e302d7046159f882fdafa735bf7b32542314f75b333b902870c9")
        normalized = MODULE.patch_transport("final-graph", raw)
        self.assertNotIn(b"\r", normalized)
        self.assertEqual(hashlib.sha256(normalized).hexdigest(),
                         "5222d2634878ac5e1c68264b6d521f0ca95df3a47397e3cf167b3ff7de844d58")
        self.assertEqual(MODULE.patch_transport("offline", b"a\r\n"), b"a\r\n")
        with self.assertRaisesRegex(ValueError, "reviewed LF projection"):
            MODULE.patch_transport("final-graph", raw + b"\n+extra\n")

    def test_patches_cannot_add_omit_or_change_an_unreviewed_file(self):
        before = [pin("one", b"one"), pin("two", b"two")]
        changed = [pin("one", b"patched"), before[1]]
        MODULE.verify_transformed_inventory(before, changed, {"one"})
        for after in (changed[:1], changed + [pin("extra", b"x")], [changed[0], pin("two", b"drift")]):
            with self.subTest(after=after), self.assertRaises(ValueError):
                MODULE.verify_transformed_inventory(before, after, {"one"})

    def test_extractor_outputs_are_audited_before_inventory_and_receipt_use(self):
        destination = ROOT / "literal"
        source = destination / "source"
        receipt = destination / "source-files.json"
        row = pin("file.cc", b"literal")
        events = []
        cpu = SimpleNamespace(
            extract_source=mock_extract,
            source_inventory=lambda path: events.append(("inventory", path)) or [row],
            write_json=lambda path, body: events.append(("saved", path, body)),
        )
        self.assertIsNone(mock_extract.__globals__.get("source_inventory"))
        with patch.object(Path, "rglob", return_value=[source / "nested", source / "nested/file.cc"]), \
                patch.object(MODULE.time, "monotonic", return_value=1):
            guarded = MODULE.audited_cpu(
                cpu, lambda paths, created, _: events.append(("audit", tuple(paths), created)), 120,
            )
            guarded.extract_source(destination / "source.tar.gz", destination, {})
        self.assertEqual(events, [
            ("audit", (source, source / "nested", source / "nested/file.cc"), True),
            ("inventory", source),
            ("audit", (receipt,), False),
            ("saved", receipt, [row]),
            ("audit", (receipt,), True),
        ])
        self.assertIsNone(mock_extract.__globals__.get("source_inventory"))

    def test_fresh_stage_calls_only_exact_archives_and_keeps_inventories(self):
        root = ROOT / "literal"
        source, mirror, frontend = root / "source", root / "source/mirror", root / "frontend"
        baseline = [pin(CAPI, b"base")]
        baseline.extend(pin(path, b"source") for path in (
            "cmake/external/helper_functions.cmake",
            "onnxruntime/core/platform/windows/env.cc",
            "onnxruntime/core/providers/cuda/cudnn_loader.cc",
            "onnxruntime/core/providers/cuda/cufft_loader.cc",
        ))
        final = [dict(baseline[0], sha256=MODULE.COLLECTOR_SOURCE), *baseline[1:]]
        frontend_rows = [pin("include/cudnn_frontend_shim.h", b"x"),
                         pin("include/cudnn_frontend_Tensor.h", b"y")]
        cpu_leaf = dict(pin("cpu.zip", b"cpu"), name="cpu")
        deps = [cpu_leaf, *LOCK["dependencies"]]
        names = {row["name"]: root / (row["name"] + ".zip") for row in deps}
        mirror_rows = sorted([
            {"path": row["name"] + ".zip", "bytes": row["bytes"], "sha256": row["sha256"]}
            for row in deps
        ], key=lambda row: row["path"])
        cpu = SimpleNamespace(
            VERSION="1.30.0", COMMIT=LOCK["upstream_commit"], LOCK={"source": {}},
            plain=Mock(side_effect=lambda path, **_: path), verify_file=Mock(),
            selected_dependencies=Mock(return_value=[cpu_leaf]),
            mirror_relative=lambda row: row["name"] + ".zip",
            verify_source=Mock(return_value=baseline),
        )
        checked = SimpleNamespace(
            extract_source=Mock(),
            source_inventory=Mock(side_effect=lambda path, **_: {
                source: final, frontend: frontend_rows, mirror: mirror_rows,
            }[path]),
        )
        r8 = SimpleNamespace(verify_mirror_closure=Mock(),
                             safe_frontend=Mock(return_value={"files_before_patches": frontend_rows}))
        run, guard = Mock(), Mock()
        with patch.object(Path, "exists", return_value=False), patch.object(Path, "is_dir", return_value=True), \
                patch.object(MODULE, "check_deadline"), patch.object(MODULE, "audited_cpu", return_value=checked), \
                patch.object(MODULE, "apply_patch", side_effect=lambda *_: {}) as apply, \
                patch.object(MODULE, "ensure_directory"), patch.object(MODULE, "mirror_inputs") as mirror_inputs:
            value = MODULE.materialize(
                destination=root, source_archive=root / "archive.tgz", dependency_archives=names,
                lock=LOCK, cpu=cpu, r8=r8, patch_tool=root / "patch.exe",
                run_patch=run, guard_paths=guard, deadline=120,
            )
        self.assertEqual(mirror_inputs.call_args_list, [
            call(cpu, mirror, [cpu_leaf], names, guard, 120),
            call(cpu, mirror, LOCK["dependencies"], names, guard, 120),
        ])
        self.assertEqual(apply.call_count, 7)
        self.assertEqual(value["mirror_inventory"], mirror_rows)
        self.assertEqual(value["source_inventory"], final)
        self.assertEqual(value["frontend_inventory"], frontend_rows)
        self.assertEqual(value["collector_source"]["path"], str(source / CAPI))
        checked.extract_source.assert_called_once_with(root / "archive.tgz", root, {})
        run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
