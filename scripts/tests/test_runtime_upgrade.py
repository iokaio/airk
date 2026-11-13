# SPDX-License-Identifier: Apache-2.0
"""Upgrade fixture retention and exact cleanup ordering; no native runtime is launched."""
import importlib.util
import ast
from contextlib import closing
from pathlib import Path, PureWindowsPath
import tempfile
import json
import hashlib
import sqlite3
import time
import subprocess
import signal
import copy
import re
import sys
from types import SimpleNamespace
from unittest import mock
import unittest

SPEC = importlib.util.spec_from_file_location("runtime_upgrade", Path(__file__).parents[1] / "check-runtime-upgrade.py")
UPGRADE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(UPGRADE)

class UpgradeFixtureOwnershipTests(unittest.TestCase):
    def test_replacement_apply_uses_one_durable_authorized_operation(self):
        invoke = mock.Mock(return_value={"replaced": True, "residue": []})
        with mock.patch.object(UPGRADE.secrets, "token_hex", return_value="d" * 64) as token:
            actual = UPGRADE.apply_fixture_replacement(invoke, Path("airk"), Path("install"), "c" * 64)
        token.assert_called_once_with(32)
        invoke.assert_called_once_with(Path("airk"), "runtime", "replace-installation", "--install-dir", Path("install"),
            "--candidate-sha256", "c" * 64, "--reinstall", "--operation-id", "d" * 64, "--apply")
        self.assertEqual(actual, invoke.return_value)

    def test_fresh_replacement_requires_complete_native_empty_registry_and_catalog(self):
        inspected = {"registry_namespace": "a" * 32, "clean_break_error": None, "clean_break": {
            "required": False, "installed_contract": "airk-state/1", "registrations_complete": True,
            "registrations_scope": "current_registry", "registrations": []}}
        def page(method):
            self.assertIn(method["catalog_page"]["request"]["kind"], ("indexes", "projects"))
            return {"status": "catalog_page", "page": {
                "namespace": "a" * 32, "revision": 1, "items": [], "continuation": None}}
        rpc = mock.Mock(side_effect=page)
        result = UPGRADE.validate_fresh_replacement(inspected, rpc, "a" * 32)
        self.assertEqual(result["inspection"], inspected)
        self.assertEqual(set(result["catalog_pages"]), {"indexes", "projects"})
        self.assertEqual(rpc.call_count, 2)

    def test_fresh_replacement_refuses_missing_null_or_incomplete_native_observations(self):
        inspected = {"registry_namespace": "a" * 32, "clean_break_error": None, "clean_break": {
            "required": False, "installed_contract": "airk-state/1", "registrations_complete": True,
            "registrations_scope": "current_registry", "registrations": []}}
        rpc = mock.Mock()
        for fault in ("missing_report", "null_report", "missing_roster", "null_roster", "incomplete", "old_scope", "missing_error", "error", "required", "contract", "registered", "namespace"):
            altered = copy.deepcopy(inspected)
            report = altered["clean_break"]
            if fault == "missing_report": altered.pop("clean_break")
            elif fault == "null_report": altered["clean_break"] = None
            elif fault == "missing_roster": report.pop("registrations")
            elif fault == "null_roster": report["registrations"] = None
            elif fault == "incomplete": report["registrations_complete"] = False
            elif fault == "old_scope": report["registrations_scope"] = "predecessor_reset_roster"
            elif fault == "missing_error": altered.pop("clean_break_error")
            elif fault == "error": altered["clean_break_error"] = "unavailable"
            elif fault == "required": report["required"] = True
            elif fault == "contract": report["installed_contract"] = None
            elif fault == "registered": report["registrations"] = [{"binding": "b" * 64}]
            else: altered["registry_namespace"] = "b" * 32
            with self.subTest(fault=fault), self.assertRaises(RuntimeError):
                UPGRADE.validate_fresh_replacement(altered, rpc, "a" * 32)

    def test_fresh_replacement_refuses_nonempty_crossed_or_incomplete_catalog_pages(self):
        inspected = {"registry_namespace": "a" * 32, "clean_break_error": None, "clean_break": {
            "required": False, "installed_contract": "airk-state/1", "registrations_complete": True,
            "registrations_scope": "current_registry", "registrations": []}}
        def page(method):
            return {"status": "catalog_page", "page": {
                "namespace": "a" * 32, "revision": 1, "items": [], "continuation": None}}
        for fault in ("indexes", "projects", "namespace", "revision", "missing_items", "missing_continuation", "null_page"):
            def wrong_page(method):
                result = page(method)
                kind = method["catalog_page"]["request"]["kind"]
                if fault == kind: result["page"]["items"] = [{"kind": "index" if kind == "indexes" else "project"}]
                elif fault == "namespace": result["page"]["namespace"] = "b" * 32
                elif fault == "revision" and kind == "projects": result["page"]["revision"] = 2
                elif fault == "missing_items": result["page"].pop("items")
                elif fault == "missing_continuation": result["page"].pop("continuation")
                elif fault == "null_page": result["page"] = None
                return result
            with self.subTest(catalog_fault=fault), self.assertRaises(RuntimeError):
                UPGRADE.validate_fresh_replacement(inspected, wrong_page, "a" * 32)

    def test_persisted_baseline_resolved_windows_paths_accept_drive_and_unc_aliases(self):
        class ResolvedPath:
            def __init__(self, value):
                self.value = str(value)

            def resolve(self, *, strict):
                self.assert_strict = strict
                if not strict:
                    raise AssertionError("Ownership paths must resolve strictly")
                if self.value == "missing-location":
                    raise FileNotFoundError(self.value)
                return self

            def __fspath__(self):
                return self.value

            def __eq__(self, other):
                return isinstance(other, ResolvedPath) and self.value == other.value

        def validate(marker, install, control, host):
            with mock.patch.object(UPGRADE.sys, "platform", host), mock.patch.object(UPGRADE, "Path", ResolvedPath):
                return UPGRADE.validate_clean_break_baseline(marker, {"successor_namespace": "a" * 32},
                    ResolvedPath(control), ResolvedPath(install))

        for install, recorded_install, control, recorded_control in (
                (r"D:\owned\install", r"\\?\D:\owned\install", r"D:\owned\control", r"\\?\D:\owned\control"),
                (r"\\server\share\install", r"\\?\UNC\server\share\install", r"\\server\share\control", r"\\?\UNC\server\share\control")):
            marker = {"schema": 1, "state_contract": "airk-state/1", "baseline_version": "1.8.16",
                      "namespace": "a" * 32, "install": recorded_install, "control": recorded_control}
            with self.subTest(install=install):
                validate(marker, install, control, "win32")
                for field, wrong in (("namespace", "b" * 32), ("install", r"D:\other\install"),
                                     ("control", r"D:\other\control"), ("schema", 2),
                                     ("state_contract", "other/1"), ("baseline_version", "1.8.15")):
                    with self.subTest(field=field), self.assertRaises(RuntimeError):
                        validate({**marker, field: wrong}, install, control, "win32")
                with self.assertRaises(RuntimeError):
                    validate(marker, install, control, "linux")
                for field in ("install", "control"):
                    with self.subTest(missing=field), self.assertRaises(FileNotFoundError):
                        validate({**marker, field: "missing-location"}, install, control, "win32")
        posix = {"schema": 1, "state_contract": "airk-state/1", "baseline_version": "1.8.16",
                 "namespace": "a" * 32, "install": "/owned/Install", "control": "/owned/control"}
        with self.assertRaises(RuntimeError):
            validate(posix, "/owned/install", "/owned/control", "linux")

    def test_owned_client_capture_resolved_windows_alias_preserves_exact_image_boundary(self):
        metadata = SimpleNamespace(st_mode=UPGRADE.stat.S_IFREG, st_nlink=1, st_size=12,
                                   st_file_attributes=0, st_uid=UPGRADE.os.getuid() if hasattr(UPGRADE.os, "getuid") else None,
                                   st_dev=1, st_ino=2)
        for plain, resolved in ((r"D:\owned\airk.exe", r"\\?\D:\owned\airk.exe"),
                                (r"\\server\share\airk.exe", r"\\?\UNC\server\share\airk.exe")):
            path = SimpleNamespace(lstat=lambda: metadata, is_symlink=lambda: False,
                                   resolve=lambda **kwargs: PureWindowsPath(resolved),
                                   absolute=lambda: PureWindowsPath(plain))
            with self.subTest(path=plain), mock.patch.object(UPGRADE.sys, "platform", "win32"), mock.patch.object(UPGRADE, "digest", return_value="a" * 64):
                result = UPGRADE.capture_client_image(path)
                self.assertEqual(result["path"], str(PureWindowsPath(resolved)))
                with mock.patch.object(path, "absolute", return_value=PureWindowsPath(r"D:\other\airk.exe")), self.assertRaises(RuntimeError):
                    UPGRADE.capture_client_image(path)
                with mock.patch.object(path, "is_symlink", return_value=True), self.assertRaises(RuntimeError):
                    UPGRADE.capture_client_image(path)

    def test_successor_control_reuse_checks_captured_windows_paths_without_resolving_retired_root(self):
        tree = ast.parse(Path(UPGRADE.__file__).read_text(encoding="utf-8"))
        message = "Clean break reused the original control root or namespace"
        checks = [node for node in ast.walk(tree) if isinstance(node, ast.If)
                  and any(isinstance(child, ast.Raise) and isinstance(child.exc, ast.Call)
                          and any(isinstance(arg, ast.Constant) and arg.value == message for arg in child.exc.args)
                          for child in node.body)]
        self.assertEqual(len(checks), 1)
        validator = compile(ast.fix_missing_locations(ast.Module(body=[checks[0]], type_ignores=[])), "<actual successor control guard>", "exec")
        # Pure paths have no resolve method: the captured old root has already
        # been retired, so checking its native canonical spelling must not stat it.
        for old, current in ((r"D:\owned\control", r"\\?\D:\owned\control"),
                             (r"\\server\share\control", r"\\?\UNC\server\share\control")):
            environment = {**UPGRADE.__dict__, "old_control": PureWindowsPath(old),
                           "control": PureWindowsPath(current), "old_namespace": "a" * 32,
                           "rebuild": {"successor_namespace": "b" * 32}}
            with self.subTest(old=old), mock.patch.object(UPGRADE.sys, "platform", "win32"):
                with self.assertRaises(RuntimeError):
                    exec(validator, environment)
                environment["control"] = PureWindowsPath(r"D:\new\control")
                exec(validator, environment)
                environment["rebuild"]["successor_namespace"] = "a" * 32
                with self.assertRaises(RuntimeError):
                    exec(validator, environment)

    def test_clean_break_boundary_routes_every_prior_version_and_keeps_supported_line(self):
        for previous in ("0.1.0", "1.5.12", "1.8.11", "1.8.14", "1.8.15"):
            for candidate in ("1.8.16", "1.8.17", "1.9.0"):
                self.assertTrue(UPGRADE.clean_break_required(previous, candidate))
        for previous, candidate in (("1.8.16", "1.8.17"), ("1.8.17", "1.9.0"), ("1.8.11", "1.8.15")):
            self.assertFalse(UPGRADE.clean_break_required(previous, candidate))
        for invalid in ("1.8", "1.8.16-beta", "01.8.16", None):
            with self.subTest(version=invalid), self.assertRaises(RuntimeError):
                UPGRADE.clean_break_required(invalid, "1.8.16")

    def clean_break_observation(self):
        originals, rows = [], []
        for position in range(2):
            original = {"binding": str(position+1)*64, "root": f"source{position}",
                "data_root": f"old-data{position}", "collection": f"collection{position}"}
            replacement = {**original, "binding": str(position+3)*64, "data_root": f"new-data{position}"}
            old_job = {"id": str(position+5)*64, "outcome": {"state": "succeeded", "revision": "airk-r1:"+"a"*32+":1:"+"e"*64}}
            job = {"id": str(position+7)*64, "project": replacement["binding"], "kind": "index", "state": "succeeded",
                "attempts": 1, "outcome": {"state": "succeeded", "revision": "airk-r1:"+"b"*32+":1:"+"f"*64}}
            originals.append({"registration": original, "paused": position == 1, "watch": position == 1,
                "sources": {}, "indexed_job": old_job})
            rows.append({"original": original, "replacement": replacement, "sources": {}, "original_paused": position == 1,
                "paused": position == 1, "watching": position == 1, "job": job, "readiness": "ready", "failure": None})
        rebuild = {"schema": 1, "mode": "clean_break", "candidate": "c"*64, "predecessor": "d"*64,
            "provenance": "native_rebuild_from_saved_configuration",
            "original_namespace": "a"*32, "successor_namespace": "b"*32, "index_scope": None,
            "failure": None, "stage": "ready", "indexes": rows, "indexes_complete": True, "expected_indexes": 2,
            "cleanup": {"complete": True, "state": "complete", "failure": None}}
        return originals, {"rebuild": rebuild, "rebuild_error": None}

    def test_clean_break_requires_all_fresh_indexes_and_preserves_each_supported_preference(self):
        originals, diagnosis = self.clean_break_observation()
        before = copy.deepcopy(originals)
        jobs = {item["indexed_job"]["id"] for item in originals}
        validate = lambda value: UPGRADE.validate_clean_break_rebuild(value, originals, "a"*32, jobs, "c"*64, "d"*64)
        observed = validate(diagnosis)
        self.assertEqual(len(observed["indexes"]), 2)
        self.assertFalse(observed["indexes"][0]["paused"])
        self.assertTrue(observed["indexes"][1]["paused"])
        self.assertEqual(originals, before)
        for fault in ("missing", "duplicate", "extra", "old_binding", "same_storage", "changed_root", "old_job", "old_namespace",
                      "changed_settings", "changed_pause", "subset", "failed", "no_publication", "old_publication", "no_cleanup", "blocked_cleanup", "wrong_mode"):
            altered = copy.deepcopy(diagnosis)
            rebuild = altered["rebuild"]
            row = rebuild["indexes"][1]
            if fault == "missing": rebuild["indexes"].pop()
            elif fault == "duplicate": rebuild["indexes"][1] = rebuild["indexes"][0]
            elif fault == "extra": rebuild["indexes"].append(copy.deepcopy(row))
            elif fault == "old_binding": row["replacement"]["binding"] = row["original"]["binding"]
            elif fault == "same_storage": row["replacement"]["data_root"] = row["original"]["data_root"]
            elif fault == "changed_root": row["replacement"]["root"] = "other-source"
            elif fault == "old_job": row["job"]["id"] = originals[1]["indexed_job"]["id"]
            elif fault == "old_namespace": rebuild["successor_namespace"] = rebuild["original_namespace"]
            elif fault == "changed_settings": row["watching"] = False
            elif fault == "changed_pause": row["paused"] = False
            elif fault == "subset": rebuild["index_scope"] = originals[0]["registration"]["binding"]
            elif fault == "failed": rebuild["stage"] = "failed"
            elif fault == "no_publication": row["job"]["outcome"]["revision"] = None
            elif fault == "old_publication": row["job"]["outcome"]["revision"] = originals[1]["indexed_job"]["outcome"]["revision"]
            elif fault == "no_cleanup": rebuild["cleanup"] = None
            elif fault == "blocked_cleanup": rebuild["cleanup"] = {"complete": False, "state": "blocked", "failure": {"code": "cleanup_blocked"}}
            else: rebuild["mode"] = "rebuild"
            with self.subTest(fault=fault), self.assertRaises(RuntimeError):
                validate(altered)
        pending = copy.deepcopy(diagnosis)
        pending["rebuild"].update(stage="preparing", successor_namespace=None,
            cleanup={"complete": False, "state": "waiting_for_indexes", "failure": None})
        for row in pending["rebuild"]["indexes"]:
            row.update(replacement=None, job=None, readiness="pending")
        self.assertIsNone(validate(pending))

    def test_clean_break_rejects_reused_storage_hidden_by_windows_path_aliases(self):
        for original_root, replacement_root in ((r"D:\owned\data", r"\\?\D:\owned\data"),
                                                 (r"\\server\share\data", r"\\?\UNC\server\share\data")):
            originals, diagnosis = self.clean_break_observation()
            originals[0]["registration"]["data_root"] = original_root
            diagnosis["rebuild"]["indexes"][0]["replacement"]["data_root"] = replacement_root
            with self.subTest(root=original_root), mock.patch.object(UPGRADE.sys, "platform", "win32"):
                with self.assertRaises(RuntimeError):
                    UPGRADE.validate_clean_break_rebuild(diagnosis, originals, "a" * 32,
                        {item["indexed_job"]["id"] for item in originals}, "c" * 64, "d" * 64)

    def test_clean_break_waits_for_complete_native_roster_without_accepting_a_partial_result(self):
        originals, diagnosis = self.clean_break_observation()
        pending = copy.deepcopy(diagnosis)
        pending["rebuild"].update(stage="accepted", successor_namespace=None, indexes=[], indexes_complete=False,
            cleanup={"complete": False, "state": "waiting_for_indexes", "failure": None})
        validate = lambda value: UPGRADE.validate_clean_break_rebuild(value, originals, "a"*32, set(), "c"*64, "d"*64)
        self.assertIsNone(validate(pending), "accepted reset is not a complete empty-index success")
        for fault in ("missing_qualifier", "wrong_count", "partial_roster", "ready_partial"):
            changed = copy.deepcopy(pending)
            if fault == "missing_qualifier": changed["rebuild"].pop("indexes_complete")
            elif fault == "wrong_count": changed["rebuild"]["expected_indexes"] = 1
            elif fault == "partial_roster": changed["rebuild"]["indexes"] = diagnosis["rebuild"]["indexes"][:1]
            else: changed["rebuild"]["stage"] = "ready"
            with self.subTest(fault=fault), self.assertRaises(RuntimeError):
                validate(changed)

    def test_clean_break_ready_stage_waits_for_jobs_policy_restore_and_cleanup_without_hiding_false_authority(self):
        originals, ready = self.clean_break_observation()
        old_jobs = {item["indexed_job"]["id"] for item in originals}
        validate = lambda value: UPGRADE.validate_clean_break_rebuild(value, originals, "a"*32, old_jobs, "c"*64, "d"*64)
        for state in ("pending", "running", "succeeded"):
            waiting = copy.deepcopy(ready)
            waiting["rebuild"]["cleanup"] = {"complete": False, "state": "waiting_for_indexes", "failure": None, "observation": None}
            waiting["rebuild"]["indexes"][1]["paused"] = False
            for row in waiting["rebuild"]["indexes"]:
                row["job"]["state"] = state
                if state != "succeeded":
                    row["job"]["outcome"] = None
                    row["readiness"] = "pending"
            with self.subTest(state=state):
                self.assertIsNone(validate(waiting), "native Ready is not cleanup or restored-policy completion")
            for fault in ("wrong_root", "old_job", "old_namespace", "wrong_provenance", "failed_job", "blocked_cleanup"):
                changed = copy.deepcopy(waiting)
                row = changed["rebuild"]["indexes"][0]
                if fault == "wrong_root": row["replacement"]["root"] = "unreviewed-source"
                elif fault == "old_job": row["job"]["id"] = next(iter(old_jobs))
                elif fault == "old_namespace": changed["rebuild"]["successor_namespace"] = "a"*32
                elif fault == "wrong_provenance": changed["rebuild"]["provenance"] = "untrusted_source_substitution"
                elif fault == "failed_job": row["job"].update(state="failed", outcome={"state": "failed", "code": "source_unavailable"})
                else: changed["rebuild"]["cleanup"] = {"complete": False, "state": "blocked", "failure": {"code": "ownership_blocked"}}
                with self.subTest(state=state, fault=fault), self.assertRaises(RuntimeError):
                    validate(changed)
        for fault in ("pending_job", "missing_publication", "unrestored_pause"):
            completed = copy.deepcopy(ready)
            row = completed["rebuild"]["indexes"][1]
            if fault == "pending_job": row["job"].update(state="pending", outcome=None)
            elif fault == "missing_publication": row["job"]["outcome"]["revision"] = None
            else: row["paused"] = False
            with self.subTest(completed_fault=fault), self.assertRaises(RuntimeError):
                validate(completed)

    def test_clean_break_catalog_pages_keep_complete_native_values_and_pin_revision(self):
        first = {"status": "catalog_page", "future": {"complete": True}, "page": {"namespace": "a"*32, "revision": 3,
            "items": [{"kind": "index", "value": {"id": "idx_"+"1"*32}}],
            "continuation": {"kind": "indexes", "revision": 3, "after": "idx_"+"1"*32,
                "index_id": None, "project_id": None}}}
        second = {"status": "catalog_page", "page": {"namespace": "a"*32, "revision": 3,
            "items": [{"kind": "index", "value": {"id": "idx_"+"2"*32}}], "continuation": None}}
        rpc = mock.Mock(side_effect=[first, second])
        rows, pages = UPGRADE.catalog_page_items(rpc, "indexes")
        self.assertEqual(pages, [first, second])
        self.assertEqual(len(rows), 2)
        request = rpc.call_args_list[1].args[0]["catalog_page"]["request"]
        self.assertEqual(request["after"], first["page"]["continuation"])
        self.assertEqual(request["expected_catalog_revision"], 3)
        for field, value in (("namespace", "b"*32), ("revision", 4)):
            changed = copy.deepcopy(second); changed["page"][field] = value
            with self.subTest(field=field), self.assertRaises(RuntimeError):
                UPGRADE.catalog_page_items(mock.Mock(side_effect=[first, changed]), "indexes")

    def test_clean_break_consumes_retained_cleanup_proof_without_replaying_any_native_operation(self):
        originals, diagnosis = self.clean_break_observation()
        rebuild = diagnosis["rebuild"]
        process = lambda scope: {"scope": scope, "remaining": 0, "observations": [], "image_roster": [], "image_exclusions": []}
        roots = [item["registration"]["data_root"] for item in originals]
        observation = {"complete": True, "phase": "complete", "scope": "owned_predecessor_native_state_indexes_and_executables",
            "old_control": "old/control", "collections_total": 2, "data_roots": roots,
            "retired_collections": [str(Path(item["registration"]["data_root"]) / "collections-1.4" / item["registration"]["collection"])
                for item in originals],
            "retired_data_roots": [item["registration"]["data_root"] for item in originals],
            "launchers": {"schema": 1, "active": "c"*64, "legacy_launchers_retired": True, "launcher": None,
                "universal_launcher": None, "processes": process("verified_launcher_images")},
            "runtimes": {"active": "c"*64, "complete": True, "selected_service_restarted": False,
                "retired_runtimes": ["d"*64], "processes": process("verified_superseded_runtime_images")},
            "future_complete_receipt": {"retained": True}}
        rebuild["cleanup"]["observation"] = observation
        with mock.patch.object(UPGRADE.subprocess, "run") as native:
            retained = UPGRADE.validate_clean_break_cleanup(rebuild, originals, "old/control", "c"*64, "d"*64)
        native.assert_not_called()
        self.assertIs(retained, observation)
        self.assertEqual(retained["future_complete_receipt"], {"retained": True})
        for fault in ("missing", "partial", "wrong_control", "missing_root", "duplicate_root", "wrong_count", "remaining_process",
                      "restarted", "empty_retirement", "old_launcher", "new_runtime_retired"):
            changed = copy.deepcopy(rebuild)
            value = changed["cleanup"]["observation"]
            if fault == "missing": changed["cleanup"]["observation"] = None
            elif fault == "partial": value["complete"] = False
            elif fault == "wrong_control": value["old_control"] = "other/control"
            elif fault == "missing_root": value["retired_data_roots"].pop()
            elif fault == "duplicate_root": value["retired_data_roots"].append(value["retired_data_roots"][0])
            elif fault == "wrong_count": value["collections_total"] = 1
            elif fault == "remaining_process": value["launchers"]["processes"]["remaining"] = 1
            elif fault == "restarted": value["runtimes"]["selected_service_restarted"] = True
            elif fault == "empty_retirement": value["runtimes"]["retired_runtimes"] = []
            elif fault == "old_launcher": value["launchers"]["launcher"] = "old.exe"
            else: value["runtimes"]["retired_runtimes"].append("c"*64)
            with self.subTest(fault=fault), self.assertRaises(RuntimeError):
                UPGRADE.validate_clean_break_cleanup(changed, originals, "old/control", "c"*64, "d"*64)

    def test_clean_break_connection_refuses_missing_or_broadened_sources_before_mutation(self):
        replacement = {"binding": "b"*64, "root": "fictional-source", "data_root": "new-data", "collection": "fixture"}
        index = {"kind": "index", "value": {"id": "idx_"+"1"*32, "backing": replacement, "tombstone": False}}
        source = {"kind": "source", "value": {"id": "src_"+"1"*32, "enabled": True, "owner": True, "canonical_path": replacement["root"]}}
        for indexes, sources in (([], []), ([index, index], []), ([index], []), ([index], [source, source]),
                                 ([index], [{"kind": "source", "value": {**source["value"], "canonical_path": "other-source"}}])):
            rpc = mock.Mock()
            with mock.patch.object(UPGRADE, "catalog_page_items", side_effect=[(indexes, []), (sources, [])]), \
                    self.subTest(indexes=indexes, sources=sources), self.assertRaises(RuntimeError):
                UPGRADE.fresh_clean_break_selection(rpc, replacement)
            rpc.assert_not_called()

    def test_clean_break_auto_route_uses_actual_versions_before_any_runtime_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            old = self.input_runtime(root / "old", "1.8.11", 1)
            new = self.input_runtime(root / "new", "1.8.16", 2)
            args = ["check-runtime-upgrade.py", "--runtime", str(new), "--previous-runtime", str(old),
                "--output", str(root / "result.json"), "--fixture-setup", "reviewed"]
            with mock.patch.object(sys, "argv", args), mock.patch.object(UPGRADE, "run_clean_break", return_value="clean-break") as reset, \
                    mock.patch.object(UPGRADE.subprocess, "run") as native:
                self.assertEqual(UPGRADE.main(), "clean-break")
            reset.assert_called_once(); native.assert_not_called()

    def test_source_input_ledger_covers_setup_failure_before_registration_and_never_claims_empty_success(self):
        self.assertEqual(UPGRADE.observe_source_inputs([]), {"count": 0, "preserved": None, "observations": []})
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "README.md"
            source.write_bytes(b"original fictional source before fallible setup")
            source_inputs = [{"source_file": str(source), "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest()}]
            registrations = []  # A native setup failure has not yet added a completed Index definition.
            observed = UPGRADE.observe_source_inputs(source_inputs)
            self.assertEqual(registrations, [])
            self.assertEqual(observed["count"], 1)
            self.assertTrue(observed["preserved"])
            self.assertEqual(observed["observations"][0]["source_file"], str(source))
            source.write_bytes(b"source was changed despite setup failure")
            self.assertFalse(UPGRADE.observe_source_inputs(source_inputs)["preserved"])
            source.unlink()
            missing = UPGRADE.observe_source_inputs(source_inputs)
            self.assertEqual(missing["count"], 1)
            self.assertFalse(missing["preserved"])

    def test_legacy_windows_path_budget_covers_staging_parts_final_payload_and_utf16(self):
        inventories = [{"root": "C:/source", "files": {"airk.exe": "a"*64, "licenses/nested/license.txt": "b"*64}}]
        observed = UPGRADE.legacy_windows_path_budget("D:/aq/au-12345678", inventories, "win32")
        self.assertTrue(observed["checked"])
        self.assertLessEqual(observed["max_file_utf16_units"], 259)
        self.assertGreater(observed["max_file_utf16_units"], 120, "reserved partial hash path is budgeted")
        for root, names in (("D:/"+"deep/"*27, inventories),
                            ("D:/aq/u", [{"root": "C:/source", "files": {"deep/"*34+"file.txt": "a"*64}}]),
                            ("D:/aq/u", [{"root": "C:/"+"a"*250, "files": {"file.txt": "a"*64}}]),
                            ("D:/"+"😀"*70, inventories)):
            with self.subTest(root=root), self.assertRaisesRegex(RuntimeError, "legacy_windows_fixture_path_budget"):
                UPGRADE.legacy_windows_path_budget(root, names, "win32")
        self.assertFalse(UPGRADE.legacy_windows_path_budget("/long/"*70, inventories, "linux")["checked"])

    def test_fixture_base_override_does_not_modify_existing_directory_and_short_owner_is_retained(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            selected = root / "short"
            selected.mkdir()
            before = selected.stat()
            with mock.patch.dict(UPGRADE.os.environ, {"AIRK_PRIVATE_CENSUS_BASE": str(root / "missing-long-base")}):
                self.assertEqual(UPGRADE.fixture_parent(selected), selected.resolve())
            self.assertEqual(selected.stat().st_mode, before.st_mode)
            owner = UPGRADE.owned_temporary(selected)
            self.assertTrue(Path(owner.name).name.startswith("au-"))
            self.assertEqual(len(Path(owner.name).name), 11)
            self.assertTrue(Path(owner.name).is_dir())
            owner.cleanup()

    def input_runtime(self, path, version, schema):
        path.mkdir()
        main = path / ("AIRK.app/Contents/MacOS/airk" if sys.platform == "darwin" else "airk.exe" if sys.platform == "win32" else "airk")
        main.parent.mkdir(parents=True, exist_ok=True)
        main.write_bytes(b"inert fictional runtime; never execute")
        native = {"schema": schema, "version": version, "os": "windows" if sys.platform == "win32" else "macos" if sys.platform == "darwin" else "linux",
                  "arch": "aarch64" if UPGRADE.platform.machine().lower() in ("arm64", "aarch64") else "x86_64", "airk_sha256": UPGRADE.digest(main)}
        if schema == 1:
            helper = main.with_name("airk-embed.exe" if sys.platform == "win32" else "airk-embed")
            helper.write_bytes(b"inert fictional helper; never execute")
            native["embed_sha256"] = UPGRADE.digest(helper)
        (path / "airk-runtime.json").write_text(json.dumps(native), encoding="utf-8")
        files = {item.relative_to(path).as_posix(): UPGRADE.digest(item) for item in path.rglob("*") if item.is_file()}
        install = {key: native[key] for key in ("version", "os", "arch")}
        install.update(schema=1, files=files)
        (path / "airk-install.json").write_text(json.dumps(install), encoding="utf-8")
        return path

    def test_input_preflight_checks_all_seals_and_runs_no_subprocess(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            old = self.input_runtime(root / "old", "1.8.11", 1)
            new = self.input_runtime(root / "new", "1.8.16", 2)
            receipt = root / "inputs.json"
            arguments = ["check-runtime-upgrade.py", "--runtime", str(new), "--previous-runtime", str(old),
                         "--clean-break", "--check-inputs", "--output", str(receipt)]
            with mock.patch.object(sys, "argv", arguments), mock.patch.object(UPGRADE.subprocess, "run") as run, \
                    mock.patch.object(UPGRADE.subprocess, "Popen") as popen, mock.patch.object(UPGRADE.subprocess, "check_output") as output:
                UPGRADE.main()
            run.assert_not_called(); popen.assert_not_called(); output.assert_not_called()
            observed = json.loads(receipt.read_text(encoding="utf-8"))
            self.assertTrue(observed["passed"])
            self.assertFalse(observed["runtime_executed"])
            self.assertEqual(observed["inputs"]["predecessor"]["install_manifest_sha256"], UPGRADE.digest(old / "airk-install.json"))
            self.assertEqual(observed["inputs"]["candidate"]["version"], "1.8.16")
            with mock.patch.object(sys, "argv", arguments), self.assertRaises(FileExistsError):
                UPGRADE.main()
            self.assertEqual(json.loads(receipt.read_text(encoding="utf-8")), observed)
            unsealed = new / "unexpected.txt"
            unsealed.write_bytes(b"unexpected bytes")
            with self.assertRaisesRegex(RuntimeError, "sealed file inventory"):
                UPGRADE.check_upgrade_inputs(new, old, clean_break=True)
            unsealed.unlink()
            manifest = new / "airk-install.json"
            original_manifest = manifest.read_bytes()
            altered = json.loads(original_manifest)
            altered["arch"] = "foreign_arch"
            manifest.write_text(json.dumps(altered), encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "sealed file inventory"):
                UPGRADE.check_upgrade_inputs(new, old, clean_break=True)
            manifest.write_bytes(original_manifest)
            (new / "airk-runtime.json").write_text("{}", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "sealed file inventory"):
                UPGRADE.check_upgrade_inputs(new, old, clean_break=True)
            (new / "airk-runtime.json").unlink()
            with self.assertRaises(FileNotFoundError):
                UPGRADE.check_upgrade_inputs(new, old, clean_break=True)

    def test_input_preflight_retains_inaccessible_root_failure_before_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            receipt = Path(temporary) / "failed.json"
            arguments = ["check-runtime-upgrade.py", "--runtime", "inaccessible-fixture", "--previous-runtime", "old-fixture",
                         "--check-inputs", "--output", str(receipt)]
            denied = PermissionError("fixture access denied")
            with mock.patch.object(sys, "argv", arguments), mock.patch.object(UPGRADE.Path, "lstat", side_effect=denied), \
                    mock.patch.object(UPGRADE.subprocess, "Popen") as popen:
                with self.assertRaises(PermissionError) as observed:
                    UPGRADE.main()
            self.assertIs(observed.exception, denied)
            popen.assert_not_called()
            failure = json.loads(receipt.read_text(encoding="utf-8"))
            self.assertFalse(failure["passed"])
            self.assertFalse(failure["runtime_executed"])
            self.assertEqual(failure["failure"]["type"], "PermissionError")

    def test_input_preflight_keeps_darwin_predecessor_helper_inside_bundle(self):
        with tempfile.TemporaryDirectory() as temporary, mock.patch.object(UPGRADE.sys, "platform", "darwin"), \
                mock.patch.object(UPGRADE.platform, "machine", return_value="arm64"):
            root = Path(temporary)
            old = self.input_runtime(root / "old", "1.8.11", 1)
            new = self.input_runtime(root / "new", "1.8.16", 2)
            self.assertTrue((old / "AIRK.app/Contents/MacOS/airk-embed").is_file())
            self.assertFalse((old / "airk-embed").exists())
            observed = UPGRADE.check_upgrade_inputs(new, old, clean_break=True)
            self.assertTrue(observed["passed"])

    def test_pre_candidate_cleanup_requires_exact_old_owner_and_retains_stop_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); binary = root/"old.exe"; binary.write_bytes(b"inert exact old image")
            install = root/"install"; install.mkdir()
            execution = UPGRADE.digest(binary)
            health = {"status":"health","observation":{"runtime_sha256":"a"*64,"executable_sha256":execution,"instance":"b"*32,"process_id":123}}
            # Match the actual Clap RuntimeCommand catalog and deny_unknown_fields
            # lifecycle Input, rather than blessing an invented mocked subcommand.
            native = Path(__file__).parents[2] / "crates/airk-installer/src"
            runtime = (native / "runtime_install.rs").read_text(encoding="utf-8").split("pub enum RuntimeCommand {", 1)[1].split("\n}", 1)[0]
            commands = {re.sub(r"(?<!^)(?=[A-Z])", "-", value).lower()
                for value in re.findall(r"^    ([A-Z][A-Za-z]+)\s*(?:\{|,)", runtime, re.MULTILINE)}
            self.assertIn("lifecycle", commands)
            self.assertNotIn("stop", commands)
            lifecycle = (native / "runtime_install" / "lifecycle.rs").read_text(encoding="utf-8").split("pub struct Input {", 1)[1].split("\n}", 1)[0]
            fields = set(re.findall(r"^    ([a-z_]+):", lifecycle, re.MULTILINE))
            self.assertEqual(fields, {"caller", "key", "action", "expected_instance", "runtime", "force"})
            calls = []
            response = {}
            def run(executable, *arguments, input=None, timeout=None):
                calls.append((executable, arguments, input))
                if arguments[:2] == ("service", "rpc"):
                    method = json.loads(input)
                    if method == "health": value = health
                    elif method == "request_epoch": value = {"status": "request_epoch", "namespace": "c"*32, "epoch": 7}
                    else: self.fail("Unexpected fixture observation")
                else:
                    self.assertIn(arguments[1], commands, "teardown must use a real native parser command")
                    self.assertEqual(arguments, ("runtime", "lifecycle", "--install-dir", install))
                    request = json.loads(input)
                    self.assertEqual(set(request), fields)
                    self.assertEqual(request["action"], "stop")
                    self.assertFalse(request["force"])
                    self.assertEqual(request["expected_instance"], health["observation"]["instance"])
                    self.assertEqual(request["runtime"], health["observation"]["runtime_sha256"])
                    self.assertEqual(request["key"]["namespace"], "c"*32)
                    self.assertEqual(request["key"]["epoch"], 7)
                    self.assertRegex(request["caller"], r"^[0-9a-f]{32}$")
                    self.assertRegex(request["key"]["nonce"], r"^[0-9a-f]{32}$")
                    retained = list(root.glob("*stop-request*.json"))
                    self.assertEqual(len(retained), 1, "complete exact stop intent must be saved before mutation")
                    self.assertEqual(json.loads(retained[0].read_text(encoding="utf-8")), request)
                    value = {"status": "lifecycle", "record": {"id": "d"*64, "phase": "complete",
                        "request": {**{key: value for key, value in request.items() if key != "force"}, "manager": False}},
                        "future_complete_native_receipt": {"retained": True}}
                    response.update(value)
                return SimpleNamespace(returncode=0, stdout=json.dumps(value), stderr="")
            command = mock.Mock(side_effect=run)
            result = UPGRADE.stop_predecessor_before_staging(command,binary,install,root/"control","a"*64,execution)
            self.assertEqual(result["native_stop"], response)
            self.assertFalse(result["process_exit_confirmed"])
            self.assertEqual(calls[1][1][:2], ("service", "rpc"))
            self.assertEqual(json.loads(calls[1][2]), "request_epoch")
            self.assertEqual(calls[2][1], ("runtime", "lifecycle", "--install-dir", install))
            for field,value in (("runtime_sha256","c"*64),("executable_sha256","c"*64),("instance","invalid"),("process_id",False)):
                crossed = copy.deepcopy(health); crossed["observation"][field] = value
                command = mock.Mock(return_value=SimpleNamespace(returncode=0,stdout=json.dumps(crossed)))
                with self.subTest(field=field), self.assertRaises(RuntimeError):
                    UPGRADE.stop_predecessor_before_staging(command,binary,install,root/"control","a"*64,execution)
                self.assertEqual(command.call_count,1)
            (install/"activation.json").write_text("pending exact owner")
            command = mock.Mock()
            with self.assertRaises(RuntimeError):
                UPGRADE.stop_predecessor_before_staging(command,binary,install,root/"control","a"*64,execution)
            command.assert_not_called()

    def test_owned_stop_keeps_exact_intent_and_full_unconfirmed_receipt_without_force_or_replay(self):
        for fault in ("phase", "request", "exit", "malformed_record"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary); install = root / "install"; install.mkdir()
                binary = root / "airk.exe"; binary.write_bytes(b"inert exact owned fixture executable")
                health = {"status": "health", "observation": {"instance": "b"*32, "runtime_sha256": "a"*64,
                    "executable_sha256": UPGRADE.digest(binary), "process_id": 123}}
                submitted, returned = [], []
                def command(executable, *arguments, input=None, timeout=None):
                    if arguments[:2] == ("service", "rpc"):
                        self.assertEqual(json.loads(input), "request_epoch")
                        return SimpleNamespace(returncode=0, stdout=json.dumps({"status": "request_epoch", "namespace": "c"*32, "epoch": 9}), stderr="")
                    self.assertEqual(arguments, ("runtime", "lifecycle", "--install-dir", install))
                    request = json.loads(input); submitted.append(request)
                    self.assertFalse(request["force"])
                    record = {"id": "d"*64, "phase": "draining" if fault == "phase" else "complete",
                        "request": {**{key: value for key, value in request.items() if key != "force"}, "manager": False}}
                    if fault == "request": record["request"]["expected_instance"] = "f"*32
                    value = {"status": "lifecycle", "record": None if fault == "malformed_record" else record,
                        "complete_native_failure_details": {"retry": {"exact_request_only": True}}}
                    returned.append(value)
                    return SimpleNamespace(returncode=1 if fault == "exit" else 0, stdout=json.dumps(value), stderr="retained native failure")
                with self.assertRaisesRegex(RuntimeError, "Stop outcome is unconfirmed or crossed"):
                    UPGRADE.stop_owned_runtime(command, binary, install, root / "control", "a"*64, UPGRADE.digest(binary), observed_health=health)
                self.assertEqual(len(submitted), 1, "uncertain Stop is never replayed or forced")
                request = json.loads((root / "fixture-stop-request.json").read_text(encoding="utf-8"))
                observation = json.loads((root / "fixture-stop-observation.json").read_text(encoding="utf-8"))
                self.assertEqual(request, submitted[0])
                self.assertEqual(observation["native_stop"], returned[0])
                self.assertFalse(observation["lifecycle_complete"])
                self.assertFalse(observation["process_exit_confirmed"])

    def test_publication_summary_and_expansion_require_the_same_validated_identity(self):
        token = "airk-r1:"+"1"*32+":4:"+"2"*64
        summary = {"token":token}
        expanded = {"token":token,"incarnation":"1"*32,"generation":4,"revision":"2"*64,"collection":"upgrade"}
        self.assertEqual(UPGRADE.publication_identity(summary,4,"upgrade"),UPGRADE.publication_identity(expanded,4,"upgrade"))
        for field,value in (("incarnation","3"*32),("revision","3"*64),("generation",5),("generation",True),("collection","foreign"),("token","arbitrary")):
            with self.subTest(field=field,value=value), self.assertRaises(RuntimeError):
                UPGRADE.publication_identity({**expanded,field:value},4,"upgrade")
        with self.assertRaises(RuntimeError):
            UPGRADE.publication_identity(summary,5,"upgrade")

    def test_scoped_semantic_identity_requires_both_actual_publication_bound_queries(self):
        status = {"publication":{"token":"airk-r1:"+"1"*32+":4:"+"2"*64},"generation":4,"indexed_vectors":1,
                  "semantic_readiness":{"state":"ready","selected_artifacts":1,"completed_artifacts":1}}
        original_status = copy.deepcopy(status)
        model = {"reference":{"pipeline_digest":"a"*64,"legacy_identity":"b"*64},"profile":"retained-model"}
        queries = {mode:{"publication":status["publication"],"generation":4,
                    "semantic_readiness":{**status["semantic_readiness"],"pipeline_digest":"a"*64,"selection_digest":"c"*64},
                    "execution":{"requested_mode":mode,"served_mode":mode,"incomplete":False}}
                   for mode in ("vector","hybrid")}
        queries["vector"]["publication"] = {**status["publication"],"generation":4,"incarnation":"1"*32,"revision":"2"*64,"collection":"upgrade"}
        identity = UPGRADE.semantic_identity(status,model,queries)
        self.assertEqual(identity["pipeline_digest"],"a"*64)
        self.assertEqual(identity["selection_digest"],"c"*64)
        self.assertEqual(identity["identity_provenance"],"actual_vector_and_hybrid_publication_reads")
        self.assertEqual(status,original_status,"scoped status fields must remain unchanged")
        with self.assertRaises(RuntimeError):
            UPGRADE.semantic_identity(status,model,{"vector":queries["vector"]})
        changes = [("publication",{"token":"other"}),("generation",5),
                   ("semantic_readiness",{**queries["hybrid"]["semantic_readiness"],"pipeline_digest":"d"*64}),
                   ("semantic_readiness",{**queries["hybrid"]["semantic_readiness"],"selection_digest":"d"*64}),
                   ("semantic_readiness",{**queries["hybrid"]["semantic_readiness"],"completed_artifacts":0}),
                   ("execution",{"requested_mode":"hybrid","served_mode":"lexical","incomplete":False})]
        for field,value in changes:
            altered = copy.deepcopy(queries)
            altered["hybrid"][field] = value
            with self.subTest(field=field,value=value), self.assertRaises(RuntimeError):
                UPGRADE.semantic_identity(status,model,altered)
        with self.assertRaises(RuntimeError):
            UPGRADE.semantic_identity({**status,"semantic_readiness":{**status["semantic_readiness"],"pipeline_digest":"d"*64}},model,queries)

    def test_reviewed_https_reconnect_uses_public_native_enrollment_once(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            control = root / "control"
            private = control / "http-1"
            private.mkdir(parents=True, mode=0o700)
            spec = importlib.util.spec_from_file_location("upgrade_enrollment_contract", Path(__file__).parents[1] / "http_fixture.py")
            helper = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(helper)
            selected = {"namespace":"a" * 32,"expected_catalog_revision":4,"index_id":"idx_original"}
            submissions = []
            def invoke(_binary, *args, input=None, timeout=180):
                if args[:2] == ("runtime", "http-connection"):
                    self.assertGreater(timeout, 0); self.assertLessEqual(timeout, 60)
                    source = Path(args[args.index("--selection-file") + 1])
                    self.assertEqual(json.loads(source.read_text()), {"kind":"scoped","selection":selected})
                    self.assertEqual(args[args.index("--client") + 1], "unknown")
                    identifier = args[args.index("--connection-id") + 1]
                    submissions.append(identifier)
                    token, ca = private / ("credential-" + identifier + ".token"), private / "ca.pem"
                    token.write_text("owned fixture reference"); ca.write_text("owned fixture CA reference")
                    return {"schema":1,"connection_id":identifier,"selection":{"kind":"scoped","selection":selected},
                            "credential_file":str(token),"ca_certificate_file":str(ca)}
                self.assertEqual(args[:2], ("service", "rpc"))
                self.assertEqual(json.loads(input), "health", "Console must not perform private HTTP enrollment")
                return {"observation":{"instance":"same owned service"}}
            transport = SimpleNamespace(enroll_native=helper.enroll_native,
                validate_native_identity=mock.Mock(side_effect=RuntimeError("stop after native enrollment boundary")))
            loader = SimpleNamespace(loader=SimpleNamespace(exec_module=lambda module:None))
            with mock.patch.object(UPGRADE.importlib.util,"spec_from_file_location",return_value=loader), \
                    mock.patch.object(UPGRADE.importlib.util,"module_from_spec",return_value=transport):
                with self.assertRaisesRegex(RuntimeError,"native enrollment boundary"):
                    UPGRADE.verify_https_reconnect(invoke, root / "airk", root / "install", control, root,
                        {"binding":"original"}, selected, "original-config-hash", b"saved source")
            self.assertEqual(len(submissions), 1)
            self.assertEqual(list(private.glob("fixture-selection-*.json")), [])

    def test_darwin_owned_exit_waits_for_exact_native_image_retirement_not_signal_causality(self):
        with tempfile.TemporaryDirectory() as temporary:
            launcher = Path(temporary).resolve() / "airk-launcher"
            launcher.write_bytes(b"fictional predecessor executable bytes")
            image = UPGRADE.capture_client_image(launcher)
            self.assertEqual(image["sha256"], hashlib.sha256(launcher.read_bytes()).hexdigest())
            row = {"path":image["path"], "sha256":image["sha256"]}
            cleanup = {"launchers":{"schema":1, "legacy_launchers_retired":True,
                "launcher":None, "universal_launcher":None, "processes":{"scope":"verified_launcher_images", "remaining":0,
                "image_roster":[row], "observations":[{**row,"observation":{"complete":True,"failure":None,
                    "matching_executable_pids":[],"unobserved":0}}]}}}
            for status in (0, -UPGRADE.DARWIN_SIGKILL):
                pending = UPGRADE.darwin_client_exit(123, status, image)
                self.assertFalse(pending["qualified"])
                self.assertFalse(pending["signal_causality_proven"])
                # Native cleanup deleted the exact original executable; retain
                # its captured full digest rather than treating absence as proof.
                if launcher.exists(): launcher.unlink()
                observed = UPGRADE.darwin_client_exit(123, status, image, cleanup)
                self.assertTrue(observed["qualified"])
                self.assertFalse(observed["held_kernel_census"])
                self.assertFalse(observed["signal_causality_proven"])
            for invalid in (None, True, 1, -signal.SIGTERM):
                with self.subTest(status=invalid), self.assertRaisesRegex(RuntimeError,"accepted exact owned exit"):
                    UPGRADE.darwin_client_exit(123, invalid, image, cleanup)
            for fault in ("path", "hash", "duplicate", "absent", "incomplete", "remaining", "unobserved", "failure"):
                bad = copy.deepcopy(cleanup); processes = bad["launchers"]["processes"]
                if fault == "path": processes["image_roster"][0]["path"] = str(Path(image["path"]).with_name("foreign"))
                elif fault == "hash": processes["observations"][0]["sha256"] = "0"*64
                elif fault == "duplicate": processes["image_roster"] *= 2
                elif fault == "absent": processes["image_roster"] = []
                elif fault == "incomplete": processes["observations"][0]["observation"]["complete"] = False
                elif fault == "remaining": processes["observations"][0]["observation"]["matching_executable_pids"] = [123]
                elif fault == "unobserved": processes["observations"][0]["observation"]["unobserved"] = 1
                elif fault == "failure": processes["observations"][0]["observation"]["failure"] = "denied"
                with self.subTest(fault=fault), self.assertRaisesRegex(RuntimeError,"Darwin predecessor"):
                    UPGRADE.darwin_client_exit(123, -UPGRADE.DARWIN_SIGKILL, image, bad)

    def test_native_cleanup_cannot_pass_with_remaining_payload_or_incomplete_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            install = Path(temporary)
            active, old = "a" * 64, "b" * 64
            (install / "runtimes" / active).mkdir(parents=True)
            process_fields = {"remaining": 0, "observations": [], "image_roster": [], "image_exclusions": []}
            launcher = {"schema": 1, "active": active, "legacy_launchers_retired": True,
                        "launcher": None, "universal_launcher": None,
                        "processes": {**process_fields, "scope": "verified_launcher_images"}}
            retired = {"active": active, "complete": True, "selected_service_restarted": False,
                       "retired_runtimes": [old],
                       "processes": {**process_fields, "scope": "verified_superseded_runtime_images"}}
            result = UPGRADE.retire_obsolete_runtime(mock.Mock(side_effect=[launcher, retired]),
                                                    "airk", install, active, old)
            self.assertEqual(result["runtimes"], retired)
            leftover = install / "runtimes" / old
            leftover.mkdir()
            with self.assertRaisesRegex(RuntimeError, "Obsolete runtime payloads remain"):
                UPGRADE.retire_obsolete_runtime(mock.Mock(side_effect=[launcher, retired]),
                                               "airk", install, active, old)
            leftover.rmdir()
            for field, value in (("complete", False), ("selected_service_restarted", True),
                                 ("retired_runtimes", []), ("retired_runtimes", old),
                                 ("retired_runtimes", [old, old]), ("retired_runtimes", [old, active]),
                                 ("processes", {**retired["processes"], "remaining": False}),
                                 ("processes", {"remaining": 1})):
                with self.subTest(field=field), self.assertRaisesRegex(RuntimeError, "retirement is incomplete"):
                    UPGRADE.retire_obsolete_runtime(mock.Mock(side_effect=[launcher, {**retired, field: value}]),
                                                   "airk", install, active, old)

    def test_registry_migration_distinguishes_modern_inference_contract_and_rejects_downgrade(self):
        for version in (12, 13):
            UPGRADE.verify_registry_version(version, 12, 2)
        UPGRADE.verify_registry_version(12, 11, 1)
        for version, previous, schema in ((13, 12, 1), (12, 13, 2), (14, 12, 2),
                                          (11, 11, 2), (12, 12, 3)):
            with self.subTest(version=version, previous=previous, schema=schema):
                with self.assertRaisesRegex(RuntimeError, "Upgraded registry schema"):
                    UPGRADE.verify_registry_version(version, previous, schema)

    def test_darwin_runtime_resolution_keeps_main_in_its_bundle(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            main = root / "AIRK.app/Contents/MacOS/airk"
            main.parent.mkdir(parents=True); main.write_bytes(b"inert signed-layout fixture")
            (root/"airk").write_bytes(b"must not select an unsealed main copy")
            self.assertEqual(UPGRADE.runtime_binary(root,"darwin"),main)
            main.unlink()
            with self.assertRaisesRegex(RuntimeError,"sealed runtime main"):
                UPGRADE.runtime_binary(root,"darwin")

    def test_semantic_queries_refuse_lexical_fallback_and_changed_pipeline_or_source(self):
        original = b"original retained source\n"
        publication = {"token":"airk-r1:"+"1"*32+":4:"+"2"*64}
        body = {"execution":{"requested_mode":"vector","served_mode":"vector","incomplete":False},"publication":publication,
                "semantic_readiness":{"pipeline_digest":"a"*64},"hits":[{"citation":{"path":"README.md","content_hash":hashlib.sha256(original).hexdigest()},"text":original.decode()}]}
        UPGRADE.assert_semantic_query(body,"vector",publication,"a"*64,original)
        for field,value in (("execution",{"requested_mode":"vector","served_mode":"lexical","incomplete":False}),
                            ("execution",{"requested_mode":"vector","served_mode":"vector","incomplete":True}),
                            ("semantic_readiness",{"pipeline_digest":"b"*64}), ("publication",{"token":"new"}), ("hits",[])):
            with self.subTest(field=field), self.assertRaises(RuntimeError):
                UPGRADE.assert_semantic_query({**body,field:value},"vector",publication,"a"*64,original)

    def test_old_semantic_jobs_only_observe_the_original_accepted_identity(self):
        job = {"id":"a"*64,"project":"b"*64,"kind":"vectors","state":"pending"}
        rpc = mock.Mock(side_effect=[{"job":{**job,"state":"running"}},{"job":{**job,"state":"succeeded"}}])
        with mock.patch.object(UPGRADE.time,"sleep"):
            observed = UPGRADE.wait_semantic_job(rpc,{"status":"job","job":job},job["project"],"vectors")
        self.assertEqual(observed["completed"]["id"],job["id"])
        self.assertEqual(rpc.call_args_list,[mock.call({"job":{"id":job["id"]}})]*2)
        for changed in ({"id":"c"*64,"state":"succeeded"},{"state":"failed"}):
            rpc = mock.Mock(return_value={"job":{**job,**changed}})
            with self.assertRaises(RuntimeError):
                UPGRADE.wait_semantic_job(rpc,{"status":"job","job":job},job["project"],"vectors")
            self.assertEqual(rpc.call_count,1)

    def test_semantic_job_snapshot_detects_new_vector_work_while_retaining_full_outcomes(self):
        with tempfile.TemporaryDirectory() as temporary:
            database = Path(temporary).resolve() / "jobs.sqlite3"
            with closing(sqlite3.connect(database)) as sql, sql:
                sql.execute("CREATE TABLE service_jobs(seq INTEGER,id TEXT,project TEXT,kind TEXT,state TEXT,epoch INTEGER,attempts INTEGER,outcome BLOB)")
                sql.execute("INSERT INTO service_jobs VALUES (1,'saved','owned','vectors','succeeded',7,1,?)",(json.dumps({"future":{"retained":True}}).encode(),))
            before = UPGRADE.semantic_jobs(database,"owned")
            self.assertEqual(before[0]["outcome"],{"future":{"retained":True}})
            with closing(sqlite3.connect(database)) as sql, sql:
                sql.execute("INSERT INTO service_jobs VALUES (2,'unrequested','owned','index','pending',8,0,NULL)")
            self.assertNotEqual(UPGRADE.semantic_jobs(database,"owned"),before)
            self.assertEqual(UPGRADE.semantic_jobs(database,"other"),[])

    def test_model_reference_cannot_claim_minilm_using_different_actual_assets(self):
        catalog = json.loads((Path(__file__).parents[2] / "crates/airk-retrieval/src/model_catalog.json").read_bytes())
        with tempfile.TemporaryDirectory() as temporary:
            control = Path(temporary)
            root = control / "model-files-1.4"; model = root / "sets" / ("a"*64)
            model.mkdir(parents=True)
            selected = {"shared":{"set":"a"*64},"legacy_identity":"b"*64,"pipeline_digest":"c"*64,"assets":{}}
            contents = {"model.json":json.dumps(catalog["profiles"][0]["manifest"]).encode(),"model.onnx":b"not the approved weights","tokenizer.json":b"{}","LICENSE":b"fictional test license"}
            for name,content in contents.items():
                (model/name).write_bytes(content)
                selected["assets"][name] = {"bytes":len(content),"sha256":hashlib.sha256(content).hexdigest()}
            (root / "default-fixture.json").write_text(json.dumps(selected))
            with self.assertRaisesRegex(RuntimeError,"assets do not match"):
                UPGRADE.retained_model(control)

    def test_owned_git_rules_preserve_local_corpus_and_refuse_changed_config(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            local = root / ".gitignore"
            local.write_text("private-fixture.txt\n")
            original = dict(UPGRADE.os.environ)
            environment = UPGRADE.isolated_git_environment(root)
            self.assertEqual(dict(UPGRADE.os.environ),original)
            self.assertTrue(Path(environment["GIT_CONFIG_GLOBAL"]).is_relative_to(root))
            self.assertEqual(environment["GIT_CONFIG_NOSYSTEM"],"1")
            self.assertEqual(local.read_text(),"private-fixture.txt\n")
            self.assertEqual(UPGRADE.isolated_git_environment(root),environment)
            config = Path(environment["GIT_CONFIG_GLOBAL"])
            config.write_text("unexpected preexisting rules")
            with self.assertRaisesRegex(RuntimeError,"changed"):
                UPGRADE.isolated_git_environment(root)
            self.assertEqual(config.read_text(),"unexpected preexisting rules")

    def test_https_reconnect_retries_only_typed_read_refusal_and_retains_every_reply(self):
        def envelope(value, failed=False):
            return {"isError":failed,"content":[{"type":"text","text":json.dumps(value)}]}
        retry = envelope({"status":"retry","retryable":True,"action":"repeat_same_request","code":"publication_busy","retry_after_ms":1},True)
        success = envelope({"hits":[],"future_field":{"retained":True}})
        retained = []
        read = mock.Mock(side_effect=[retry,success])
        with mock.patch.object(UPGRADE.time,"sleep"):
            self.assertEqual(UPGRADE.typed_https_read(read,retained.append,time.monotonic()+5),success)
        self.assertEqual(retained,[retry,success])
        self.assertEqual(read.call_count,2)
        self.assertEqual(read.call_args_list[0].args,read.call_args_list[1].args)
        submitted = envelope({"error":{"code":"direct_query_admission","reason":"busy","query_submitted":True,"retryable":True,"retry_after_ms":1}},True)
        retained = []
        read = mock.Mock(return_value=submitted)
        with self.assertRaises(AssertionError):
            UPGRADE.typed_https_read(read,retained.append,time.monotonic()+5)
        self.assertEqual(read.call_count,1)
        self.assertEqual(retained,[submitted])

    def test_https_comparison_preserves_all_envelope_fields_and_only_normalizes_known_timing(self):
        specification = importlib.util.spec_from_file_location("upgrade_http_parity", Path(__file__).parents[1] / "http_fixture.py")
        transport = importlib.util.module_from_spec(specification); specification.loader.exec_module(transport)
        left = {"content":[{"type":"text","text":json.dumps({"observed_unix_ms":1,"hits":[]})}],"structuredContent":{"request_id":"one","future":{"retained":True}},"future_top":{"schema":9}}
        right = json.loads(json.dumps(left)); right["content"][0]["text"] = json.dumps({"observed_unix_ms":2,"hits":[]}); right["structuredContent"]["request_id"] = "two"
        self.assertEqual(transport.parity_envelope(left),transport.parity_envelope(right))
        for key in ("structuredContent","future_top"):
            changed = json.loads(json.dumps(right)); changed[key] = {}
            self.assertNotEqual(transport.parity_envelope(left),transport.parity_envelope(changed))
        self.assertIsInstance(left["content"][0]["text"],str)

    def test_reviewed_fixture_uses_only_explicit_native_setup_and_one_source_grant(self):
        with tempfile.TemporaryDirectory() as parent:
            source, data = Path(parent) / "source", Path(parent) / "data"
            source.mkdir(); data.mkdir()
            calls = []
            registration = {"binding": "fixture", "root": str(source), "data_root": str(data), "collection": "upgrade"}
            revision = 1
            def rpc(method):
                nonlocal revision
                calls.append(method)
                if method == "request_epoch": return {"namespace": "native", "epoch": 7}
                name, body = next(iter(method.items()))
                if name == "catalog_host_context": return {"descriptor": {"namespace": "native", "catalog_revision": revision}}
                if name == "catalog_index_setup_draft": return {"draft": {"native": "unchanged"}}
                if name == "catalog_index_setup_apply":
                    self.assertEqual(body["draft"], {"native": "unchanged"}); revision += 1
                    return {"status": "catalog_index_setup_applied", "receipt": {"stage": "ready", "catalog_revision": revision, "index_id": "idx_fixture", "source_ids": ["src_fixture"]}}
                if name == "catalog_preview": return {"preview_digest": "native-preview"}
                if name == "catalog_apply":
                    self.assertEqual(body["preview_digest"], "native-preview"); revision += 1
                    return {"status": "catalog_applied", "receipt": {"catalog_revision": revision}}
                if name == "catalog_grant_draft": return {"draft": {"request": {"operation": {"grant": {"id": "grt_fixture"}}}, "preview_digest": "native-preview"}}
                if name == "catalog_detail": return {"detail": {"item": {"value": {"backing": registration}}}}
                self.fail(f"Unexpected fixture mutation: {method}")
            actual, selected = UPGRADE.reviewed_fixture(rpc, source, data)
            self.assertEqual(actual, registration)
            setup = next(row["catalog_index_setup_draft"]["selection"] for row in calls if isinstance(row, dict) and "catalog_index_setup_draft" in row)
            self.assertEqual((setup["primary_root"], setup["data_root"], setup["additional_roots"], setup["setup_watch"]), (str(source), str(data), [], False))
            member = next(row["catalog_preview"]["request"]["operation"]["membership"] for row in calls if isinstance(row, dict) and "catalog_preview" in row and row["catalog_preview"]["request"]["operation"]["operation"] == "set_membership")
            self.assertEqual(member["scope"], [{"source_root_id": "src_fixture", "kind": "subtree", "path": ""}])
            grant = next(row["catalog_grant_draft"]["selection"] for row in calls if isinstance(row, dict) and "catalog_grant_draft" in row)
            self.assertEqual(grant["index_ids"], ["idx_fixture"])
            self.assertEqual(grant["project_ids"], selected["project_ids"])
            self.assertEqual(len(selected["project_ids"]), 1)

    def test_reviewed_snapshot_preserves_full_values_and_refuses_incomplete_membership(self):
        selected = {"context_key": "context", "namespace": "native", "index_id": "index", "project_ids": ["project"], "grant_id": "grant"}
        continuation = None
        def rpc(method):
            name, body = next(iter(method.items()))
            if name == "catalog_host_context": return {"descriptor": {"namespace": "native", "catalog_revision": 9}, "capabilities": {"policy_contract": 3}}
            if name == "catalog_detail": return {"detail": {"namespace": "native", "revision": 9, "item": {"kind": body["entity"]["kind"], "value": {"future": [1, 2], "id": body["entity"]["id"]}}}}
            if name == "catalog_page": return {"page": {"namespace": "native", "revision": 9, "continuation": continuation, "items": [{"future_member": {"retained": True}}]}}
            self.fail(str(method))
        actual, capabilities = UPGRADE.reviewed_snapshot(rpc, selected)
        self.assertEqual(actual["grant"], {"future": [1, 2], "id": "grant"})
        self.assertEqual(actual["memberships"], [{"future_member": {"retained": True}}])
        self.assertEqual(capabilities, {"policy_contract": 3})
        continuation = {"next": "unread"}
        with self.assertRaisesRegex(RuntimeError, "incomplete"):
            UPGRADE.reviewed_snapshot(rpc, selected)

    def test_failure_cleanup_never_masks_original_refusal_or_deletes_fixture(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent))
            root = Path(temporary.name)
            (root / "install").mkdir()
            sentinel = root / "diagnostic.json"
            sentinel.write_text('{"native":"original-refusal"}')
            def failed_stop():
                raise RuntimeError("secondary stop failure")
            with self.assertRaisesRegex(RuntimeError, "original ConfigurationUnavailable"):
                try:
                    raise RuntimeError("original ConfigurationUnavailable")
                finally:
                    observed = UPGRADE.finish_fixture(temporary, root, False,
                        lambda: self.fail("failed fixture must not uninstall"), observe_stop=failed_stop)
            self.assertTrue(observed["fixture_retained"])
            self.assertEqual(observed["errors"][0]["operation"], "observe_owned_candidate_stop")
            self.assertEqual(sentinel.read_text(), '{"native":"original-refusal"}')

    def test_pending_committed_activation_never_auto_uninstalls_or_aborts(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent)); root = Path(temporary.name)
            install = root / "install"; install.mkdir()
            (install / "activation.json").write_text('{"phase":"probe"}')
            observed = UPGRADE.finish_fixture(temporary, root, True,
                lambda: self.fail("pending activation must not uninstall"))
            self.assertTrue(observed["activation_pending"])
            self.assertTrue(observed["fixture_retained"])
            self.assertFalse(observed["teardown_complete"])
            self.assertTrue((install / "activation.json").is_file())

    def test_uninstall_failure_retains_previously_completed_assertion_evidence(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent)); root = Path(temporary.name)
            (root / "install").mkdir()
            def refuse_uninstall(): raise RuntimeError("teardown refused")
            observed = UPGRADE.finish_fixture(temporary, root, True, refuse_uninstall)
            self.assertTrue(root.is_dir())
            self.assertFalse(observed["teardown_complete"])
            self.assertEqual(observed["errors"][0]["operation"], "uninstall")

    def test_success_removes_only_owned_fixture_after_acknowledged_uninstall(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent)); root = Path(temporary.name)
            (root / "install").mkdir(); sibling = Path(parent) / "unrelated"; sibling.mkdir()
            calls = []
            observed = UPGRADE.finish_fixture(temporary, root, True, lambda: (calls.append("uninstall") or {"uninstalled": True}))
            self.assertEqual(calls, ["uninstall"])
            self.assertTrue(observed["teardown_complete"])
            self.assertFalse(root.exists())
            self.assertTrue(sibling.is_dir())

    def test_missing_install_never_counts_as_acknowledged_uninstall(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent)); root = Path(temporary.name)
            observed = UPGRADE.finish_fixture(temporary, root, True,
                lambda: self.fail("missing install cannot be uninstalled"))
            self.assertFalse(observed["uninstall_acknowledged"])
            self.assertFalse(observed["teardown_complete"])
            self.assertTrue(root.is_dir())
            self.assertIn("missing", observed["errors"][0]["error"])

    def test_deferred_uninstall_preserves_fixture_and_its_unknown_state(self):
        with tempfile.TemporaryDirectory() as parent:
            temporary = UPGRADE.owned_temporary(Path(parent)); root = Path(temporary.name)
            (root / "install").mkdir()
            observed = UPGRADE.finish_fixture(temporary, root, True,
                lambda: {"uninstalled": False, "deferred_runtimes": ["owned"]})
            self.assertFalse(observed["uninstall_acknowledged"])
            self.assertTrue(root.is_dir())

    def test_native_timeout_retains_exact_partial_streams_and_original_exception(self):
        with tempfile.TemporaryDirectory() as parent:
            root = Path(parent); records = []
            error = subprocess.TimeoutExpired(["owned.exe", "runtime", "activate"], 1,
                output=b"partial native stdout", stderr=b"original activation diagnostics")
            with mock.patch.object(UPGRADE.subprocess, "run", side_effect=error):
                with self.assertRaises(subprocess.TimeoutExpired) as observed:
                    UPGRADE.record_native_command(root, records, "owned.exe", ["runtime", "activate"], timeout=1)
            self.assertIs(observed.exception, error)
            self.assertEqual((root / "command-001.stdout").read_bytes(), b"partial native stdout")
            self.assertEqual((root / "command-001.stderr").read_bytes(), b"original activation diagnostics")
            self.assertEqual(records[0]["failure_type"], "TimeoutExpired")
            self.assertIsNone(records[0]["exit_code"])

if __name__ == "__main__": unittest.main()
