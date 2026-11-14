# SPDX-License-Identifier: Apache-2.0
"""Static source build policy/security tests; no ORT/Cargo build or network."""
import copy
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile
from unittest.mock import patch
from contextlib import ExitStack
from types import SimpleNamespace

SPEC = importlib.util.spec_from_file_location("airk_static_ort", Path(__file__).parents[1] / "provision-static-onnx-runtime.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
WINDOWS = "x86_64-pc-windows-msvc"


def tools():
    return {"cmake": {"path": "C:/tools/cmake.exe"}, "ninja": {"path": "C:/tools/ninja.exe"},
            "patch": {"path": "C:/tools/patch.exe"}, "python": {"path": "C:/tools/python.exe"},
            "compiler": {"path": "/tools/clang"}, "cxx": {"path": "/tools/clang++"},
            "windows_sdk": "10.0.26100.0", "msvc_toolset": "14.44.35207",
            "macos_deployment_target": "13.3", "macos_sdk": "/tools/MacOSX.sdk"}


class StaticOrtPolicyTests(unittest.TestCase):
    def test_fresh_and_completed_build_handoff_aggregates_but_verify_never_mutates(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            aggregate = SimpleNamespace(aggregate=unittest.mock.Mock(return_value={"aggregate": True}),
                                        verify=unittest.mock.Mock(return_value={"verified": True}))
            with patch.object(MODULE, "load_script", return_value=aggregate) as loader:
                self.assertEqual(MODULE.finish_build(root, WINDOWS), {"aggregate": True})
                self.assertEqual(loader.call_args.args, ("static-onnx-aggregate.py",))
                self.assertIs(aggregate.aggregate.call_args.args[2].verify_original_build, MODULE.verify_original_build)
                (root / "build-receipt.json").write_text("original completed evidence")
                with patch.object(MODULE, "owned_destination", return_value=root), patch.object(MODULE, "run_owned") as compile:
                    self.assertEqual(MODULE.build(SimpleNamespace(), root, WINDOWS), {"aggregate": True})
                    compile.assert_not_called()
                self.assertEqual((root / "build-receipt.json").read_text(), "original completed evidence")
            with patch.object(MODULE, "verify_original_build", return_value={"original": True}), patch.object(MODULE, "load_script") as loader:
                with self.assertRaisesRegex(ValueError, "--aggregate-static"):
                    MODULE.verify_build(root, WINDOWS)
                loader.assert_not_called()
                self.assertFalse((root / "aggregate").exists())
                (root / "static-aggregate-receipt.json").write_text("original aggregate evidence")
                loader.return_value = aggregate
                self.assertEqual(MODULE.verify_build(root, WINDOWS), {"verified": True})
                aggregate.verify.assert_called_once()

    def test_static_provisioner_uses_python39_compatible_recipe_and_refuses_older_hosts(self):
        import ast
        for name in ("provision-static-onnx-runtime.py", "provision-static-onnx-tools.py", "darwin-waitid.py"):
            ast.parse((Path(__file__).parents[1] / name).read_text(encoding="utf-8"), feature_version=(3, 9))
        options = SimpleNamespace(cmake=sys.executable, ninja=sys.executable, cc=sys.executable,
                                  cxx=sys.executable, patch=sys.executable, static_tools=None)
        for version in ((3, 8, 0), (3, 9, 6)):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                def run(argv, cwd, log, timeout):
                    log.write_text("compiler fixture version\n")
                    return dict(exit_code=0, timed_out=False, log_oversized=False, owned_descendants_settled=True)
                with patch.object(MODULE, "host_target", return_value="x86_64-unknown-linux-gnu"), patch.object(MODULE.sys, "version_info", version), patch.object(MODULE, "probe_version", return_value={}), patch.object(MODULE, "run_owned", side_effect=run):
                    if version < (3, 9):
                        with self.assertRaisesRegex(ValueError, "Python3.9"):
                            MODULE.toolchain(options, root, "x86_64-unknown-linux-gnu")
                    else:
                        self.assertIn("python", MODULE.toolchain(options, root, "x86_64-unknown-linux-gnu"))

    def test_repeated_tool_probes_preserve_original_logs_and_keep_build_stage_exclusion(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / "logs").mkdir()
            original = root / "logs/tool-cmake.log"
            original.write_bytes(b"retained failed ownership probe evidence")
            observed = []
            def run(argv, cwd, log, timeout):
                with log.open("xb") as output:
                    output.write(b"cmake version 3.31.6\n")
                observed.append(log)
                return dict(exit_code=0, timed_out=False, log_oversized=False,
                            owned_descendants_settled=True, log=str(log))
            with patch.object(MODULE, "run_owned", side_effect=run):
                for _ in range(2):
                    result = MODULE.probe_version(Path(sys.executable), "3.31.6", root, "cmake")
                    self.assertEqual(result["probe"]["log"], str(observed[-1]))
            self.assertEqual(original.read_bytes(), b"retained failed ownership probe evidence")
            self.assertEqual(len(set(observed)), 2)
            self.assertTrue(all(path.read_bytes() == b"cmake version 3.31.6\n" for path in observed))
            for name in ("compiler", "cxx", "sdk"):
                first, second = MODULE.probe_log(root, name), MODULE.probe_log(root, name)
                self.assertNotEqual(first, second)
            # Generic command logging still refuses an existing build log;
            # this change never authorizes retrying a configure/build attempt.
            with self.assertRaises(FileExistsError):
                MODULE.run_owned([sys.executable, "--version"], root, original, 15)

    def test_fresh_static_producer_keeps_default_all_and_adds_excluded_re2(self):
        # The observed Ninja graph proves session+re2 misses nine archives.
        # Exercise actual orchestration without starting a compiler or tools.
        with tempfile.TemporaryDirectory() as directory, ExitStack() as stack:
            root = Path(directory).resolve()
            stack.enter_context(patch.object(MODULE, "owned_destination", return_value=root))
            stack.enter_context(patch.object(MODULE, "verify_source"))
            stack.enter_context(patch.object(MODULE, "toolchain", return_value=tools()))
            stack.enter_context(patch.object(MODULE, "digest", return_value="a" * 64))
            stack.enter_context(patch.object(MODULE, "windows_command", side_effect=lambda command, *_: command))
            invoked = stack.enter_context(patch.object(MODULE, "run_owned", side_effect=lambda *args: {
                "exit_code": 0, "timed_out": False, "log_oversized": False, "owned_descendants_settled": True}))
            stack.enter_context(patch.object(MODULE, "library_inventory", return_value=[]))
            stack.enter_context(patch.object(MODULE, "verify_cmake_cache"))
            stack.enter_context(patch.object(MODULE, "collect_notices", return_value=[]))
            stack.enter_context(patch.object(MODULE, "finish_build", return_value={"verified": True}))
            self.assertEqual(MODULE.build(SimpleNamespace(), root, WINDOWS), {"verified": True})
            command = invoked.call_args_list[1].args[0]
            index = command.index("--target")
            self.assertEqual(command[index + 1:index + 3], ["all", "re2"])
            self.assertEqual(command[-2:], ["--parallel", "2"])
            self.assertNotIn("onnxruntime_session", command)
            self.assertEqual(MODULE.read_json(root / "build-receipt.json")["build_argv"], command)

    def test_official_pins_roster_and_source_seal_are_complete(self):
        MODULE.validate_lock()
        self.assertEqual(MODULE.LOCK["ort_version"], "1.30.0")
        self.assertEqual(MODULE.LOCK["source"]["files"], 11046)
        self.assertEqual(MODULE.LOCK["source"]["sha256"], "1b4ec55fa26ee8fe622ea256894dfe9381bdfa99d41489994a1961720f556c48")
        self.assertEqual(len(MODULE.selected_dependencies(WINDOWS)), 14)
        self.assertEqual(len(MODULE.selected_dependencies("aarch64-apple-darwin")), 15)
        for change in (lambda lock: lock.update(ort_version="1.22.0"),
                       lambda lock: lock["dependencies"].pop(),
                       lambda lock: lock["source"].update(url="https://example.invalid/source"),
                       lambda lock: lock["dependencies"][0].update(upstream_sha1="x" * 40)):
            changed = copy.deepcopy(MODULE.LOCK)
            change(changed)
            with self.assertRaises(ValueError):
                MODULE.validate_lock(changed)

    def test_static_full_operator_and_cpu_baseline_are_explicit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            command = MODULE.configure_argv(root, WINDOWS, tools())
            for name in MODULE.OFF_OPTIONS:
                self.assertIn("-Donnxruntime_" + name + "=OFF", command)
            self.assertIn("-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL", command)
            self.assertIn("-DFETCHCONTENT_TRY_FIND_PACKAGE_MODE=NEVER", command)
            self.assertIn("-DPYTHON_EXECUTABLE=C:/tools/python.exe", command)
            self.assertIn("-DPython3_EXECUTABLE=C:/tools/python.exe", command)
            mac = MODULE.configure_argv(root, "aarch64-apple-darwin", tools())
            self.assertIn("-DCMAKE_OSX_ARCHITECTURES=arm64", mac)
            self.assertIn("-DCMAKE_OSX_DEPLOYMENT_TARGET=13.3", mac)
            linux = MODULE.configure_argv(root, "x86_64-unknown-linux-gnu", tools())
            self.assertIn("-DCMAKE_CXX_FLAGS=-march=x86-64 -mtune=generic", linux)
            self.assertFalse(any("march=native" in value or "Pyke" in value or "NuGet" in value for value in linux + mac + command))

    def test_actual_cmake_cache_must_match_static_policy(self):
        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory).resolve() / "CMakeCache.txt"
            cache.write_text("onnxruntime_BUILD_SHARED_LIB:BOOL=OFF\nCMAKE_BUILD_TYPE:STRING=Release\n")
            command = ["-Donnxruntime_BUILD_SHARED_LIB=OFF", "-DCMAKE_BUILD_TYPE=Release"]
            MODULE.verify_cmake_cache(cache, command)
            cache.write_text("onnxruntime_BUILD_SHARED_LIB:BOOL=ON\nCMAKE_BUILD_TYPE:STRING=Release\n")
            with self.assertRaises(ValueError):
                MODULE.verify_cmake_cache(cache, command)

    def test_unapproved_urls_redirects_and_traversals_are_refused(self):
        for url in ("http://github.com/a/b.zip", "https://github.com.evil.invalid/a", "https://u:p@github.com/a", "https://github.com/a?token=secret", "https://github.com/a/%2e%2e/b"):
            with self.assertRaises(ValueError):
                MODULE.archive_url(url)
        for name in ("../outside", "/absolute", "a/../b", "C:/outside", "a\\b", "a/./b", "a\ncommand"):
            with self.assertRaises(ValueError):
                MODULE.relative_name(name)
        with self.assertRaises(ValueError):
            MODULE.PinnedRedirect().redirect_request(None, None, 302, "", {}, "http://github.com/a")

    def test_unknown_destination_and_changed_owner_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            foreign = root / "foreign"
            foreign.mkdir()
            (foreign / "keep.txt").write_text("preserve")
            with self.assertRaises(ValueError):
                MODULE.owned_destination(foreign, WINDOWS, create=True)
            self.assertEqual((foreign / "keep.txt").read_text(), "preserve")
            owned = MODULE.owned_destination(root / "owned", WINDOWS, create=True)
            with self.assertRaises(ValueError):
                MODULE.owned_destination(owned, "aarch64-apple-darwin")

    def test_aliased_owned_inputs_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            first, second = root / "first", root / "second"
            first.write_bytes(b"original")
            os.link(first, second)
            with self.assertRaises(ValueError):
                MODULE.plain(first, regular=True)
            second.unlink()
            try:
                second.symlink_to(first)
            except OSError:
                return  # Windows symlink permission is not required for the hardlink assertion.
            with self.assertRaises(ValueError):
                MODULE.plain(second, regular=True)

    def test_missing_offline_input_never_downloads_or_creates_files(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(MODULE.urllib.request, "build_opener") as opener:
            path = Path(directory).resolve() / "absent/input.zip"
            with self.assertRaises(ValueError):
                MODULE.fetch(path, MODULE.LOCK["dependencies"][0])
            opener.assert_not_called()
            self.assertFalse(path.parent.exists())

    def test_corrupt_existing_archive_is_never_replaced(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(MODULE.urllib.request, "build_opener") as opener:
            path = Path(directory).resolve() / "input.zip"
            path.write_bytes(b"corrupt")
            with self.assertRaises(ValueError):
                MODULE.fetch(path, MODULE.LOCK["dependencies"][0], download=True)
            self.assertEqual(path.read_bytes(), b"corrupt")
            opener.assert_not_called()

    def test_source_archive_rejects_links_and_traversal_before_extraction(self):
        for name, kind in (("root/../escape", tarfile.REGTYPE), ("root/link", tarfile.SYMTYPE), ("root/hard", tarfile.LNKTYPE)):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                root = Path(directory).resolve()
                archive = root / "source.tar.gz"
                with tarfile.open(archive, "w:gz") as output:
                    member = tarfile.TarInfo(name)
                    member.type = kind
                    if kind == tarfile.REGTYPE:
                        member.size = 1
                        output.addfile(member, io.BytesIO(b"x"))
                    else:
                        member.linkname = "../escape"
                        output.addfile(member)
                row = {"members": 1, "unpacked_bytes": 1 if kind == tarfile.REGTYPE else 0, "archive_root": "root"}
                with self.assertRaises(ValueError):
                    MODULE.extract_source(archive, root, row)
                self.assertFalse((root / "source").exists())
                self.assertFalse((root.parent / "escape").exists())

    def test_tiny_import_libraries_cannot_qualify_as_static_components(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            build = root / "b/Release"
            build.mkdir(parents=True)
            for component in MODULE.STATIC_COMPONENTS:
                (build / ("onnxruntime_" + component + ".lib")).write_bytes(b"import" * 400)
            with self.assertRaisesRegex(ValueError, "genuine static"):
                MODULE.library_inventory(root, WINDOWS)

    def test_windows_cmd_rejects_shell_injection_before_writing(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            selected = tools()
            selected["vcvars"] = {"path": "C:/VS/vcvars64.bat"}
            with self.assertRaises(ValueError):
                MODULE.windows_command(["cmake", "-B", "safe & injected"], selected, root, "configure")
            self.assertFalse((root / "commands").exists())

    def test_environment_export_requires_owned_runner_file_and_fixed_safe_keys(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            env_file = root / "set-env"
            env_file.write_text("EXISTING=kept\n")
            result = {"environment": MODULE.legacy_environment(root)}
            with patch.dict(os.environ, {"GITHUB_ACTIONS": "true", "RUNNER_TEMP": str(root), "GITHUB_ENV": str(env_file)}):
                MODULE.github_environment(result)
                self.assertIn("ORT_SKIP_DOWNLOAD=1\n", env_file.read_text())
                original = env_file.read_bytes()
                crossed = copy.deepcopy(result)
                crossed["environment"]["ORT_LIB_LOCATION"] = "safe\nUNAPPROVED=value"
                with self.assertRaises(ValueError):
                    MODULE.github_environment(crossed)
                self.assertEqual(env_file.read_bytes(), original)
            with patch.dict(os.environ, {"GITHUB_ACTIONS": "false"}):
                with self.assertRaises(ValueError):
                    MODULE.github_environment(result)

    def test_unknown_legacy_receipt_cannot_adopt_arbitrary_existing_libraries(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            (root / "windows-static-build-result.json").write_text("{}")
            with self.assertRaises(ValueError):
                MODULE.legacy_inputs(root)
            self.assertFalse((root / "owner.json").exists())

    def test_legacy_checkout_preserves_raw_bytes_and_allows_only_exact_git_text_eols(self):
        self.assertEqual(MODULE.checkout_representation(b"original\n", b"original\n", "file"), "none")
        self.assertEqual(MODULE.checkout_representation(b"original\n", b"original\r\n", "file"), "git_lf_to_crlf")
        for upstream, compiled in ((b"original\n", b"changed\r\n"), (b"binary\0\n", b"binary\0\r\n"), (b"\xff\n", b"\xff\r\n"), (b"a\rb\n", b"a\rb\r\n")):
            with self.assertRaises(ValueError):
                MODULE.checkout_representation(upstream, compiled, "file")

    def test_failed_or_unsettled_stage_cannot_qualify(self):
        valid = {"exit_code": 0, "timed_out": False, "log_oversized": False, "owned_descendants_settled": True}
        self.assertTrue(MODULE.successful(valid))
        for key, value in (("exit_code", 1), ("timed_out", True), ("log_oversized", True), ("owned_descendants_settled", False)):
            self.assertFalse(MODULE.successful({**valid, key: value}))


class OwnedStaticBuildProcessTests(unittest.TestCase):
    def test_real_child_exit_is_recorded_and_parent_is_outside_owned_job(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            result = MODULE.run_owned([sys.executable, "-c", "print('owned child'); raise SystemExit(3)"], root, root / "child.log", 10)
            self.assertEqual(result["exit_code"], 3)
            self.assertTrue(result["owned_descendants_settled"])
            self.assertIn("owned child", (root / "child.log").read_text())
            if os.name == "nt":
                self.assertFalse(result["parent_assigned_to_job"])
                self.assertTrue(result["child_assigned_before_resume"])

    def test_real_owned_descendant_and_timeout_are_settled_without_process_sweeps(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            script = "import subprocess,sys,time; subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)']); print('descendant created',flush=True); time.sleep(60)"
            result = MODULE.run_owned([sys.executable, "-c", script], root, root / "timeout.log", 1)
            self.assertTrue(result["timed_out"])
            self.assertNotEqual(result["exit_code"], 0)
            self.assertTrue(result["owned_descendants_settled"])
            if os.name == "nt":
                self.assertTrue(result["owned_job_forced_termination"])


class StaticLicenseRetentionTests(unittest.TestCase):
    def test_in_archive_license_alias_retains_full_regular_text_and_refuses_escape_or_cycle(self):
        for target, expected in (("../LICENSE", b"Full original license"), ("../../outside", None), ("LICENSE", None)):
            with self.subTest(target=target), tempfile.TemporaryDirectory() as directory:
                archive = Path(directory) / "notices.zip"
                with zipfile.ZipFile(archive, "w") as zipped:
                    zipped.writestr("dependency/LICENSE", b"Full original license")
                    alias = zipfile.ZipInfo("dependency/python/LICENSE")
                    alias.create_system = 3
                    alias.external_attr = 0o120777 << 16
                    zipped.writestr(alias, target)
                with zipfile.ZipFile(archive) as zipped:
                    alias = zipped.getinfo("dependency/python/LICENSE")
                    if expected is None:
                        with self.assertRaises(ValueError):
                            MODULE.zip_notice_content(zipped, alias)
                    else:
                        content, source = MODULE.zip_notice_content(zipped, alias)
                        self.assertEqual(content, expected)
                        self.assertEqual(source, "dependency/LICENSE")

    def fixture(self, root):
        source = root / "source"
        downloads = root / "downloads"
        source.mkdir()
        downloads.mkdir()
        (source / "LICENSE").write_bytes(b"ORT full license fixture")
        (source / "ThirdPartyNotices.txt").write_bytes(b"ORT full third-party notice fixture")
        dependencies = []
        for name in sorted(MODULE.CPU_NAMES):
            path = downloads / (name + ".zip")
            with zipfile.ZipFile(path, "w") as zipped:
                if name == "mp11":
                    zipped.writestr("mp11/include/header.hpp", "// Distributed under the Boost Software License 1.0; see LICENSE_1_0.txt")
                else:
                    zipped.writestr(name + "/LICENSE", "Full license fixture: " + name)
                if name in ("microsoft_gsl", "microsoft_wil"):
                    zipped.writestr(name + "/ThirdPartyNotices.txt", "Additional third-party terms: " + name)
            dependencies.append({"name": name, "bytes": path.stat().st_size, "sha256": MODULE.digest(path)})
        lock = copy.deepcopy(MODULE.LOCK)
        full_license = b"Full Boost Software License 1.0 fixture"
        extra = lock["additional_notices"][0]
        extra.update(bytes=len(full_license), sha256=hashlib.sha256(full_license).hexdigest())
        (downloads / extra["cache_name"]).write_bytes(full_license)
        return lock, dependencies

    def test_all_fourteen_full_licenses_and_vendor_third_party_notices_are_retained(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            lock, dependencies = self.fixture(root)
            with patch.object(MODULE, "LOCK", lock), patch.object(MODULE, "selected_dependencies", return_value=dependencies):
                rows = MODULE.collect_notices(root, WINDOWS)
                self.assertEqual({row["archive"] for row in rows if "archive" in row}, MODULE.CPU_NAMES)
                boost = next(row for row in rows if row["path"] == "notices/mp11/LICENSE_1_0.txt")
                self.assertEqual(boost["license_id"], "BSL-1.0")
                self.assertEqual(boost["source_commit"], lock["additional_notices"][0]["source_commit"])
                for vendor in ("microsoft_gsl", "microsoft_wil"):
                    self.assertTrue(any(row["archive"] == vendor and row["path"].endswith("ThirdPartyNotices.txt") for row in rows if "archive" in row))
                before = {path.relative_to(root): (path.stat().st_mtime_ns, path.read_bytes()) for path in (root / "notices").rglob("*") if path.is_file()}
                self.assertEqual(MODULE.collect_notices(root, WINDOWS, verify_only=True), rows)
                self.assertEqual(before, {path.relative_to(root): (path.stat().st_mtime_ns, path.read_bytes()) for path in (root / "notices").rglob("*") if path.is_file()})

    def test_interrupted_notice_copy_resumes_exact_bytes_without_overwrite_or_cleanup(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            lock, dependencies = self.fixture(root)
            with patch.object(MODULE, "LOCK", lock), patch.object(MODULE, "selected_dependencies", return_value=dependencies):
                first = root / "notices/onnxruntime/LICENSE"
                first.parent.mkdir(parents=True)
                first.write_bytes((root / "source/LICENSE").read_bytes())
                original_mtime = first.stat().st_mtime_ns
                unknown = root / "notices/keep-user-note.txt"
                unknown.write_text("preserve unrelated partial evidence")
                MODULE.collect_notices(root, WINDOWS)
                self.assertEqual(first.stat().st_mtime_ns, original_mtime)
                self.assertEqual(unknown.read_text(), "preserve unrelated partial evidence")
                self.assertTrue((root / "notices/mp11/LICENSE_1_0.txt").is_file())

    def test_corrupt_partial_is_preserved_and_fails_before_writing_any_missing_notice(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            lock, dependencies = self.fixture(root)
            with patch.object(MODULE, "LOCK", lock), patch.object(MODULE, "selected_dependencies", return_value=dependencies):
                first = root / "notices/onnxruntime/LICENSE"
                first.parent.mkdir(parents=True)
                first.write_bytes(b"corrupt partial")
                with self.assertRaises(ValueError):
                    MODULE.collect_notices(root, WINDOWS)
                self.assertEqual(first.read_bytes(), b"corrupt partial")
                self.assertEqual([path.name for path in first.parent.iterdir()], ["LICENSE"])
                self.assertFalse((root / "notices/mp11").exists())

    def test_missing_full_boost_license_fails_before_notice_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            lock, dependencies = self.fixture(root)
            (root / "downloads" / lock["additional_notices"][0]["cache_name"]).unlink()
            with patch.object(MODULE, "LOCK", lock), patch.object(MODULE, "selected_dependencies", return_value=dependencies):
                with self.assertRaises(FileNotFoundError):
                    MODULE.collect_notices(root, WINDOWS)
                self.assertFalse((root / "notices").exists())


if __name__ == "__main__":
    unittest.main()
