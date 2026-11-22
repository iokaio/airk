# SPDX-License-Identifier: Apache-2.0
"""Synthetic checks for the Answer-evaluation fixture launcher; no service starts."""

import argparse
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("answer_eval_fixture", HERE / "fixture.py")
fixture = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fixture)


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
        env = fixture.emitter_environment(handle, scopes, "focus", False, "ab" * 32, "p-1", ["R1", "R2"])
        self.assertEqual(env["AIRK_ARBITER_EVALUATION_WORK"], "W")
        self.assertEqual(env["AIRK_ARBITER_EVALUATION_ENABLED"], "false")
        self.assertEqual(env["AIRK_ARBITER_EVALUATION_SCOPES_SHA256"], "cd" * 32)
        self.assertEqual(json.loads(env["AIRK_ARBITER_EVALUATION_EXTERNAL_SOURCES"]),
                         {"mode": "read-only", "source_roots": ["R1", "R2"]})
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


class TrimBusyRetry(unittest.TestCase):
    def test_registry_busy_is_retried_and_other_errors_are_not(self):
        """Regression (L16-cs1, 22 Nov 2026): a trim refused with registry_busy is retried with a new draft."""
        calls = []
        original_trim, original_policy, original_atoms = fixture.trim_path, fixture.index_policy, fixture.denied_atoms
        try:
            def busy_then_ok(commands, index_id, source_root_id, kind, path, nonce):
                calls.append(nonce)
                if len(calls) < 3:
                    raise ValueError('service refused catalog_source_mutation_draft: {"code": "registry_busy"}')
                return {"draft_preview_digest": "d"}
            fixture.trim_path = busy_then_ok
            fixture.index_policy = lambda commands, index_id: {}
            fixture.denied_atoms = lambda status: (set(), None)
            outcome = fixture.trim_with_busy_retry(None, "i", "s", "exact", "a/b", sleep=lambda s: None)
            self.assertEqual(outcome["draft_preview_digest"], "d")
            self.assertEqual(len(set(calls)), 3)
            fixture.trim_path = lambda *a: (_ for _ in ()).throw(ValueError("schema refused"))
            with self.assertRaises(ValueError):
                fixture.trim_with_busy_retry(None, "i", "s", "exact", "a/b", sleep=lambda s: None)
            calls.clear()
            fixture.trim_path = lambda *a: (calls.append(1), (_ for _ in ()).throw(
                ValueError("registry_busy")))[1]
            fixture.denied_atoms = lambda status: ({("exact", "a/b")}, None)
            landed = fixture.trim_with_busy_retry(None, "i", "s", "exact", "a/b", sleep=lambda s: None)
            self.assertEqual(landed["applied"], "already denied after a busy retry")
            self.assertEqual(len(calls), 1)
        finally:
            fixture.trim_path, fixture.index_policy, fixture.denied_atoms = original_trim, original_policy, original_atoms


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
