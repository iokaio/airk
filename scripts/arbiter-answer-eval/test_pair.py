# SPDX-License-Identifier: Apache-2.0
"""Synthetic tests for the loop pair (munarium and bookbuilder): defaults, the generic leak check, no airk index.

No service, index, model or private input is used. The synthetic paths below are invented.
"""

import argparse
import json
import tempfile
import unittest
from pathlib import Path

import common
import gate_report
import grade
import leak_check
import make_loop_spec
import run_plan
import settled

fixture = leak_check.fixture_module()


def scenario(repo, slot="C1", phrase="alpha beta gamma"):
    notes = [{"id": f"N{n}", "mark": "essential", "path": f"src/file{n}.rs", "lines": [1, 2], "symbol": None,
              "fact": "A fact.", "alternatives": []} for n in range(1, 5)]
    return {"schema": "airk.private.answer-eval-scenario/1", "id": f"{repo}-{slot}", "repository": repo,
            "set": "development", "slot": slot, "kind": "coding", "instruction": "Do the thing.",
            "leak_probe": phrase, "source_commit": "0" * 40, "notes": notes}


def write_scenarios(root, rows):
    for folder, repo, slot, phrase in rows:
        path = Path(root) / folder / repo / f"{slot}.json"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(scenario(repo, slot, phrase)), encoding="utf-8")


class Defaults(unittest.TestCase):
    def test_the_default_pair_is_munarium_and_bookbuilder_and_airk_stays_known(self):
        self.assertEqual(common.REPOS, ("munarium", "bookbuilder"))
        self.assertEqual(common.LOOP_INDEXES, ("bookbuilder-v2", "munarium"))
        self.assertNotIn("airk", common.REPOS)
        self.assertEqual(set(common.KNOWN_REPOS), {"airk", "bookbuilder", "munarium"})
        self.assertEqual(common.FORBIDDEN_LOOP_INDEX, "airk")
        # The launcher's own default: the repository with denies first.
        self.assertEqual(fixture.DEFAULT_REPOSITORIES, common.LOOP_INDEXES)
        self.assertIsNone(fixture.repository_order_problem(common.LOOP_INDEXES, Path("no-such-root")))

    def test_scenario_files_and_loaders_take_munarium_and_still_load_airk_history(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            write_scenarios(root, [("scenarios", "munarium", "C1", "one"), ("scenarios", "munarium", "A3", "two"),
                                   ("scenarios", "bookbuilder", "C1", "three"), ("scenarios", "airk", "C1", "four")])
            files = common.scenario_files(root / "scenarios")
            self.assertEqual([f"{p.parent.name}/{p.name}" for p in files],
                             ["munarium/C1.json", "munarium/A3.json", "bookbuilder/C1.json"])
            self.assertEqual([common.load_scenario(p)["repository"] for p in files],
                             ["munarium", "munarium", "bookbuilder"])
            history = common.scenario_files(root / "scenarios", repos=("airk",))
            self.assertEqual(common.load_scenario(history[0])["id"], "airk-C1")
            unknown = root / "scenarios" / "munarium" / "W1.json"
            unknown.write_text(json.dumps(dict(scenario("munarium", "W1"), repository="other")), encoding="utf-8")
            with self.assertRaises(common.HarnessError):
                common.load_scenario(unknown)

    def test_tool_defaults_name_the_pair(self):
        spec = make_loop_spec.gate_spec("06-gate-lg", Path("R"), make_loop_spec.PROFILES, list(common.REPOS), {})
        self.assertEqual(spec["repos"], ["munarium", "bookbuilder"])
        self.assertEqual(spec["settle_repos"], ["munarium", "bookbuilder"])
        self.assertTrue(all(i.startswith(("munarium-", "bookbuilder-")) for i in spec["scenario_ids"]))
        self.assertEqual(len(spec["scenario_ids"]), 20)
        template = make_loop_spec.template()
        self.assertEqual(template["spec"]["repos"], ["munarium", "bookbuilder"])
        self.assertNotIn("airk-C1", template["spec"]["scenario_ids"])
        parser = argparse.ArgumentParser()
        parser.add_argument("--repo", choices=common.KNOWN_REPOS)
        self.assertEqual(parser.parse_args(["--repo", "munarium"]).repo, "munarium")
        self.assertEqual(parser.parse_args(["--repo", "airk"]).repo, "airk")

    def test_rule_counts_wins_for_the_repositories_the_pass_ran(self):
        def rows(first, second):
            out = []
            for n in range(20):
                repo = first if n < 10 else second
                win = n % 10 < 6
                out.append({"repo": repo, "kind": "coding",
                            "case": {"case": "substantial-win" if win else "tie", "substantial": win, "worse": False},
                            "answer_grade": 3, "best_grade": 2})
            return out
        loop = grade.configuration_rule(rows("munarium", "bookbuilder"))
        self.assertEqual(loop["wins_by_repo"], {"bookbuilder": 6, "munarium": 6})
        self.assertTrue(loop["substantially_improves"])
        history = grade.configuration_rule(rows("airk", "bookbuilder"))
        self.assertEqual(history["wins_by_repo"], {"airk": 6, "bookbuilder": 6})
        self.assertTrue(history["substantially_improves"])
        one_sided = rows("munarium", "bookbuilder")
        for row in one_sided[10:]:
            row["case"] = {"case": "tie", "substantial": False, "worse": False}
        for row in one_sided[:10]:
            row["case"] = {"case": "substantial-win", "substantial": True, "worse": False}
        self.assertEqual(grade.configuration_rule(one_sided)["wins_by_repo"], {"bookbuilder": 0, "munarium": 10})
        self.assertFalse(grade.configuration_rule(one_sided)["substantially_improves"])
        self.assertEqual(grade.configuration_rule([])["wins_by_repo"], {"munarium": 0, "bookbuilder": 0})

    def test_munarium_is_private_in_the_gate_report(self):
        reason = 'The agent read `fn check_token()` and "the capability table" before finishing.'
        self.assertEqual(gate_report.PRIVATE_REPOS, ("bookbuilder", "munarium"))
        for repo in ("munarium", "bookbuilder"):
            cleaned = gate_report.reason_for(repo, reason)
            self.assertNotIn("check_token", cleaned)
            self.assertNotIn("capability table", cleaned)
            self.assertEqual(cleaned, gate_report.WITHHELD)
        self.assertIn("check_token", gate_report.reason_for("airk", reason))

    def test_settled_and_plan_defaults(self):
        self.assertEqual(run_plan.settle_repositories({}), ["munarium", "bookbuilder"])
        self.assertEqual(run_plan.settle_repositories({"repositories": "munarium"}), ["munarium"])
        # Stage commands name no repository unless the row gives index names: the launcher then enrols every
        # index of the lifetime, so a scenario name such as "bookbuilder" never reaches `stage`.
        self.assertEqual(run_plan.stage_repositories({"repositories": "munarium,bookbuilder"}), [])
        self.assertEqual(run_plan.stage_repositories({"stage_repositories": "munarium,bookbuilder-v2"}),
                         ["--repositories", "munarium,bookbuilder-v2"])
        self.assertTrue(callable(settled.check))


class Handles(unittest.TestCase):
    def test_bookbuilder_resolves_through_the_root_alias_file(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder) / "fixture-06-gate-lg-C"
            handle_path = root / "06-gate-lg-C-base" / "handle.json"
            handle_path.parent.mkdir(parents=True)
            handle_path.write_text(json.dumps({
                "state": "ready", "cli": "airkctl", "control": "c",
                "indexes": {"bookbuilder-v2": {"index_id": "idx_b", "binding": "b_b"},
                            "munarium": {"index_id": "idx_m", "binding": "b_m"}}}), encoding="utf-8")
            (root / "index-aliases.json").write_text(json.dumps({"aliases": {"bookbuilder": "bookbuilder-v2"}}),
                                                     encoding="utf-8")
            handle = common.load_handle(handle_path)
            self.assertEqual(handle.index_id("munarium"), "idx_m")
            self.assertEqual(handle.index_id("bookbuilder"), "idx_b")
            self.assertEqual(handle.binding("bookbuilder"), "b_b")
            self.assertEqual(leak_check.index_name(handle, "bookbuilder"), "bookbuilder-v2")
            self.assertEqual(leak_check.index_name(handle, "munarium"), "munarium")
            with self.assertRaises(common.HarnessError):
                handle.index_id("airk")
            self.assertEqual(leak_check.forbidden_index(handle_path, [], fixture), [])
            (root / "indexes").mkdir()
            (root / "indexes" / "airk.state.json").write_text("{}", encoding="utf-8")
            (root / "indexes" / "minilm.state.json").write_text("{}", encoding="utf-8")
            self.assertEqual(leak_check.forbidden_index(handle_path, [], fixture),
                             [{"where": "state file", "index": "airk"}])


class ScenarioProbes(unittest.TestCase):
    def test_only_scenario_objects_of_the_repository_are_read(self):
        """Regression (22 Nov 2026): a review's findings.json (a list) lay beside the scenario folders."""
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            write_scenarios(root, [("scenarios", "munarium", "C1", "one two"), ("scenarios", "munarium", "A1", "three"),
                                   ("sealed", "munarium", "C1", "four"), ("scenarios", "bookbuilder", "C1", "five")])
            review = root / "scenarios" / "munarium-review"
            review.mkdir()
            (review / "findings.json").write_text(json.dumps([{"id": 1}]), encoding="utf-8")
            (root / "scenarios" / "munarium" / "notes.json").write_text(json.dumps(["not a scenario"]),
                                                                        encoding="utf-8")
            (root / "scenarios" / "munarium" / "other.json").write_text(
                json.dumps(dict(scenario("bookbuilder", "C2", "six"))), encoding="utf-8")
            rows = leak_check.scenario_probes(root, "munarium")
            self.assertEqual([(row["source"], row["phrase"]) for row in rows],
                             [("scenarios/munarium/A1.json", "three"), ("scenarios/munarium/C1.json", "one two"),
                              ("sealed/munarium/C1.json", "four")])
            self.assertTrue(all("sha256" not in row for row in rows))
            self.assertEqual(leak_check.scenario_probes(root, "airk"), [])

    def test_the_freeze_is_used_when_it_holds_the_repository(self):
        import hashlib
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            write_scenarios(root, [("scenarios", "bookbuilder", "C1", "frozen"),
                                   ("scenarios", "bookbuilder", "C2", "new"),
                                   ("scenarios", "munarium", "C1", "unfrozen")])
            relative = "scenarios/bookbuilder/C1.json"
            digest = hashlib.sha256((root / relative).read_bytes()).hexdigest()
            (root / "scenario-freeze.json").write_text(json.dumps({"files": {relative: digest}}), encoding="utf-8")
            self.assertEqual(leak_check.scenario_probes(root, "bookbuilder"),
                             [{"source": relative, "phrase": "frozen", "sha256": digest}])
            # munarium is not in the freeze yet, so its scenarios are read from its folder, without a pin.
            self.assertEqual([row["phrase"] for row in leak_check.scenario_probes(root, "munarium")], ["unfrozen"])
            (root / relative).write_text("{}", encoding="utf-8")
            with self.assertRaises(common.HarnessError):
                leak_check.scenario_probes(root, "munarium")


class LeakCheck(unittest.TestCase):
    LISTING = {"paths": ["src/a.rs", "docs/lab/example/.env.example", "README.md"], "complete": True, "restarts": 0,
               "errors": []}

    def probes(self, *phrases, frozen=True):
        return [dict({"source": f"scenarios/x/C{n}.json", "phrase": phrase}, **({"sha256": "ab"} if frozen else {}))
                for n, phrase in enumerate(phrases, 1)]

    def search(self, table):
        calls = []

        def run(phrase):
            calls.append(phrase)
            return {"ok": True, "error": None, "paths": table.get(phrase, []), "elapsed_ms": 1}
        return run, calls

    def test_a_repository_without_exclusions_is_still_probed_and_records_the_empty_list(self):
        run, calls = self.search({"one two": ["src/a.rs"]})
        row = leak_check.check_repository("munarium", "munarium", self.probes("one two", "three four", frozen=False),
                                          self.LISTING, fixture, run)
        self.assertEqual(calls, ["one two", "three four"])
        self.assertEqual((row["exclusions"], row["probe_count"], row["listed_files"]), ([], 2, 3))
        self.assertEqual(row["probe_source"], "scenario folders (not frozen)")
        self.assertEqual(row["listed_excluded_paths"], [])
        self.assertTrue(row["passed"])
        # A credential-like name is a finding to read, not a failure: only an excluded path fails the check.
        self.assertEqual(row["credentials"]["matches"], {".env.*": ["docs/lab/example/.env.example"]})
        none = leak_check.check_repository("munarium", "munarium", [], self.LISTING, fixture, run)
        self.assertEqual((none["probe_count"], none["probe_source"], none["passed"]),
                         (0, "none: no scenario names this repository yet", True))

    def test_each_repository_is_checked_against_its_own_index_rules(self):
        key = "ironspire/scripts/set-rss-key.ps1"
        run, _ = self.search({"served": [key, "src/a.cs"], "clean": ["src/a.cs"]})
        row = leak_check.check_repository("bookbuilder", "bookbuilder-v2", self.probes("served", "clean"),
                                          {"paths": ["src/a.cs"], "complete": True}, fixture, run)
        self.assertFalse(row["passed"])
        self.assertEqual(row["failures"], [{"repository": "bookbuilder", "scenario": "scenarios/x/C1.json",
                                           "path": key}])
        self.assertEqual(row["probe_source"], "scenario freeze")
        self.assertEqual(sorted(tuple(atom) for atom in row["exclusions"]), sorted(fixture.EXCLUSIONS["bookbuilder-v2"]))
        # The same path is no finding for munarium: its list is its own (empty).
        other = leak_check.check_repository("munarium", "munarium", self.probes("served"), self.LISTING, fixture, run)
        self.assertTrue(other["passed"])
        listed = leak_check.check_repository("bookbuilder", "bookbuilder-v2", [],
                                             {"paths": ["src/a.cs", key], "complete": True}, fixture, run)
        self.assertEqual((listed["listed_excluded_paths"], listed["passed"]), ([key], False))
        self.assertEqual(listed["credentials"]["matches"], {"set-*-key.ps1": [key]})

    def test_a_failed_search_or_an_incomplete_listing_fails_the_check(self):
        row = leak_check.check_repository("munarium", "munarium", self.probes("x"), self.LISTING, fixture,
                                          lambda phrase: {"ok": False, "error": "CLI exit 1", "paths": []})
        self.assertFalse(row["passed"])
        self.assertEqual(row["failures"][0]["error"], "CLI exit 1")
        run, _ = self.search({})
        row = leak_check.check_repository("munarium", "munarium", [], dict(self.LISTING, complete=False), fixture, run)
        self.assertFalse(row["passed"])
        with self.assertRaises(common.HarnessError):
            leak_check.check_repository("other", "other", [], self.LISTING, fixture, run)


if __name__ == "__main__":
    unittest.main()
