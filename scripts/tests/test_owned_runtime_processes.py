# SPDX-License-Identifier: Apache-2.0
"""Process census fixture contracts; these tests never run AIRK or signal a PID."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import sys
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("owned_processes", Path(__file__).parents[1] / "owned-runtime-processes.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def digest(data):
    return hashlib.sha256(data).hexdigest()


class Owner:
    def __init__(self, state):
        self.state = state
        self.closed = False

    def gone(self):
        return self.state.get("gone", False)

    def observation(self):
        return dict(self.state["row"])

    def close(self):
        self.closed = True


class Backend:
    user = "owned-user"

    def __init__(self, states):
        self.states = states
        self.opened = []

    def pids(self):
        return list(self.states)

    def open(self, pid):
        state = self.states[pid]
        if isinstance(state, BaseException):
            raise state
        owner = Owner(state)
        self.opened.append(owner)
        return owner


class CensusTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.install = Path(self.temporary.name).resolve() / "install"
        self.install.mkdir()
        self.main = b"owned-old-main"
        self.helper = b"owned-old-helper"
        self.pair = {"schema": 1, "version": "1.8.12", "os": "linux", "arch": "x86_64",
                     "airk_sha256": digest(self.main), "embed_sha256": digest(self.helper)}
        pair_raw = json.dumps(self.pair).encode()
        manifest = {"schema": 1, "version": "1.8.12", "os": "linux", "arch": "x86_64",
                    "files": {"airk": digest(self.main), "airk-embed": digest(self.helper),
                              "airk-runtime.json": digest(pair_raw)}}
        self.manifest_raw = json.dumps(manifest).encode()
        self.identity = digest(self.manifest_raw)
        self.runtime = self.install / "runtimes" / self.identity
        self.runtime.mkdir(parents=True)
        for name, data in [("airk", self.main), ("airk-embed", self.helper),
                           ("airk-runtime.json", pair_raw), ("airk-install.json", self.manifest_raw)]:
            (self.runtime / name).write_bytes(data)
        self.state = {"row": {"pid": 101, "start": "immutable-start", "user": "owned-user", "path": str(self.runtime / "airk")}}
        self.backend = Backend({101: self.state})
        self.patch = mock.patch.object(MODULE, "_backend", return_value=self.backend)
        self.patch.start()
        self.addCleanup(self.patch.stop)

    def capture(self, required=None):
        return MODULE.capture(self.install, self.identity, {101} if required is None else required)

    def test_actual_inventory_and_held_identity_capture_then_exit_are_reported(self):
        with self.capture() as census:
            row = census.before["processes"][0]
            self.assertEqual(row, {**self.state["row"], "executable_sha256": digest(self.main)})
            self.assertEqual(census.before["user"], "owned-user")
            self.assertEqual(census.before["global_completeness"], "not_certified")
            self.state["gone"] = True
            result = census.verify_gone(timeout=0)
            self.assertTrue(result["captured_identities_exited"])
            self.assertEqual(result["signals_sent"], 0)
            self.assertEqual(result["after"]["processes"], [])
        self.assertTrue(all(owner.closed for owner in self.backend.opened))

    def test_live_held_identity_is_not_pid_exit_evidence_and_close_never_stops_it(self):
        census = self.capture()
        self.backend.states.clear()  # A changed PID list cannot prove kernel-object exit.
        with self.assertRaisesRegex(RuntimeError, "still alive"):
            census.verify_gone(timeout=0)
        census.close()
        self.assertFalse(self.state.get("gone", False))
        self.assertTrue(all(owner.closed for owner in self.backend.opened))
        with self.assertRaisesRegex(RuntimeError, "closed census"):
            census.verify_gone()

    def test_pid_reuse_is_safe_but_new_exact_image_survivors_refuse_success(self):
        with self.capture() as census:
            self.state["gone"] = True
            self.backend.states[101] = {"row": {**self.state["row"], "start": "new-incarnation"}}
            with self.assertRaisesRegex(RuntimeError, "survivors"):
                census.verify_gone(timeout=0)
            self.backend.states[101]["row"]["path"] = str(self.install / "unrelated")
            result = census.verify_gone(timeout=0)
            self.assertEqual(result["captured_count"], 1)

    def test_required_pid_missing_or_wrong_user_fails_and_releases_all_handles(self):
        with self.assertRaisesRegex(RuntimeError, "not captured"):
            self.capture({999})
        self.assertTrue(all(owner.closed for owner in self.backend.opened))
        self.state["row"]["user"] = "someone-else"
        with self.assertRaisesRegex(RuntimeError, "another user"):
            self.capture()
        self.assertTrue(all(owner.closed for owner in self.backend.opened))

    def test_unobservable_other_processes_are_counted_without_claiming_completeness(self):
        self.backend.states[202] = PermissionError(13, "unavailable")
        with self.capture() as census:
            self.assertEqual(census.before["unobserved"], 1)
            self.assertEqual(census.before["enumerated"], 2)
            self.assertEqual(census.before["global_completeness"], "not_certified")

    def test_known_exact_image_disappearing_during_hash_is_not_silently_ignored(self):
        original = MODULE._digest
        calls = 0
        def changing(path):
            nonlocal calls
            calls += 1
            if calls > 2:
                raise FileNotFoundError(2, "gone")
            return original(path)
        with mock.patch.object(MODULE, "_digest", side_effect=changing):
            with self.assertRaisesRegex(RuntimeError, "identity became unavailable"):
                self.capture()

    def test_changed_mapped_identity_or_seal_is_not_accepted(self):
        original = Owner.observation
        calls = 0
        def changing(owner):
            nonlocal calls
            calls += 1
            row = original(owner)
            if calls > 1:
                row["start"] = "changed"
            return row
        with mock.patch.object(Owner, "observation", changing):
            with self.assertRaisesRegex(RuntimeError, "identity changed"):
                self.capture()

    def test_process_and_match_limits_fail_closed(self):
        with mock.patch.object(self.backend, "pids", return_value=list(range(MODULE.MAX_SCANNED + 1))):
            with self.assertRaisesRegex(RuntimeError, "scan bound"):
                self.capture()
        self.backend.states = {pid: {"row": {**self.state["row"], "pid": pid}} for pid in range(1, 66)}
        with self.assertRaisesRegex(RuntimeError, "match bound"):
            self.capture(set())
        self.assertTrue(all(owner.closed for owner in self.backend.opened))

    def test_inventory_digest_metadata_and_image_seals_are_required(self):
        (self.runtime / "airk-install.json").write_bytes(self.manifest_raw + b" ")
        with self.assertRaisesRegex(RuntimeError, "inventory digest"):
            self.capture()
        (self.runtime / "airk-install.json").write_bytes(self.manifest_raw)
        (self.runtime / "airk-runtime.json").write_text("{}")
        with self.assertRaisesRegex(RuntimeError, "inventory-bound"):
            self.capture()

    def test_modified_alias_or_launcher_is_preserved_and_blocks_capture(self):
        alias = self.runtime / "airk-service"
        alias.write_bytes(b"unknown")
        with self.assertRaisesRegex(RuntimeError, "seal mismatch"):
            self.capture()
        self.assertEqual(alias.read_bytes(), b"unknown")
        alias.unlink()
        launcher = self.install / "airk-launcher"
        launcher.write_bytes(b"launcher")
        (self.install / "launcher.json").write_text(json.dumps({"schema": 1, "sha256": digest(b"different")}))
        with self.assertRaisesRegex(RuntimeError, "seal mismatch"):
            self.capture()
        self.assertEqual(launcher.read_bytes(), b"launcher")

    def test_postscan_retired_launcher_is_allowed_but_resurrected_path_is_not(self):
        launcher = self.install / "airk-launcher"
        launcher.write_bytes(b"launcher")
        (self.install / "launcher.json").write_text(json.dumps({"schema": 1, "sha256": digest(b"launcher")}))
        with self.capture() as census:
            self.state["gone"] = True
            launcher.unlink()
            self.assertTrue(census.verify_gone(timeout=0)["captured_identities_exited"])
            launcher.write_bytes(b"different-current-image")
            self.backend.states[202] = {"row": {**self.state["row"], "pid": 202, "path": str(launcher)}}
            with self.assertRaisesRegex(RuntimeError, "seal changed"):
                census.verify_gone(timeout=0)

    def test_postscan_requires_immutable_old_runtime_even_after_all_processes_exit(self):
        with self.capture() as census:
            self.state["gone"] = True
            (self.runtime / "airk").write_bytes(b"changed")
            with self.assertRaisesRegex(RuntimeError, "rollback image changed"):
                census.verify_gone(timeout=0)

    def test_correct_alias_and_launcher_are_part_of_exact_image_census(self):
        (self.runtime / "airk-source").write_bytes(self.main)
        launcher = self.install / "airk-launcher"
        launcher.write_bytes(b"launcher")
        (self.install / "launcher.json").write_text(json.dumps({"schema": 1, "sha256": digest(b"launcher")}))
        self.backend.states[202] = {"row": {**self.state["row"], "pid": 202, "path": str(launcher)}}
        with self.capture({101, 202}) as census:
            self.assertEqual(len(census.before["processes"]), 2)

    def test_hardlinks_and_symlinks_are_rejected_without_removal(self):
        import os
        original = self.runtime / "airk"
        hardlink = self.runtime / "alias-hardlink"
        os.link(original, hardlink)
        with self.assertRaisesRegex(RuntimeError, "regular file"):
            self.capture()
        self.assertTrue(hardlink.exists())
        hardlink.unlink()
        link = self.runtime / "airk-index"
        try:
            link.symlink_to(original)
        except OSError:
            return  # Windows developer-mode privilege is not required by unit tests.
        with self.assertRaisesRegex(RuntimeError, "canonical|link"):
            self.capture()
        self.assertTrue(link.is_symlink())

    def test_invalid_wait_and_required_pid_values_are_refused(self):
        for value in [{True}, {-1}, [101], set(range(65))]:
            with self.assertRaisesRegex(RuntimeError, "required predecessor"):
                self.capture(value)
        with self.capture() as census:
            for value in [-1, 61, "10", True, float("nan"), float("inf")]:
                with self.assertRaisesRegex(RuntimeError, "wait bound"):
                    census.verify_gone(value)


class OperatingSystemIdentityTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform == "win32", "Windows query-only process handle contract")
    def test_windows_held_current_process_has_stable_start_path_and_native_user(self):
        backend = MODULE._Windows()
        owner = backend.open(os.getpid())
        try:
            row = owner.observation()
            self.assertEqual(row["pid"], os.getpid())
            self.assertEqual(row["user"], backend.user)
            self.assertEqual(MODULE._key(row["path"]), MODULE._key(sys.executable))
            self.assertGreater(int(row["start"]), 0)
            self.assertEqual(row, owner.observation())
            self.assertFalse(owner.gone())
        finally:
            owner.close()
        self.assertIsNone(owner.handle)

    @unittest.skipUnless(sys.platform == "linux" and hasattr(os, "pidfd_open"), "Linux held pidfd contract")
    def test_linux_held_current_process_has_stable_start_path_and_native_user(self):
        backend = MODULE._Linux()
        owner = backend.open(os.getpid())
        try:
            row = owner.observation()
            self.assertEqual(row["pid"], os.getpid())
            self.assertEqual(row["user"], backend.user)
            self.assertEqual(Path(row["path"]).resolve(), Path(sys.executable).resolve())
            self.assertGreater(int(row["start"]), 0)
            self.assertEqual(row, owner.observation())
            self.assertFalse(owner.gone())
        finally:
            owner.close()
        self.assertIsNone(owner.fd)


if __name__ == "__main__":
    unittest.main()
