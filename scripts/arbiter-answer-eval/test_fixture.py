# SPDX-License-Identifier: Apache-2.0
"""Synthetic checks for the Answer-evaluation fixture launcher; no service starts."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("answer_eval_fixture", HERE / "fixture.py")
fixture = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fixture)


class DispatchCensus(unittest.TestCase):
    def test_census_is_a_whole_number_within_the_service_ceiling(self):
        self.assertEqual(fixture.census_bound("38016"), 38016)
        self.assertEqual(fixture.census_bound(65536), 65536)
        for bad in ("0", "65537", "-1", "+5", "1e4", " 10", "", "x"):
            with self.assertRaises(argparse.ArgumentTypeError, msg=bad):
                fixture.census_bound(bad)

    def test_worker_variables_carry_the_census_only_with_the_observer(self):
        self.assertEqual(fixture.census_extras(True, None), {"AIRK_NATIVE_DISPATCH_OBSERVER": "true"})
        self.assertEqual(fixture.census_extras(True, 38016),
                         {"AIRK_NATIVE_DISPATCH_OBSERVER": "true", "AIRK_NATIVE_DISPATCH_CENSUS_BOUND": "38016"})
        self.assertEqual(fixture.census_extras(False, None), {"AIRK_NATIVE_DISPATCH_OBSERVER": "false"})
        with self.assertRaises(Exception):
            fixture.census_extras(False, 38016)


class Clock:
    """An injected clock: it moves only when the test says so."""

    def __init__(self):
        self.now = 1000.0

    def __call__(self):
        return self.now


class ReadyReceipt(unittest.TestCase):
    """The service writes ready.json in place; a reader must wait for a complete, parseable file (LH, 23 Nov)."""

    COMPLETE = {"health": {"process_id": 7}, "inputs_sha256": "ab" * 32, "profiles": []}

    def test_partial_then_complete_file(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "ready.json"
            observe = fixture.complete_json_observer(path, clock=Clock())
            self.assertIsNone(observe())  # absent
            whole = json.dumps(self.COMPLETE).encode()
            path.write_bytes(whole[:25])  # half written: not parseable
            self.assertIsNone(observe())
            path.write_bytes(whole[:0])  # created, nothing written yet
            self.assertIsNone(observe())
            path.write_bytes(whole)
            self.assertEqual(observe(), self.COMPLETE)

    def test_a_multibyte_character_cut_in_two_is_not_ready(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "ready.json"
            whole = json.dumps({"note": "café"}, ensure_ascii=False).encode("utf-8")
            path.write_bytes(whole[:whole.index("é".encode("utf-8")) + 1])
            observe = fixture.complete_json_observer(path, clock=Clock())
            self.assertIsNone(observe())
            path.write_bytes(whole)
            self.assertEqual(observe(), {"note": "café"})

    def test_a_file_held_by_its_writer_is_waited_for(self):
        # errno 13 is what a Windows sharing violation raises.
        held = [PermissionError(13, "Permission denied")] * 2
        whole = json.dumps(self.COMPLETE).encode()

        def read(path):
            if held:
                raise held.pop()
            return whole

        observe = fixture.complete_json_observer(Path("ready.json"), clock=Clock(), read=read)
        self.assertIsNone(observe())
        self.assertIsNone(observe())
        self.assertEqual(observe(), self.COMPLETE)

    def test_a_file_that_stays_unreadable_fails_after_the_grace_with_its_last_error(self):
        clock = Clock()

        def read(path):
            raise PermissionError(13, "Permission denied")

        observe = fixture.complete_json_observer(Path("ready.json"), clock=clock, grace=60, read=read)
        self.assertIsNone(observe())  # first seen at t0
        clock.now += 60
        self.assertIsNone(observe())  # exactly the grace: still waiting
        clock.now += 0.5
        with self.assertRaises(ValueError) as caught:
            observe()
        self.assertIn("PermissionError", str(caught.exception))
        self.assertIn("60 s", str(caught.exception))

    def test_an_absent_file_never_counts_against_the_grace(self):
        clock = Clock()
        observe = fixture.complete_json_observer(Path("no-such-folder") / "ready.json", clock=clock, grace=1)
        for _ in range(3):
            clock.now += 3600
            self.assertIsNone(observe())

    def test_other_read_errors_are_not_swallowed(self):
        def read(path):
            raise IsADirectoryError(21, "is a directory")

        with self.assertRaises(OSError):
            fixture.complete_json_observer(Path("ready.json"), clock=Clock(), read=read)()

    def test_worker_waits_through_the_observer_and_keeps_its_validations(self):
        # The worker takes the parsed receipt from the wait (no second read) and checks it as before.
        source = (HERE / "fixture.py").read_text(encoding="utf-8")
        start = source.index("def worker(args):")
        worker = source[start:source.index("\ndef ", start + 1)]
        self.assertIn('complete_json_observer(work / "ready.json")', worker)
        self.assertIn("file_changes=READY_CHANGES", worker)
        self.assertNotIn('ready = read_json(work / "ready.json")', worker)
        for check in ('ready["health"]["process_id"] == process.pid', 'ready["health"]["executable_sha256"]',
                      'ready["inputs_sha256"] == inputs["sha256"]', 'control.samefile(ready["control_root"])',
                      "ready receipt differs from the launched fixture"):
            self.assertIn(check, worker)

    @unittest.skipUnless(os.name == "nt", "the native directory-change wait is Windows only")
    def test_native_wait_returns_the_completed_file_after_a_partial_write(self):
        # The real wait, driven by events: the writer completes the file only after the observer has seen it
        # half written, so there is no timing to guess.
        nat = fixture.native()
        whole = json.dumps(self.COMPLETE).encode()
        with tempfile.TemporaryDirectory() as folder:
            work = Path(folder)
            path = work / "ready.json"
            inner = fixture.complete_json_observer(path)
            saw_partial = threading.Event()

            def observe():
                value = inner()
                if value is None and path.is_file():
                    saw_partial.set()
                return value

            def writer():
                path.write_bytes(whole[:20])
                saw_partial.wait(60)
                with path.open("ab") as stream:
                    stream.write(whole[20:])

            child = subprocess.Popen([sys.executable, "-I", "-c", "import time; time.sleep(120)"],
                                     stdin=subprocess.DEVNULL)
            thread = threading.Thread(target=writer)
            try:
                thread.start()
                result = nat["wait_observed"](work, observe, child, fixture.time.monotonic() + 60, "fixture readiness",
                                              file_changes=fixture.READY_CHANGES)
            finally:
                thread.join(60)
                child.kill()
                child.wait()
            self.assertEqual(result, self.COMPLETE)


class Exclusions(unittest.TestCase):
    def test_exact_and_subtree_rules(self):
        for path in ("docs/arbiter/final-plan-fp16.md", "docs/arbiter/answer-evaluation/README.md",
                     "docs/arbiter/answer-evaluation/a/b.md", "docs/simplify/one-process-ledger.md",
                     "docs/agent-current-work.md", "scripts/arbiter-answer-eval/fixture.py",
                     "docs\\agent-current-work.md"):
            self.assertTrue(fixture.excluded(path), path)
        for path in ("docs/arbiter/final-plan.md", "docs/arbiter/answer-evaluation-notes.md",
                     "docs/simplify/one-process-ledger.md.bak", "scripts/arbiter-answer-evaluation/x.py",
                     "docs/agent-current-work.md/x", "README.md"):
            self.assertFalse(fixture.excluded(path), path)

    def test_bookbuilder_has_no_benchmark_rules(self):
        self.assertFalse(fixture.excluded("docs/agent-current-work.md", "bookbuilder"))

    def test_fresh_bookbuilder_index_is_a_denied_snapshot(self):
        """Regression (22 Nov 2026): bookbuilder-v2 is exported at the scenario commit and denied before indexing."""
        self.assertEqual(fixture.EXCLUSIONS["bookbuilder-v2"], fixture.EXCLUSIONS["bookbuilder"])
        self.assertEqual(fixture.SNAPSHOTS["bookbuilder-v2"], "238b22a722ceb78447d72e39585fd6a7d4d3dd0c")
        self.assertEqual(fixture.REPOSITORIES["bookbuilder-v2"], fixture.REPOSITORIES["bookbuilder"])
        self.assertTrue(fixture.excluded("ironspire/scripts/set-rss-key.ps1", "bookbuilder-v2"))
        self.assertEqual(fixture.snapshot_root("bookbuilder-v2", Path("F")).name, "bookbuilder-v2-238b22a722ce")

    def test_bookbuilder_denies_the_credential_scripts(self):
        """Regression (22 Nov 2026): the two set-*-key.ps1 scripts the credential probe found stay denied."""
        for path in ("ironspire/scripts/set-rss-key.ps1", "ironspire/scripts/set-sendgrid-key.ps1",
                     "ironspire\\scripts\\set-sendgrid-key.ps1"):
            self.assertTrue(fixture.excluded(path, "bookbuilder"), path)
        for path in ("ironspire/scripts/set-rss-key.ps1.md", "ironspire/scripts/other.ps1"):
            self.assertFalse(fixture.excluded(path, "bookbuilder"), path)
        self.assertFalse(fixture.excluded("ironspire/scripts/set-rss-key.ps1", "airk"))
        self.assertEqual(sorted(fixture.EXCLUSIONS["bookbuilder"]),
                         [("exact", "ironspire/scripts/set-rss-key.ps1"),
                          ("exact", "ironspire/scripts/set-sendgrid-key.ps1")])


class Paths(unittest.TestCase):
    def test_hit_paths_finds_nested_locations(self):
        value = {"hits": [{"path": "a.rs", "citation": {"file": "b.rs"}}, {"records": [{"relative_path": "c"}]}]}
        self.assertEqual(sorted(fixture.hit_paths(value)), ["a.rs", "b.rs", "c"])

    def test_denied_atoms_reads_every_deny(self):
        status = {"policy": {"denies": [
            {"rule_id": "r1", "kind": "exact", "path": "docs/x.md", "reason": "trim"},
            {"rule_id": "r2", "kind": "subtree", "path": "scripts/y", "reason": "trim"}]}}
        atoms, _ = fixture.denied_atoms(status)
        self.assertEqual(atoms, {("exact", "docs/x.md"), ("subtree", "scripts/y")})

    def test_lifetime_names_are_plain(self):
        self.assertEqual(fixture.checked_name("L2-restart_1.a"), "L2-restart_1.a")
        for bad in ("", "../x", "a/b", ".hidden", "a b", "x" * 65):
            with self.assertRaises(ValueError):
                fixture.checked_name(bad)


class Requests(unittest.TestCase):
    def test_emitter_environment_declares_the_same_read_only_roots(self):
        handle = {"service_environment": {"AIRK_ARBITER_EVALUATION_WORK": "W", "TEMP": "T"}}
        scopes = {"path": "S", "sha256": "cd" * 32}
        # Absolute roots, so the check does not depend on the working directory: a worktree that lies under
        # ``artifacts`` would otherwise resolve relative roots into it. A snapshot root under ``artifacts``
        # (munarium, bookbuilder-v2) is not declared; a checkout outside it is.
        with tempfile.TemporaryDirectory() as outside:
            roots = [str(Path(outside) / "R1"), str(Path(outside) / "R2")]
            inside = str(fixture.ROOT / "artifacts" / "x" / "sources" / "munarium-9948345d1d14")
            env = fixture.emitter_environment(handle, scopes, "focus", False, "ab" * 32, "p-1", roots + [inside])
        self.assertEqual(env["AIRK_ARBITER_EVALUATION_WORK"], "W")
        self.assertEqual(env["AIRK_ARBITER_EVALUATION_ENABLED"], "false")
        self.assertEqual(env["AIRK_ARBITER_EVALUATION_SCOPES_SHA256"], "cd" * 32)
        self.assertEqual(json.loads(env["AIRK_ARBITER_EVALUATION_EXTERNAL_SOURCES"]),
                         {"mode": "read-only", "source_roots": roots})
        self.assertFalse(hasattr(fixture, "configure_request"))

    def test_gpu_profiles_are_hooks_that_refuse(self):
        with tempfile.TemporaryDirectory() as folder:
            for name in ("focus-static1024-cuda", "context-compact-cuda", "context-large-cuda"):
                with self.assertRaises(NotImplementedError):
                    fixture.set_stage(Path(folder), {}, name, True, [])

    def test_unwrap_accepts_bare_and_wrapped_answers(self):
        self.assertEqual(fixture.unwrap({"request_epoch": {"epoch": 1}}, "request_epoch"), {"epoch": 1})
        self.assertEqual(fixture.unwrap({"response": {"request_epoch": 2}}, "request_epoch"), 2)
        with self.assertRaises(ValueError):
            fixture.unwrap({"failure": {"code": "x"}}, "request_epoch")


class ChangeSetProfiles(unittest.TestCase):
    """Regression (22 Nov 2026): the change-set-1 catalog-only profiles must be nameable by `stage`."""

    def test_cs1_profiles_have_cpu_manifests_and_the_right_stage(self):
        rows = {name: row for name, row in fixture.PROFILES.items() if "-cs1-" in name}
        self.assertEqual(sorted(rows), ["context-compact-cpu-cs1-provisional-read",
                                        "context-large-cpu-cs1-provisional-read",
                                        "focus-static1024-cpu-cs1-publisher-w16",
                                        "focus-static1024-cpu-cs1-software-w16"])
        for name, row in rows.items():
            self.assertEqual(row["device"], "cpu")
            self.assertEqual(row["stage"], name.split("-")[0])
            self.assertRegex(row["manifest"], r"^[0-9a-f]{64}$")
        cand1 = fixture.PROFILES["focus-static1024-cpu-cand1-software-w16"]
        self.assertEqual((cand1["stage"], cand1["device"]), ("focus", "cpu"))
        self.assertEqual(cand1["manifest"], "c3cc396a0e85521f53ff186f753330b2f6f1a632f79909f3202dc640079e98c4")
        for name, manifest in (("focus-static1024-cpu-cand2-software-w16",
                                "994e9cd75837169fb2994ef4244759d81a97753aa4a38f09c1110a3f96397c33"),
                               ("context-compact-cpu-cs2-context-b",
                                "c393cac2c6d7785143b92784f16e4d4e55600efed14eec9c9f19f2549a012504"),
                               ("context-large-cpu-cs2-context-b",
                                "4fd52d861dcbd11e9b5ab2a4f9d072c34e13ef0fc97f09e224fe017a1f0dbef1")):
            self.assertEqual(fixture.PROFILES[name]["manifest"], manifest)
            self.assertEqual(fixture.PROFILES[name]["stage"], name.split("-")[0])
        manifests = [row["manifest"] for row in fixture.PROFILES.values() if row["manifest"]]
        self.assertEqual(len(manifests), len(set(manifests)))


class FakeService:
    """A scripted service for the source-policy tests: it keeps the denies in force, the jobs each project
    holds per ``progress`` sample, and the answers the next reviews and applies give. Every call is logged."""

    def __init__(self, folder, drafts=(), applies=(), progress=(), in_force_after=0):
        self.folder, self.prefix, self.count, self.stop_file = Path(folder), "t", 0, None
        self.deadline = fixture.time.monotonic() + 3600
        self.drafts, self.applies, self.progress = list(drafts), list(applies), list(progress)
        self.denies, self.pending, self.log = [], [], []
        self.in_force_after = in_force_after  # policy-status reads before a pending change comes into force
        self.indexed, self.waited, self.auto_index = [], [], False
        self.job_states = {}  # a job id -> the state `jobs get` reports (default succeeded)

    def lines(self, label, *arguments):
        self.log.append(label)
        return [{"draft": {}}, {"receipt": {"index_id": "idx_" + label, "request": {
            "registration": {"binding": "binding-" + label}}}}]

    def policy(self):
        if self.pending:
            if self.in_force_after <= 0:
                self.denies += self.pending
                self.pending = []
            else:
                self.in_force_after -= 1
        denies = [{"rule_id": f"r{n}", "kind": kind, "path": path, "reason": "trim"}
                  for n, (kind, path) in enumerate(self.denies)]
        return {"status": "catalog_policy_status",
                "snapshot": {"policy": {"denies": denies}, "application": {"removed_artifacts": 0} if denies else None,
                             "last_error": None},
                "job": {"state": "succeeded"} if denies else None}

    def json(self, label, *arguments):
        self.log.append(label)
        if label in ("setup-progress",):
            return {"queued_jobs": [{"id": "setup-index", "kind": "index", "state": "pending"}]}
        if label.startswith("install-minilm-"):
            self.auto_index = True  # the install starts the installing project's index pass by itself
            return {"job": {"id": "install"}}
        if label == "configure-job":
            return {"job": {"id": arguments[-1], "kind": "configure", "state": "succeeded",
                            "outcome": {"state": "succeeded"}}}
        if label == "install-progress":
            jobs = [{"id": "auto-index", "kind": "index", "state": "running"}] if self.auto_index else []
            return {"queued_jobs": jobs}
        if label in ("progress", "idle-progress"):
            jobs = self.progress.pop(0) if self.progress else []
            return {"queued_jobs": [{"id": i, "kind": k, "state": "pending"} for i, k in jobs]}
        if label == "cancel-job":
            return {}
        if label == "policy-status":
            return self.policy()
        if label == "sources":
            return {"items": [{"value": {"id": "src_1"}}]}
        if label == "request-epoch":
            return {"status": "request_epoch", "namespace": "n", "epoch": 0}
        if label == "catalog-revision":
            return {"page": {"revision": 3}}
        request = json.loads(Path(arguments[-1]).read_bytes()) if label in ("trim-draft", "trim-apply") else None
        if label == "trim-draft":
            answer = self.drafts.pop(0) if self.drafts else "ok"
            if answer != "ok":
                return answer
            change = request["catalog_source_mutation_draft"]["selection"]["change"]
            return {"status": "catalog_source_mutation_draft",
                    "draft": {"request": {"change": [change["kind"], change["path"]]}, "preview_digest": "pd"}}
        if label == "trim-apply":
            answer = self.applies.pop(0) if self.applies else "applied"
            atom = tuple(request["catalog_apply"]["request"]["change"])
            if answer == "applied":
                self.denies.append(atom)
                return {"status": "catalog_applied"}
            if answer == "unknown":
                self.pending.append(atom)
                return {"status": "catalog_outcome_unknown", "code": "catalog_apply_outcome_unknown",
                        "observation": {"kind": "authority_fence", "fence": {"state": "pending", "value": {
                            "stage": "fence_unknown", "operation_id": "op", "confirmations": []}}}}
            return answer
        if label.startswith("index-") and label != "index-job":
            self.indexed.append((label, sorted(self.denies), list(self.pending)))
            return {"job": {"id": "index-pass"}}
        if label == "index-job":
            self.waited.append(arguments[-1])
            state = self.job_states.get(arguments[-1], "succeeded")
            return {"job": {"id": arguments[-1], "kind": "index", "state": state, "outcome": {"state": state}}}
        raise AssertionError("unexpected call " + label)


BUSY_JOBS = {"status": "source_change_busy", "code": "source_change_jobs_pending",
             "busy": {"index_id": "idx", "pending_jobs": 1, "running_jobs": 0}}
REGISTRY_BUSY = {"status": "failure", "code": "registry_busy"}


class SourceChange(unittest.TestCase):
    """Regressions for the three failed 05-gate-lg bases (22 November 2026)."""

    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.saved_sleep = fixture.time.sleep
        fixture.time.sleep = lambda seconds: None
        self.addCleanup(setattr, fixture.time, "sleep", self.saved_sleep)

    def trim(self, service, bindings=None, **options):
        return fixture.trim_when_idle(service, bindings or {"repo": "b"}, "idx", "src_1", "exact", "a/b",
                                      sleep=lambda seconds: None, **options)

    def test_busy_refusals_are_typed_and_registry_busy_only_answers_a_review(self):
        self.assertEqual(fixture.busy_refusal(BUSY_JOBS, review=True)["code"], "source_change_jobs_pending")
        self.assertEqual(fixture.busy_refusal(BUSY_JOBS, review=False)["busy"]["pending_jobs"], 1)
        self.assertEqual(fixture.busy_refusal(REGISTRY_BUSY, review=True)["code"], "registry_busy")
        self.assertIsNone(fixture.busy_refusal(REGISTRY_BUSY, review=False))
        self.assertIsNone(fixture.busy_refusal({"status": "failure", "code": "invalid_registry_request"}, review=True))
        self.assertIsNone(fixture.busy_refusal({"status": "catalog_outcome_unknown"}, review=False))

    def test_a_change_waits_until_every_project_is_idle(self):
        """Root F: the fourth deny met the verify job that followed the third; the review must wait for it."""
        service = FakeService(self.folder.name,
                              progress=[[("v1", "verify")], [], [], [("x1", "vectors")], [], []])
        outcome = self.trim(service, {"first": "b1", "second": "b2"})
        self.assertEqual(outcome["state"], "applied")
        # Three rounds of progress samples over both projects (a verify job on the first, then a vectors job
        # on the second, then both idle) precede the one review; nothing is drafted while a job is unfinished.
        self.assertEqual(service.log[:6], ["idle-progress"] * 6)
        self.assertNotIn("trim-draft", service.log[:6])
        self.assertEqual(service.log.count("trim-draft"), 1)
        self.assertEqual(service.denies, [("exact", "a/b")])

    def test_a_typed_busy_refusal_returns_to_the_wait_and_is_recorded(self):
        service = FakeService(self.folder.name, drafts=[BUSY_JOBS, REGISTRY_BUSY, "ok"], applies=[BUSY_JOBS, "applied"])
        outcome = self.trim(service)
        self.assertEqual(outcome["state"], "applied")
        self.assertEqual([row["code"] for row in outcome["busy_refusals"]],
                         ["source_change_jobs_pending", "registry_busy", "source_change_jobs_pending"])
        self.assertEqual([row["at"] for row in outcome["busy_refusals"]], ["draft", "draft", "apply"])
        self.assertEqual(service.denies, [("exact", "a/b")])

    def test_busy_refusals_end_at_the_deadline_not_at_a_count(self):
        ticks = iter(range(0, 10_000, 100))
        service = FakeService(self.folder.name, drafts=[BUSY_JOBS] * 50)
        with self.assertRaisesRegex(ValueError, "kept refusing"):
            self.trim(service, seconds=450, clock=lambda: next(ticks))
        self.assertEqual(service.denies, [])

    def test_other_refusals_are_not_retried(self):
        service = FakeService(self.folder.name, drafts=[{"status": "failure", "code": "invalid_registry_request"}])
        with self.assertRaises(ValueError):
            self.trim(service)
        self.assertEqual(service.log.count("trim-draft"), 1)
        service = FakeService(self.folder.name, applies=[{"status": "failure", "code": "registry_busy"}])
        with self.assertRaisesRegex(ValueError, "trim apply was not accepted"):
            self.trim(service)

    def test_an_unknown_outcome_is_observed_until_in_force(self):
        """Roots C and L: catalog_outcome_unknown is not a refusal; the change completed about a second later."""
        service = FakeService(self.folder.name, applies=["unknown"], in_force_after=3)
        outcome = self.trim(service)
        self.assertEqual(outcome["state"], "unknown")
        self.assertEqual(outcome["fence"], {"observation": "authority_fence", "state": "pending",
                                            "stage": "fence_unknown", "operation_id": "op", "confirmations": 0,
                                            "worker_confirmations": 0})
        self.assertEqual(service.denies, [("exact", "a/b")])
        self.assertEqual(service.log.count("trim-apply"), 1)  # observed, never submitted again

    def test_an_unknown_outcome_that_never_comes_into_force_raises(self):
        ticks = iter(range(0, 100_000, 50))
        saved = fixture.time.monotonic
        fixture.time.monotonic = lambda: next(ticks)
        self.addCleanup(setattr, fixture.time, "monotonic", saved)
        service = FakeService(self.folder.name, applies=["unknown"], in_force_after=10_000)
        service.deadline = 10 ** 9
        with self.assertRaisesRegex(ValueError, "outcome unknown and not in force"):
            self.trim(service, seconds=300, clock=fixture.time.monotonic)
        self.assertEqual(service.denies, [])

    def test_a_deny_already_in_force_is_not_applied_again(self):
        service = FakeService(self.folder.name)
        service.denies = [("exact", "a/b")]
        self.assertEqual(self.trim(service)["state"], "already_denied")
        self.assertNotIn("trim-draft", service.log)

    def index(self, service, repo, phase, states, first_vectors=None):
        record = {"repository": repo}
        fixture.ensure_index(service, repo, Path("R"), states, states / f"{repo}.state.json", first_vectors, record,
                             "c" * 40, phase=phase)
        return record

    def test_every_deny_is_in_force_before_any_index_pass_is_queued(self):
        """The worker's order: phase "policy" for every repository, then the lexical passes."""
        states = Path(self.folder.name) / "indexes"
        states.mkdir()
        service = FakeService(self.folder.name, applies=["applied", "unknown"], in_force_after=2)
        for repo in fixture.DEFAULT_REPOSITORIES:
            record = self.index(service, repo, "policy", states)
            self.assertNotIn("indexing_seconds", record)
        self.assertEqual(service.indexed, [])
        self.assertEqual(sorted(service.denies), sorted(fixture.EXCLUSIONS["bookbuilder-v2"]))
        self.assertEqual(self.index(service, "munarium", "policy", states)["denies"], [])
        for repo in fixture.DEFAULT_REPOSITORIES:
            self.index(service, repo, "lexical", states)
        self.assertEqual([row[0] for row in service.indexed], ["index-bookbuilder-v2", "index-munarium"])
        for _, denies, pending in service.indexed:
            self.assertEqual(denies, sorted(fixture.EXCLUSIONS["bookbuilder-v2"]))
            self.assertEqual(pending, [])
        state = json.loads((states / "bookbuilder-v2.state.json").read_bytes())
        self.assertEqual([row["outcome"] for row in state["trims"]], ["applied", "unknown"])
        self.assertTrue(all(row["before_index"] for row in state["trims"]))
        self.assertNotIn("trim_refusals_before_index", state)

    def test_a_deny_that_cannot_be_applied_is_never_deferred_behind_the_index_pass(self):
        """Roots C and L indexed nothing, but the old fallback queued the pass with the deny missing."""
        states = Path(self.folder.name) / "indexes"
        states.mkdir()
        service = FakeService(self.folder.name, applies=[{"status": "failure", "code": "invalid_registry_request"}])
        with self.assertRaises(ValueError):
            self.index(service, "bookbuilder-v2", "lexical", states)
        self.assertEqual(service.indexed, [])
        self.assertFalse(hasattr(fixture, "trim_with_busy_retry"))

    def test_the_install_follows_the_denies_and_its_index_pass_is_adopted(self):
        """Root 06-gate-lg-C: the MiniLM install queued the installing project's index pass by itself."""
        states = Path(self.folder.name) / "indexes"
        states.mkdir()
        service = FakeService(self.folder.name)
        drafts = Path(self.folder.name) / "semantic-drafts"
        self.index(service, "bookbuilder-v2", "policy", states, first_vectors=drafts)
        install = service.log.index("install-minilm-bookbuilder-v2")
        applies = [n for n, label in enumerate(service.log) if label == "trim-apply"]
        self.assertEqual(len(applies), 2)
        self.assertLess(max(applies), install)
        state = json.loads((states / "bookbuilder-v2.state.json").read_bytes())
        self.assertEqual(state["install_index"]["job"], "auto-index")
        self.assertIsNone(state["indexed"])
        record = self.index(service, "bookbuilder-v2", "lexical", states)
        self.assertEqual(service.indexed, [])                 # no second pass is requested
        self.assertEqual(set(service.waited), {"auto-index"})  # the install's own pass is the first index pass
        state = json.loads((states / "bookbuilder-v2.state.json").read_bytes())
        self.assertEqual((state["indexed"]["job"], state["indexed"]["queued_by"]), ("auto-index", "the MiniLM install"))
        self.assertIsNone(state["indexed"]["job_seconds"])  # no registry in this test
        self.assertGreaterEqual(record["indexing_seconds"], 0)
        # A later repository is set up after the install and its pass is queued by the launcher.
        self.index(service, "munarium", "policy", states)
        self.index(service, "munarium", "lexical", states)
        self.assertEqual([row[0] for row in service.indexed], ["index-munarium"])
        state = json.loads((states / "munarium.state.json").read_bytes())
        self.assertEqual(state["indexed"]["queued_by"], "the launcher")

    def test_an_install_without_an_index_pass_leaves_the_pass_to_the_launcher(self):
        service = FakeService(self.folder.name)
        found = fixture.install_index_job(service, "b", seconds=0)
        self.assertIsNone(found["job"])
        service.auto_index = True
        self.assertEqual(fixture.install_index_job(service, "b", seconds=0)["job"], "auto-index")

    def test_a_cancelled_install_pass_is_not_adopted_by_the_next_lifetime(self):
        """A lifetime stopped during the install's pass cancels it; the restart must queue a new pass."""
        states = Path(self.folder.name) / "indexes"
        states.mkdir()
        service = FakeService(self.folder.name)
        self.index(service, "bookbuilder-v2", "policy", states, first_vectors=Path(self.folder.name) / "drafts")
        service.job_states["auto-index"] = "canceled"
        self.index(service, "bookbuilder-v2", "lexical", states)
        self.assertEqual([row[0] for row in service.indexed], ["index-bookbuilder-v2"])
        state = json.loads((states / "bookbuilder-v2.state.json").read_bytes())
        self.assertEqual((state["indexed"]["job"], state["indexed"]["queued_by"]), ("index-pass", "the launcher"))

    def test_only_the_first_repository_of_a_fresh_installation_may_need_a_deny(self):
        """Root 06-gate-lg-C listed munarium first: the bookbuilder-v2 deny then met munarium's vectors job."""
        states = Path(self.folder.name) / "indexes"
        states.mkdir()
        self.assertIsNone(fixture.repository_order_problem(fixture.DEFAULT_REPOSITORIES, states))
        self.assertEqual(fixture.DEFAULT_REPOSITORIES[0], "bookbuilder-v2")
        problem = fixture.repository_order_problem(("munarium", "bookbuilder-v2"), states)
        self.assertIn("bookbuilder-v2 needs denies and is not listed first", problem)
        self.assertIn("airk", fixture.repository_order_problem(("bookbuilder-v2", "airk"), states))
        self.assertIsNone(fixture.repository_order_problem(("munarium",), states))
        # A repository already set up has its denies; an installation with MiniLM takes a new one when quiet.
        (states / "bookbuilder-v2.state.json").write_text("{}", encoding="utf-8")
        self.assertIsNone(fixture.repository_order_problem(("munarium", "bookbuilder-v2"), states))
        (states / "bookbuilder-v2.state.json").unlink()
        (states / "minilm.state.json").write_text("{}", encoding="utf-8")
        self.assertIsNone(fixture.repository_order_problem(("munarium", "bookbuilder-v2"), states))

    def registry(self, jobs=(), follow_ups=(), setup=(), audits=()):
        import sqlite3
        control = Path(tempfile.mkdtemp(dir=self.folder.name))
        connection = sqlite3.connect(control / "service-1.4.sqlite3")
        connection.executescript(
            "CREATE TABLE service_jobs(seq INTEGER PRIMARY KEY, id TEXT, project TEXT, kind TEXT, state TEXT);"
            "CREATE TABLE service_background_audit(project TEXT, job TEXT, manual INTEGER);"
            "CREATE TABLE service_background(project TEXT, revision INTEGER, source_pending INTEGER, "
            "semantic_pending INTEGER);"
            "CREATE TABLE service_setup_pending(project TEXT);")
        connection.executemany("INSERT INTO service_jobs(id, project, kind, state) VALUES(?,?,?,?)", jobs)
        connection.executemany("INSERT INTO service_background VALUES(?,0,?,?)", follow_ups)
        connection.executemany("INSERT INTO service_setup_pending VALUES(?)", [(p,) for p in setup])
        connection.executemany("INSERT INTO service_background_audit VALUES('p',?,?)", audits)
        connection.commit()
        connection.close()
        return control

    def test_the_registry_snapshot_names_unfinished_and_promised_work(self):
        quiet = self.registry(jobs=[("j1", "p1", "index", "succeeded"), ("j2", "p1", "verify", "pending")],
                              follow_ups=[("p1", 0, 0)], audits=[("j2", 0)])
        self.assertEqual(fixture.registry_pending(quiet), {"jobs": [], "follow_ups": [], "setup_pending": []})
        busy = self.registry(jobs=[("j1", "p1", "vectors", "running"), ("j2", "p1", "verify", "pending"),
                                   ("j3", "p2", "verify", "pending"), ("j4", "p2", "verify", "running")],
                             follow_ups=[("p1", 0, 1), ("p2", 0, 0), ("p3", 1, 0)], setup=["p4"],
                             audits=[("j2", 1), ("j3", 0), ("j4", 0)])
        pending = fixture.registry_pending(busy)
        # A parked automatic audit (j3) is left out; a manual one (j2) and a running one (j4) are not.
        self.assertEqual([row["job"] for row in pending["jobs"]], ["j1", "j2", "j4"])
        self.assertEqual(pending["follow_ups"], [{"project": "p1", "index_pass": False, "vectors": True},
                                                 {"project": "p3", "index_pass": True, "vectors": False}])
        self.assertEqual(pending["setup_pending"], ["p4"])

    def test_promised_follow_up_work_keeps_a_change_waiting(self):
        """Root 06-gate-lg-C: one progress sample fell into the 48 ms between an index pass and its vectors job."""
        service = FakeService(self.folder.name)
        service.control = self.registry(follow_ups=[("p1", 0, 1)])
        ticks = iter(range(0, 10_000, 100))
        with self.assertRaisesRegex(ValueError, "did not become quiet"):
            self.trim(service, seconds=450, clock=lambda: next(ticks))
        self.assertNotIn("trim-draft", service.log)
        answers = [{"jobs": [], "follow_ups": [{"project": "p1", "index_pass": False, "vectors": True}],
                    "setup_pending": []}, {"unreadable": "locked"},
                   {"jobs": [], "follow_ups": [], "setup_pending": []}]
        saved = fixture.registry_pending
        fixture.registry_pending = lambda control: answers.pop(0) if len(answers) > 1 else answers[0]
        self.addCleanup(setattr, fixture, "registry_pending", saved)
        service = FakeService(self.folder.name, progress=[[("x", "index")], []])
        service.control = self.registry()
        self.assertEqual(self.trim(service)["state"], "applied")
        # One busy progress sample (the registry is not opened), then two registry answers that are not
        # quiet, then the quiet one: four rounds before the single review.
        self.assertEqual(service.log[:4], ["idle-progress"] * 4)
        self.assertEqual(len(answers), 1)

    def test_bindings_come_from_every_state_file(self):
        states = Path(self.folder.name) / "indexes"
        states.mkdir()
        (states / "munarium.state.json").write_text(json.dumps({"repository": "munarium", "binding": "b1"}))
        (states / "bookbuilder-v2.state.json").write_text(json.dumps({"binding": "b2"}))
        (states / "minilm.state.json").write_text(json.dumps({"installed_by": "munarium"}))
        self.assertEqual(fixture.installation_bindings(states), {"munarium": "b1", "bookbuilder-v2": "b2"})
        self.assertEqual(fixture.installation_bindings(states / "missing"), {})


class StatusRefusal(unittest.TestCase):
    """Regression for base lifetime 06-gate-lg-L-base (23 November 2026): after 3.6 hours of embedding, one
    ``airk_status`` poll was refused with ``scoped_connection_unavailable`` and the wait ended the lifetime."""

    REFUSED = {"isError": True, "structuredContent": {"error": {
        "code": "scoped_connection_unavailable", "query_submitted": False, "retryable": False}}}

    def commands(self, folder, script, progress=()):
        """A scripted ``Commands``: each status poll takes the next entry, a body or ``REFUSED`` (written to
        the call's stdout file before the call fails, as the real one does); each ``progress`` read takes the
        next entry of ``progress``, and an index pass returns a job."""
        test = self
        progress = list(progress)

        class Scripted:
            deadline, prefix, count, stop_file = float("inf"), "p", 0, None

            def __init__(self):
                self.folder = Path(folder)
                self.calls = []

            def json(self, label, *arguments):
                self.count += 1
                self.calls.append(label)
                return progress.pop(0) if label == "semantic-progress" else {"job": {"id": "retry-job"}}

            def tool(self, label, tool, index_id, arguments=None):
                self.count += 1
                answer = script.pop(0)
                if answer is test.REFUSED or isinstance(answer, str):
                    text = answer if isinstance(answer, str) else json.dumps(answer)
                    (self.folder / f"{self.prefix}-{self.count:04}-{label}.stdout").write_text(text, encoding="utf-8")
                    raise ValueError(f"{label} failed (exit 1)")
                return {"structuredContent": answer}, None, 0.0

        return Scripted()

    def test_a_refused_status_poll_is_no_observation_and_the_wait_goes_on(self):
        ready = {"semantic_readiness": {"state": "ready"}, "publication": {"token": "t"}}
        partial = {"semantic_readiness": {"state": "partial"}}
        original = fixture.time.sleep
        fixture.time.sleep = lambda seconds: None
        try:
            with tempfile.TemporaryDirectory() as folder:
                refusals = []
                commands = self.commands(folder, [partial, self.REFUSED, self.REFUSED, partial, ready])
                status = fixture.wait_semantic(commands, "idx", 60, refusals=refusals)
                self.assertEqual(status, ready)
                self.assertEqual([(r["code"], r["command"]) for r in refusals],
                                 [("scoped_connection_unavailable", "p-0002-semantic-status"),
                                  ("scoped_connection_unavailable", "p-0003-semantic-status")])
                # More in a row than the bound, a failure that is not such a refusal, a refusal after a
                # submitted query and a state other than pending or partial still end the wait.
                many = self.commands(folder, [self.REFUSED] * (fixture.STATUS_REFUSAL_POLLS + 1))
                with self.assertRaises(ValueError):
                    fixture.wait_semantic(many, "idx", 60, refusals=[])
                self.assertEqual(many.count, fixture.STATUS_REFUSAL_POLLS + 1)
                submitted = json.dumps({"structuredContent": {"error": {"code": "x", "query_submitted": True}}})
                for text in ("not json", submitted, json.dumps({"structuredContent": {"status": "failed"}})):
                    with self.assertRaises(ValueError):
                        fixture.wait_semantic(self.commands(folder, [text, ready]), "idx", 60, refusals=[])
                failed = self.commands(folder, [{"semantic_readiness": {"state": "failed"}}])
                with self.assertRaisesRegex(ValueError, "semantic status became failed"):
                    fixture.wait_semantic(failed, "idx", 60)
                # A vectors job that failed with the service's temporary code is retried once, explicitly,
                # and recorded; while a job is at work the failed state is only observed again.
                failed = {"semantic_readiness": {"state": "failed"}}
                helper = {"observation": {"job": {"id": "v1", "kind": "vectors", "state": "failed",
                                                  "outcome": {"state": "failed", "code": "helper_unavailable"}}}}
                at_work = {"observation": {"job": {"id": "retry-job", "kind": "index", "state": "running"}}}
                retries = []
                # One sample decides nothing (the observation lags a job just queued): the retry is queued
                # after three idle observations in a row, and a job seen at work resets the count.
                stale = {"observation": {"job": {"id": "m1", "kind": "maintain", "state": "succeeded"}}}
                retried = self.commands(folder, [failed] * 6 + [partial, ready],
                                        [helper, at_work, helper, helper, helper, at_work])
                self.assertEqual(fixture.wait_semantic(retried, "idx", 60, "binding", None, None, retries), ready)
                self.assertEqual([(r["code"], r["failed_job"], r["retry_job"]) for r in retries],
                                 [("helper_unavailable", "v1", "retry-job")])
                self.assertEqual(retried.calls.count("semantic-retry-index"), 1)
                # A second failure, another code, and a wait without a retry record stay fatal.
                again = self.commands(folder, [failed] * 6, [helper] * 6)
                with self.assertRaisesRegex(ValueError, "after 1 retries"):
                    fixture.wait_semantic(again, "idx", 60, "binding", None, None, [])
                # A restart: the resume pass was just queued and the observation still shows an older job.
                resumed = self.commands(folder, [failed, failed, partial, ready], [stale, at_work])
                self.assertEqual(fixture.wait_semantic(resumed, "idx", 60, "binding", None, None, []), ready)
                self.assertNotIn("semantic-retry-index", resumed.calls)
                with self.assertRaisesRegex(ValueError, r"became failed \(None, after 0 retries\)"):
                    fixture.wait_semantic(self.commands(folder, [failed] * 3, [stale] * 3), "idx", 60, "binding",
                                          None, None, [])
                other = {"observation": {"job": {"id": "v1", "kind": "vectors", "state": "failed",
                                                 "outcome": {"state": "failed", "code": "project_operation_failed"}}}}
                with self.assertRaisesRegex(ValueError, "project_operation_failed"):
                    fixture.wait_semantic(self.commands(folder, [failed] * 3, [other] * 3), "idx", 60, "binding",
                                          None, None, [])
                with self.assertRaisesRegex(ValueError, "semantic status became failed"):
                    fixture.wait_semantic(self.commands(folder, [failed]), "idx", 60, "binding")
                # The bound counts consecutive refusals: an observation in between starts it again.
                spaced = [self.REFUSED] * fixture.STATUS_REFUSAL_POLLS + [partial] + \
                         [self.REFUSED] * fixture.STATUS_REFUSAL_POLLS + [ready]
                self.assertEqual(fixture.wait_semantic(self.commands(folder, spaced), "idx", 60), ready)
        finally:
            fixture.time.sleep = original


class SmokeAnswer(unittest.TestCase):
    """The smoke Answer is the fixture's own /3 template; no schema or budget is written in the launcher."""

    CASES = json.loads((HERE.parents[1] / "fixtures" / "split" / "arbiter" / "answer_loop_cases.json").read_text(
        encoding="utf-8"))

    def test_the_request_is_the_written_template_with_the_question_set(self):
        template = {"schema": self.CASES["request_schema"], "question": "__PRIVATE_FROZEN_QUESTION__",
                    "budget": self.CASES["default_budget"], "response_mode": "full", "include_status": True}
        with tempfile.TemporaryDirectory() as folder:
            work = Path(folder)
            (work / "context-request.json").write_text(json.dumps(template), encoding="utf-8")
            self.assertEqual(fixture.answer_arguments(work, "Where is it?"), dict(template, question="Where is it?"))
            self.assertEqual(fixture.ANSWER_REQUEST_SCHEMA, self.CASES["request_schema"])
            # A template of the one-turn request, or one without a budget, is refused and never patched.
            for bad in (dict(template, schema="airk.arbiter.context.request/2"), dict(template, budget={}),
                        {k: v for k, v in template.items() if k != "budget"}):
                (work / "context-request.json").write_text(json.dumps(bad), encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "not a whole Answer /3 request"):
                    fixture.answer_arguments(work, "q")

    def test_the_reply_is_read_as_answer_3_or_the_smoke_fails(self):
        body = next(c["response"] for c in self.CASES["response_cases"] if c["id"] == "two_parts_valid")
        self.assertEqual(fixture.ANSWER_REPLY_SCHEMA, self.CASES["response_schema"])
        row = fixture.answer_smoke_row(body)
        self.assertEqual((row["schema"], row["parts"], len(row["stops"]), row["records"], row["decisions"]),
                         (self.CASES["response_schema"], len(body["parts"]), len(body["parts"]),
                          len(body["records"]), body["decision_log"]["total"]))
        self.assertEqual(row["degradations"], ["context_abstained"])
        for bad in ({"schema": "airk.arbiter.context/2", "stop": "sufficient", "records": []}, {}, None):
            with self.assertRaisesRegex(ValueError, "not an Answer /3 reply"):
                fixture.answer_smoke_row(bad)


class SearchSwitch(unittest.TestCase):
    def test_search_switch_passes_the_airk_strip(self):
        """Regression (22 Nov 2026): AIRK_ARBITER_EVALUATION_SEARCH must reach the service."""
        self.assertEqual(fixture.search_extras(None), {})
        self.assertEqual(fixture.search_extras("search_v2"), {"AIRK_ARBITER_EVALUATION_SEARCH": "search_v2"})
        self.assertEqual(fixture.search_extras("none"), {"AIRK_ARBITER_EVALUATION_SEARCH": "none"})
        with self.assertRaises(ValueError):
            fixture.search_extras("search_v3")
        with tempfile.TemporaryDirectory() as tmp:
            inputs = {"path": "i.json", "sha256": "0" * 64}
            _, overrides = fixture.service_environment(Path(tmp), 60, inputs, Path(tmp),
                                                       fixture.search_extras("search_v2"))
        self.assertEqual(overrides["AIRK_ARBITER_EVALUATION_SEARCH"], "search_v2")

    def test_search_switch_round_trips_to_the_worker(self):
        parser = argparse.ArgumentParser()
        fixture.add_selector_arguments(parser)
        args = parser.parse_args(["--search", "search_v2"])
        args.restart_from = None
        self.assertEqual(fixture.selector_arguments(args), ["--search", "search_v2"])
        self.assertIsNone(parser.parse_args([]).search)


class ProbeRetry(unittest.TestCase):
    def test_probe_is_reissued_after_a_concurrent_stage_change_only(self):
        """Regression (L23, 22 Nov 2026): a probe refused because Focus changed mid-read is issued again."""
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)

            class Commands:
                prefix = "p"

                def __init__(self, outcomes):
                    self.folder, self.outcomes, self.calls = folder, list(outcomes), 0

                def tool(self, label, tool, index_id, args):
                    self.calls += 1
                    kind, text = self.outcomes.pop(0)
                    (folder / f"p-{self.calls:04d}-{label}.stdout").write_text(text, encoding="utf-8")
                    if kind == "fail":
                        raise ValueError("probe failed")
                    return ({"structuredContent": {"ok": True}}, None, 0)
            refused = '{"error":{"message":"Focus policy or source changed after native work; ..."}}'
            commands = Commands([("fail", refused), ("ok", "{}")])
            self.assertEqual(fixture.probe_search(commands, "airk", "i", sleep=lambda s: None), {"ok": True})
            self.assertEqual(commands.calls, 2)
            other = Commands([("fail", '{"error":{"message":"read_failed"}}')])
            with self.assertRaises(ValueError):
                fixture.probe_search(other, "airk", "i", sleep=lambda s: None)


class TuningTokens(unittest.TestCase):
    def test_layer_values_pass_the_check(self):
        """Regression (22 Nov 2026): cs1e tuning tokens such as pool=8 must reach the selector."""
        layers = "all,pool=8,pool_full=8,read_floor=3072,kept=3,subjects=strict,governing=project"
        extras = fixture.selector_extras("change-set-1", True, Path("i.json"), "0" * 64, layers, None, check_file=False)
        self.assertEqual(extras["AIRK_ARBITER_EVALUATION_ANSWER_LAYERS"], layers)
        for bad in ("all,pool==8", "all,a=b=c", "all,,pool=8", "all,pool=8;x"):
            with self.assertRaises(ValueError, msg=bad):
                fixture.selector_extras("change-set-1", True, Path("i.json"), "0" * 64, bad, None, check_file=False)


class ChangeSetInputsPath(unittest.TestCase):
    def test_relative_inputs_become_absolute(self):
        """Regression (L15-cs1, 22 Nov 2026): the selector refuses a relative inputs path."""
        extras = fixture.selector_extras("change-set-1", True, Path("relative/inputs.json"), "0" * 64,
                                         "all", None, check_file=False)
        value = extras["AIRK_ARBITER_EVALUATION_CHANGE_SET_INPUTS"]
        self.assertTrue(Path(value).is_absolute(), value)
        self.assertTrue(value.endswith(str(Path("relative/inputs.json"))))


class Selectors(unittest.TestCase):
    SHA = "ab" * 32

    def extras(self, **options):
        return fixture.selector_extras(check_file=False, **options)

    def test_software_v2_takes_no_change_set_arguments(self):
        self.assertEqual(self.extras(selector="software-v2", restart=False), {})
        with self.assertRaises(ValueError):
            self.extras(selector="software-v2", restart=False, layers="all")

    def test_change_set_fresh_needs_a_seed_and_restart_refuses_one(self):
        fresh = self.extras(selector="change-set-1", restart=False, inputs=Path("I.json"), inputs_sha256=self.SHA,
                            layers="all,-focus", seed="focus")
        self.assertEqual(fresh, {"AIRK_ARBITER_EVALUATION_CHANGE_SET_INPUTS": str(Path("I.json").resolve()),
                                 "AIRK_ARBITER_EVALUATION_CHANGE_SET_INPUTS_SHA256": self.SHA,
                                 "AIRK_ARBITER_EVALUATION_ANSWER_LAYERS": "all,-focus",
                                 "AIRK_ARBITER_EVALUATION_CHANGE_SET_SEED_CPU": "focus"})
        restart = self.extras(selector="change-set-1", restart=True, inputs=Path("I.json"), inputs_sha256=self.SHA)
        self.assertNotIn("AIRK_ARBITER_EVALUATION_CHANGE_SET_SEED_CPU", restart)
        self.assertNotIn("AIRK_ARBITER_EVALUATION_ANSWER_LAYERS", restart)
        for bad in (dict(restart=False, seed=None), dict(restart=True, seed="all"), dict(restart=False, seed="cuda"),
                    dict(restart=False, seed="none", layers=""), dict(restart=False, seed="none", layers="a,,b"),
                    dict(restart=False, seed="none", inputs_sha256="XY")):
            options = dict(selector="change-set-1", inputs=Path("I.json"), inputs_sha256=self.SHA) | bad
            with self.assertRaises(ValueError, msg=str(bad)):
                self.extras(**options)
        with self.assertRaises(ValueError):
            self.extras(selector="change-set-1", restart=True, inputs=None, inputs_sha256=None)

    def test_change_set_inputs_are_pinned(self):
        import hashlib
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "inputs.json"
            path.write_text("[]\n", encoding="utf-8")
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            self.assertIn("AIRK_ARBITER_EVALUATION_CHANGE_SET_INPUTS",
                          fixture.selector_extras("change-set-1", True, path, digest))
            with self.assertRaises(ValueError):
                fixture.selector_extras("change-set-1", True, path, "0" * 64)

    def test_extras_survive_the_airk_strip_and_worker_argv_round_trips(self):
        import argparse
        import os
        saved = dict(os.environ)
        try:
            os.environ["AIRK_ARBITER_EVALUATION_ANSWER_LAYERS"] = "stale"
            extras = self.extras(selector="change-set-1", restart=False, inputs=Path("I.json"),
                                 inputs_sha256=self.SHA, layers="none", seed="none")
            env, overrides = fixture.service_environment(Path("W"), 60, {"path": "P", "sha256": self.SHA},
                                                         Path("T"), extras)
            self.assertEqual(env["AIRK_ARBITER_EVALUATION_ANSWER_LAYERS"], "none")
            self.assertEqual(overrides["AIRK_ARBITER_EVALUATION_CHANGE_SET_SEED_CPU"], "none")
        finally:
            os.environ.clear()
            os.environ.update(saved)
        parser = argparse.ArgumentParser()
        parser.add_argument("--restart-from")
        fixture.add_selector_arguments(parser)
        outer = parser.parse_args(["--selector", "change-set-1", "--change-set-inputs", "I.json",
                                   "--change-set-inputs-sha256", self.SHA, "--answer-layers", "all", "--seed-cpu",
                                   "all"])
        saved_pin = fixture.pin
        fixture.pin = lambda value: Path(value["path"])
        try:
            argv = ["--selector", outer.selector] + fixture.selector_arguments(outer)
        finally:
            fixture.pin = saved_pin
        inner = parser.parse_args(argv)
        self.assertEqual((inner.selector, str(inner.change_set_inputs), inner.change_set_inputs_sha256,
                          inner.answer_layers, inner.seed_cpu), ("change-set-1", "I.json", self.SHA, "all", "all"))
        self.assertIn("restart::change_set::actual_private_arbiter_fixture_change_set_1",
                      fixture.SELECTORS["change-set-1"])


class Progress(unittest.TestCase):
    def test_projection_uses_the_recent_artifact_rate(self):
        samples = [{"unix": 0, "completed": 100, "selected": 1100}, {"unix": 1800, "completed": 400, "selected": 1100}]
        projected = fixture.project(samples, 1800)
        self.assertEqual(projected["artifacts_per_hour"], 600.0)
        self.assertEqual(projected["remaining_artifacts"], 700)
        self.assertAlmostEqual(projected["projected_hours"], 700 / 600, places=2)
        self.assertIsNone(fixture.project(samples[:1], 1800))

    def test_jobs_per_hour_counts_successes_in_the_window(self):
        events = [("b", "j1", "vectors", "job_succeeded", 1000), ("b", "j2", "vectors", "job_running", 2000),
                  ("b", "j3", "vectors", "job_succeeded", 3_000_000), ("c", "j4", "vectors", "job_succeeded", 3_100_000)]
        self.assertEqual(fixture.jobs_per_hour(events, "b", "vectors", 3_600_000, 3_600_000), 2.0)
        self.assertEqual(fixture.jobs_per_hour(events, "b", "vectors", 1_200_000, 3_600_000), 3.0)


class Settlement(unittest.TestCase):
    def test_typed_retry_reads_only_retryable_answers(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "out.stdout"
            body = {"status": "retry", "retryable": True, "retry_after_ms": 40, "code": "publication_busy"}
            path.write_text(json.dumps({"content": [{"type": "text", "text": json.dumps(body)}]}), encoding="utf-8")
            self.assertEqual(fixture.typed_retry(path), 40)
            body["retryable"] = False
            path.write_text(json.dumps({"content": [{"type": "text", "text": json.dumps(body)}]}), encoding="utf-8")
            self.assertIsNone(fixture.typed_retry(path))
            self.assertIsNone(fixture.typed_retry(Path(folder) / "missing.stdout"))
            unsubmitted = {"error": {"code": "scoped_status_unavailable", "retryable": True,
                                     "fresh_brokerage_required": True, "query_submitted": None}}
            path.write_text(json.dumps({"isError": True, "structuredContent": unsubmitted}), encoding="utf-8")
            self.assertEqual(fixture.typed_retry(path), 500)
            unsubmitted["error"]["query_submitted"] = "q1"
            path.write_text(json.dumps({"isError": True, "structuredContent": unsubmitted}), encoding="utf-8")
            self.assertIsNone(fixture.typed_retry(path))

    def test_settle_waits_for_follow_up_work_and_cancels_only_when_asked(self):
        class Fake:
            def __init__(self, script):
                self.script, self.canceled = list(script), []

            def json(self, label, *arguments):
                if label == "cancel-job":
                    self.canceled.append(arguments[-1])
                    return {}
                jobs = self.script.pop(0) if self.script else []
                return {"observation": None, "pending_job": None,
                        "queued_jobs": [{"id": i, "kind": k, "state": "pending"} for i, k in jobs]}
        with tempfile.TemporaryDirectory() as folder:
            states = Path(folder)
            (states / "airk.state.json").write_text(json.dumps({"binding": "b", "setup_jobs_canceled": []}))
            (states / "minilm.state.json").write_text(json.dumps({"installed_by": "airk"}))
            saved = fixture.time.sleep
            fixture.time.sleep = lambda seconds: None
            try:
                # Empty, then a follow-up verify job appears: idleness must be observed again.
                fake = Fake([[], [], [("j1", "verify")], [], [], [], [], []])
                canceled, _ = fixture.settle_projects(fake, states, cancel=False)
                self.assertEqual(canceled, [])
                self.assertEqual(fake.script, [])
                fake = Fake([[("j2", "vectors")], [("j2", "vectors"), ("j3", "configure")]])
                canceled, _ = fixture.settle_projects(fake, states, cancel=True)
                self.assertEqual(fake.canceled, ["j2"])
                self.assertEqual([row["kind"] for row in canceled], ["vectors"])
            finally:
                fixture.time.sleep = saved


class Snapshot(unittest.TestCase):
    def test_snapshot_exports_only_the_tracked_tree_of_the_commit(self):
        import subprocess
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            checkout, fixtures = root / "checkout", root / "fixtures"
            checkout.mkdir()
            fixtures.mkdir()

            def git(*arguments):
                return subprocess.run(["git", "-C", str(checkout), "-c", "user.name=t", "-c", "user.email=t@example.invalid",
                                       *arguments], capture_output=True, text=True, check=True).stdout.strip()
            git("init", "-q")
            (checkout / "a.txt").write_text("one\n", encoding="utf-8")
            (checkout / ".gitignore").write_text("ignored.txt\n", encoding="utf-8")
            git("add", "a.txt", ".gitignore")
            git("commit", "-q", "-m", "c1")
            commit = git("rev-parse", "HEAD")
            (checkout / "a.txt").write_text("changed\n", encoding="utf-8")
            (checkout / "untracked.txt").write_text("x\n", encoding="utf-8")
            saved = dict(fixture.REPOSITORIES), dict(fixture.SNAPSHOTS)
            try:
                fixture.REPOSITORIES["synthetic"] = checkout
                fixture.SNAPSHOTS["synthetic"] = commit
                destination, record = fixture.ensure_snapshot("synthetic", fixtures)
                self.assertEqual((destination / "a.txt").read_text(encoding="utf-8").strip(), "one")
                self.assertFalse((destination / "untracked.txt").exists())
                self.assertEqual(record["files"], 2)
                self.assertFalse(Path(record["archive"]["path"]).exists())
                again, _ = fixture.ensure_snapshot("synthetic", fixtures)
                self.assertEqual(again, destination)
            finally:
                fixture.REPOSITORIES.clear(); fixture.REPOSITORIES.update(saved[0])
                fixture.SNAPSHOTS.clear(); fixture.SNAPSHOTS.update(saved[1])


class Scenarios(unittest.TestCase):
    def test_probes_come_from_airk_scenarios_and_sealed_only(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for set_name, repo, slot, phrase in (("scenarios", "airk", "C1", "alpha beta gamma"),
                                                 ("sealed", "airk", "A1", "delta epsilon zeta"),
                                                 ("scenarios", "bookbuilder", "C1", "not this one")):
                path = root / set_name / repo / f"{slot}.json"
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(json.dumps({"repository": repo, "leak_probe": phrase}), encoding="utf-8")
            probes = fixture.scenario_probes(root)
            self.assertEqual([row["phrase"] for row in probes], ["alpha beta gamma", "delta epsilon zeta"])

    def test_the_scan_reads_only_scenario_objects_in_the_repository_folder(self):
        """Regression (22 Nov 2026): a review's findings.json (a list) lay in scenarios/munarium-review."""
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for relative, value in (("scenarios/munarium/C1.json", {"repository": "munarium", "leak_probe": "one"}),
                                    ("sealed/munarium/A1.json", {"repository": "munarium", "leak_probe": "two"}),
                                    ("scenarios/munarium/list.json", ["not a scenario"]),
                                    ("scenarios/munarium/other.json", {"repository": "airk", "leak_probe": "x"}),
                                    ("scenarios/munarium-review/findings.json", [{"id": 1}]),
                                    ("scenarios/airk/C1.json", {"repository": "airk", "leak_probe": "three"})):
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(json.dumps(value), encoding="utf-8")
            self.assertEqual([(row["source"], row["phrase"]) for row in fixture.scenario_probes(root, "munarium")],
                             [("scenarios/munarium/C1.json", "one"), ("sealed/munarium/A1.json", "two")])
            self.assertEqual([row["phrase"] for row in fixture.scenario_probes(root)], ["three"])
            self.assertEqual(fixture.scenario_probes(root, "bookbuilder"), [])

    def test_probes_follow_the_freeze_and_refuse_a_changed_file(self):
        import hashlib
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            files = {}
            for relative, repo, phrase in (("scenarios/airk/C1.json", "airk", "one two three"),
                                           ("scenarios/bookbuilder/C1.json", "bookbuilder", "skip me"),
                                           ("scenarios/airk/C2.json", "airk", "unfrozen")):
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(json.dumps({"repository": repo, "leak_probe": phrase}), encoding="utf-8")
                if phrase != "unfrozen":
                    files[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
            (root / "scenario-freeze.json").write_text(json.dumps({"files": files}), encoding="utf-8")
            self.assertEqual([row["phrase"] for row in fixture.scenario_probes(root)], ["one two three"])
            (root / "scenarios/airk/C1.json").write_text("{}", encoding="utf-8")
            with self.assertRaises(ValueError):
                fixture.scenario_probes(root)

    def test_restart_request_pins_origin_ready_and_previous_status(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            origin = root / "L1" / "fixture"
            origin.mkdir(parents=True)
            (origin / "ready.json").write_text("{}", encoding="utf-8")
            (origin / "closed.json").write_text("{}", encoding="utf-8")
            previous = root / "L2"
            (previous / "commands").mkdir(parents=True)
            status = previous / "commands" / "final-status.stdout"
            status.write_text("{}", encoding="utf-8")
            (previous / "handle.json").write_text(json.dumps({"state": "closed", "control_owner": "L1",
                                                              "final_status": {"path": str(status)}}))
            target = root / "L3"
            target.mkdir()
            _, pinned = fixture.restart_request(root, "L2", target)
            request = json.loads(Path(pinned["path"]).read_text(encoding="utf-8"))
            self.assertEqual(Path(request["ready"]["path"]), origin / "ready.json")
            self.assertEqual(Path(request["closed"]["path"]), origin / "closed.json")
            self.assertEqual(Path(request["status"]["path"]), status)

    def test_restart_refuses_an_unclosed_lifetime(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "L2").mkdir()
            (root / "L2" / "handle.json").write_text(json.dumps({"state": "ready"}))
            with self.assertRaises(ValueError):
                fixture.restart_request(root, "L2", root)


    def unidle_lifetime(self, root, name="L38-cs2a2", running=1, context_running=0):
        work = root / name / "fixture"
        (root / name / "commands").mkdir(parents=True)
        work.mkdir(parents=True)
        (work / "closed.json").write_text(json.dumps({"instance": "i1", "shutdown_completed": True,
                                                      "service_lease_reacquired": True}), encoding="utf-8")
        status = root / name / "commands" / "final-status.stdout"
        status.write_text(json.dumps({
            "focus": {"queued": 0, "running": running, "active_profile": "p",
                      "profiles": [{"profile": "p", "tasks": ["airk.focus_rank.v1"]}]},
            "context": {"queued": 0, "running": context_running}}), encoding="utf-8")
        origin = root / "L8" / "fixture"
        origin.mkdir(parents=True)
        for item in ("ready.json", "closed.json"):
            (origin / item).write_text("{}", encoding="utf-8")
        handle = {"lifetime": name, "state": "ready", "instance": "i1", "work": str(work), "control_owner": "L8",
                  "final_stage_idle": False, "service_exit_code": 0, "restartable": True,
                  "restart_blockers": {"setup_pending": [], "unfinished_jobs": []}, "canceled_jobs": [],
                  "final_status": {"path": str(status), "sha256": fixture.sha256(status)}}
        (root / name / "handle.json").write_text(json.dumps(handle), encoding="utf-8")
        return handle, fixture.sha256(work / "closed.json")

    def test_unidle_close_exception_accepts_only_the_recorded_lifetime_and_digest(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            handle, digest = self.unidle_lifetime(root)
            self.assertFalse(fixture.prior_settled(handle))  # the default gate is unchanged
            with self.assertRaises(ValueError):  # no exception named: refused as before
                fixture.restart_request(root, "L38-cs2a2", root / "L38-cs2a2")
            with self.assertRaisesRegex(ValueError, "another closed.json digest"):  # the real table's digest
                fixture.unidle_close_exception(handle, "L38-cs2a2", digest)
            allowed = {"L38-cs2a2": digest}
            record = fixture.unidle_close_exception(handle, "L38-cs2a2", digest, allowed)
            self.assertEqual((record["closed_sha256"], record["focus"]["running"]), (digest, 1))
            with self.assertRaisesRegex(ValueError, "no recorded close exception"):
                fixture.unidle_close_exception(dict(handle, lifetime="L37-cs2b"), "L37-cs2b", digest, allowed)
            with self.assertRaisesRegex(ValueError, "not the named lifetime"):
                fixture.unidle_close_exception(dict(handle, lifetime="L37-cs2b"), "L38-cs2a2", digest, allowed)
            closed = Path(handle["work"]) / "closed.json"
            closed.write_text(closed.read_text(encoding="utf-8") + " ", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "closed.json differs"):
                fixture.unidle_close_exception(handle, "L38-cs2a2", digest, allowed)

    def test_unidle_close_exception_checks_every_receipt(self):
        cases = [({"service_exit_code": 1}, "did not exit 0"),
                 ({"canceled_jobs": ["j"]}, "cancelled jobs"),
                 ({"restart_blockers": {"setup_pending": [], "unfinished_jobs": ["x"]}}, "restart blockers"),
                 ({"final_stage_idle": True}, "non-idle final stage")]
        for change, message in cases:
            with self.subTest(message), tempfile.TemporaryDirectory() as folder:
                handle, digest = self.unidle_lifetime(Path(folder))
                with self.assertRaisesRegex(ValueError, message):
                    fixture.unidle_close_exception(dict(handle, **change), "L38-cs2a2", digest, {"L38-cs2a2": digest})
        for kwargs, message in (({"running": 2}, "exactly one running"), ({"context_running": 1}, "Context")):
            with self.subTest(message), tempfile.TemporaryDirectory() as folder:
                handle, digest = self.unidle_lifetime(Path(folder), **kwargs)
                with self.assertRaisesRegex(ValueError, message):
                    fixture.unidle_close_exception(handle, "L38-cs2a2", digest, {"L38-cs2a2": digest})

    def test_restart_request_writes_the_checked_exception(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            handle, digest = self.unidle_lifetime(root)
            target = root / "L39"
            target.mkdir()
            original = dict(fixture.UNIDLE_CLOSE_EXCEPTIONS)
            fixture.UNIDLE_CLOSE_EXCEPTIONS["L38-cs2a2"] = digest
            try:
                with self.assertRaisesRegex(ValueError, "another lifetime"):
                    fixture.restart_request(root, "L38-cs2a2", target, "L37-cs2b=" + digest)
                fixture.restart_request(root, "L38-cs2a2", target, "L38-cs2a2=" + digest)
            finally:
                fixture.UNIDLE_CLOSE_EXCEPTIONS.clear()
                fixture.UNIDLE_CLOSE_EXCEPTIONS.update(original)
            record = json.loads((target / "restart-exception.json").read_text(encoding="utf-8"))
            self.assertEqual(record["lifetime"], "L38-cs2a2")
            self.assertTrue((target / "restart-request.json").is_file())

    def test_close_attaches_the_post_restart_verification(self):
        with tempfile.TemporaryDirectory() as folder:
            life = Path(folder)
            handle = {}
            fixture.attach_verification(handle, life)
            self.assertNotIn("post_restart_verification", handle)
            (life / "post-restart-verification.json").write_text(json.dumps({"passed": True, "problems": []}))
            fixture.attach_verification(handle, life)
            self.assertEqual(handle["post_restart_verification"]["passed"], True)

if __name__ == "__main__":
    unittest.main()
