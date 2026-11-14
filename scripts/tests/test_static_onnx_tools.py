# SPDX-License-Identifier: Apache-2.0
"""Pinned build-tool policy tests; fixtures never compile Ninja or ORT."""
import copy
from contextlib import nullcontext
import hashlib
import importlib.util
import io
import json
import os
import sys
from pathlib import Path
import tarfile
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("airk_static_tools", Path(__file__).parents[1] / "provision-static-onnx-tools.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
CORE = MODULE.core_module()


class StaticToolTests(unittest.TestCase):
    def vendor_archive(self, root, target="A", cycle=False):
        archive = root / "cmake.tar.gz"
        prefix = "CMake.app/Contents/Frameworks/QtCore.framework/"
        with tarfile.open(archive, "w:gz") as output:
            member = tarfile.TarInfo("tool/" + prefix + "Versions/A/QtCore")
            member.size = 6
            output.addfile(member, io.BytesIO(b"vendor"))
            for name, reference in ((prefix + "Versions/Current", "../QtCore" if cycle else target),
                                    (prefix + "QtCore", "Versions/Current/QtCore")):
                member = tarfile.TarInfo("tool/" + name)
                member.type, member.linkname = tarfile.SYMTYPE, reference
                output.addfile(member)
        return archive, {"archive_root": "tool", "program": "CMake.app/Contents/bin/cmake",
                         "bytes": archive.stat().st_size, "sha256": CORE.digest(archive)}

    def test_vendor_framework_links_complete_additively_without_changing_regular_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            archive, row = self.vendor_archive(root)
            target = root / "existing"
            # Reproduce the old partial extraction: authenticated regular file
            # remains, but the signed vendor framework's two links were omitted.
            regular = target / "CMake.app/Contents/Frameworks/QtCore.framework/Versions/A/QtCore"
            regular.parent.mkdir(parents=True)
            regular.write_bytes(b"vendor")
            before = CORE.digest(regular)
            try:
                entries = MODULE.extract(archive, target, row, CORE)
            except OSError as error:
                if os.name == "nt" and getattr(error, "winerror", None) == 1314:
                    self.skipTest("host does not permit unprivileged vendor symlink fixture")
                raise
            self.assertEqual(sum(entry["kind"] == "vendor_symlink" for entry in entries), 2)
            self.assertEqual(CORE.digest(regular), before)
            alias = target / "CMake.app/Contents/Frameworks/QtCore.framework/QtCore"
            self.assertEqual(alias.read_bytes(), b"vendor")
            MODULE.extract(archive, target, row, CORE)
            self.assertEqual(CORE.digest(regular), before)
            alias.unlink()
            regular.write_bytes(b"change")
            with self.assertRaises(ValueError): MODULE.extract(archive, target, row, CORE)
            self.assertFalse(alias.is_symlink(), "corrupt original bytes refuse before adding any links")

    def test_vendor_link_escape_cycle_missing_target_and_wrong_existing_type_refuse(self):
        for target, cycle in (("../../../../../../outside", False), ("missing", False), ("A", True)):
            with tempfile.TemporaryDirectory() as temporary:
                archive, row = self.vendor_archive(Path(temporary), target, cycle)
                with self.assertRaises(ValueError): MODULE.archive_entries(archive, row, CORE)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            archive, row = self.vendor_archive(root)
            entries = MODULE.archive_entries(archive, row, CORE)
            tree = root / "partial"
            regular = tree / "CMake.app/Contents/Frameworks/QtCore.framework/Versions/A/QtCore"
            regular.parent.mkdir(parents=True)
            regular.write_bytes(b"vendor")
            (regular.parent.parent / "Current").mkdir()
            with self.assertRaisesRegex(ValueError, "another type"):
                MODULE.complete_vendor_links(tree, entries, CORE)

    def archive(self, root, link=None):
        path = root / "tool.tar.gz"
        with tarfile.open(path, "w:gz") as output:
            member = tarfile.TarInfo("tool/bin/program")
            content = b"original pinned executable fixture"
            member.size, member.mode = len(content), 0o755
            output.addfile(member, io.BytesIO(content))
            if link:
                member = tarfile.TarInfo("tool/bin/alias")
                member.type, member.linkname = tarfile.SYMTYPE, link
                output.addfile(member)
        return path, {"archive_root": "tool", "bytes": path.stat().st_size, "sha256": CORE.digest(path)}

    def test_exact_official_cmake_and_ninja_source_pins(self):
        MODULE.validate_lock(CORE)
        self.assertEqual(MODULE.LOCK["cmake"]["Darwin"]["sha256"], "330b9514f5112e5ed4fb08b8b05803b776fd9b539a6ae12927d14dcc0ee2ba8d")
        self.assertEqual(MODULE.LOCK["ninja"]["sha256"], "373bb33a8db2769b825a771e5d25b11d79ef5e41a934b870487257c27b187180")
        for change in (lambda lock: lock.update(cmake_version="4.0.0"), lambda lock: lock["cmake"]["Linux"].update(url="https://example.invalid/tool")):
            lock = copy.deepcopy(MODULE.LOCK)
            change(lock)
            with patch.object(MODULE, "LOCK", lock), self.assertRaises(ValueError):
                MODULE.validate_lock(CORE)

    def test_tar_extraction_is_one_forward_pass_with_sorted_identical_receipts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            archive = root / "out-of-order.tar.gz"
            with tarfile.open(archive, "w:gz") as output:
                for name, content in (("z-last", b"z"), ("a-first", b"a"), ("middle", b"m")):
                    member = tarfile.TarInfo("tool/" + name)
                    member.size, member.mode = len(content), 0o644
                    output.addfile(member, io.BytesIO(content))
            row = {"archive_root": "tool", "bytes": archive.stat().st_size, "sha256": CORE.digest(archive)}
            expected = MODULE.archive_entries(archive, row, CORE)
            original_open = tarfile.open
            with patch.object(MODULE.tarfile, "open", wraps=original_open) as opened:
                entries = MODULE.extract(archive, root / "extracted", row, CORE)
            self.assertEqual(entries, expected)
            self.assertEqual([entry["path"] for entry in entries], ["a-first", "middle", "z-last"])
            self.assertEqual(opened.call_count, 1)
            self.assertEqual(opened.call_args.kwargs.get("mode"), "r|gz")

    def test_streamed_tar_retains_root_duplicate_type_and_budget_checks(self):
        cases = [
            ("wrong-root", [("other/file", tarfile.REGTYPE), ("tool/ok", tarfile.REGTYPE)], {}, "root differs"),
            ("duplicate", [("tool/file", tarfile.REGTYPE)] * 2, {}, "Duplicate"),
            ("root-file", [("tool", tarfile.REGTYPE)], {}, "archive root"),
            ("root-duplicate", [("tool", tarfile.DIRTYPE)] * 2, {}, "archive root"),
            ("special", [("tool/fifo", tarfile.FIFOTYPE)], {}, "type/size"),
            ("file-budget", [("tool/file", tarfile.REGTYPE)], {"MAX_FILE": 0}, "type/size"),
            ("total-budget", [("tool/a", tarfile.REGTYPE), ("tool/b", tarfile.REGTYPE)], {"MAX_EXPANDED": 1}, "expansion"),
            ("member-budget", [("tool", tarfile.DIRTYPE), ("tool/a", tarfile.REGTYPE)], {"MAX_MEMBERS": 1}, "member count"),
        ]
        for name, members, limits, message in cases:
            with self.subTest(case=name), tempfile.TemporaryDirectory() as directory:
                root = Path(directory).resolve()
                archive = root / "invalid.tar.gz"
                with tarfile.open(archive, "w:gz") as output:
                    for member_name, kind in members:
                        member = tarfile.TarInfo(member_name)
                        member.type = kind
                        content = b"x" if kind == tarfile.REGTYPE else b""
                        member.size = len(content)
                        output.addfile(member, io.BytesIO(content))
                row = {"archive_root": "tool", "bytes": archive.stat().st_size, "sha256": CORE.digest(archive)}
                with patch.multiple(MODULE, **limits) if limits else nullcontext():
                    with self.assertRaisesRegex(ValueError, message):
                        MODULE.extract(archive, root / "failed", row, CORE)

    def test_archive_internal_links_are_omitted_and_outside_links_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            archive, row = self.archive(root, link="program")
            entries = MODULE.extract(archive, root / "extracted", row, CORE)
            self.assertTrue(any(entry["kind"] == "omitted_link" for entry in entries))
            self.assertFalse((root / "extracted/bin/alias").exists())
            self.assertEqual((root / "extracted/bin/program").read_bytes(), b"original pinned executable fixture")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            archive, row = self.archive(root, link="../../outside")
            with self.assertRaises(ValueError):
                MODULE.extract(archive, root / "extracted", row, CORE)
            # A later invalid streamed member leaves earlier verified bytes as
            # failed-attempt evidence, and never creates its escaping target.
            self.assertFalse((root / "outside").exists())
            self.assertEqual((root / "extracted/bin/program").read_bytes(), b"original pinned executable fixture")

    def test_existing_tree_requires_exact_sealed_bytes_and_roster(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            archive, row = self.archive(root)
            target = root / "extracted"
            MODULE.extract(archive, target, row, CORE)
            MODULE.extract(archive, target, row, CORE)
            extra = target / "extra.cmake"
            extra.write_text("unknown injected module")
            with self.assertRaises(ValueError):
                MODULE.extract(archive, target, row, CORE)
            self.assertEqual(extra.read_text(), "unknown injected module")

    def test_read_only_tools_verification_never_creates_owner_or_tree(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve() / "owned"
            CORE.owned_destination(root, "x86_64-pc-windows-msvc", create=True)
            with self.assertRaises(ValueError):
                MODULE.verify_tools(root, "x86_64-pc-windows-msvc", CORE)
            self.assertFalse((root / "tools").exists())

    def test_foreign_tools_directory_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve() / "owned"
            CORE.owned_destination(root, "x86_64-pc-windows-msvc", create=True)
            tools = root / "tools"
            tools.mkdir()
            (tools / "keep").write_text("user file")
            with self.assertRaises(ValueError):
                MODULE.setup_owner(root, "x86_64-pc-windows-msvc", CORE, create=True)
            self.assertEqual((tools / "keep").read_text(), "user file")

    def test_receipt_cannot_redirect_to_an_external_same_hash_binary(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            cmake = root / "cmake/bin/cmake.exe"
            ninja = root / "ninja-build/ninja.exe"
            for path in (cmake, ninja):
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"identical harmless executable fixture")
            receipt = {"cmake": CORE.tool_record(cmake), "ninja": CORE.tool_record(ninja)}
            MODULE.bound_tool_paths(root, "x86_64-pc-windows-msvc", receipt, CORE)
            outside = root / "outside.exe"
            outside.write_bytes(cmake.read_bytes())
            receipt["cmake"] = CORE.tool_record(outside)
            self.assertEqual(receipt["cmake"]["sha256"], CORE.digest(cmake))
            with self.assertRaisesRegex(ValueError, "redirected"):
                MODULE.bound_tool_paths(root, "x86_64-pc-windows-msvc", receipt, CORE)

    def test_ninja_policy_disables_tests_and_ambient_re2c_and_retains_native_mac_minimum(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            options = SimpleNamespace(cxx=Path(sys.executable), vs_root=None, msvc_toolset="14.44.35207", windows_sdk="10.0.26100.0")
            with patch.object(CORE, "program_path", side_effect=lambda value, name: Path(value) if value else root / name):
                configure, build, _ = MODULE.ninja_commands(root, "aarch64-apple-darwin", root / "cmake", options, CORE)
            receipt = {"configure_argv": configure, "build_argv": build, "cmake": {"path": str(root / "cmake")}, "python": {"path": str(Path(sys.executable).resolve())}}
            MODULE.verify_ninja_policy(root, "aarch64-apple-darwin", receipt, CORE)
            self.assertIn("-DBUILD_TESTING=OFF", configure)
            self.assertIn("-DRE2C=OFF", configure)
            self.assertIn("-DCMAKE_OSX_ARCHITECTURES=arm64", configure)
            for change in (lambda r: r["configure_argv"].append("-DCMAKE_TOOLCHAIN_FILE=/unknown"),
                           lambda r: r["configure_argv"].append("-DRE2C=/ambient/re2c"),
                           lambda r: r["build_argv"].append("--clean-first")):
                crossed = copy.deepcopy(receipt)
                change(crossed)
                with self.assertRaises(ValueError):
                    MODULE.verify_ninja_policy(root, "aarch64-apple-darwin", crossed, CORE)

    def test_base_cli_exposes_explicit_download_tools_without_implicit_cargo(self):
        with patch.object(CORE, "prepare") as prepare, patch.object(CORE, "build") as build, patch("sys.stderr", new_callable=io.StringIO):
            with self.assertRaises(SystemExit):
                CORE.main(["--verify", "--download-tools", "--target", "x86_64-pc-windows-msvc"])
            prepare.assert_not_called()
            build.assert_not_called()


if __name__ == "__main__":
    unittest.main()
