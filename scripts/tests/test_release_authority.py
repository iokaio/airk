# SPDX-License-Identifier: Apache-2.0
"""Independent campaign context, live-origin and environment custody cases."""
from copy import deepcopy
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("release_authority", ROOT / "scripts/release-authority.py")
AUTHORITY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUTHORITY)
CASES = json.loads((ROOT / "fixtures/split/receipts/release_authority.json").read_text())
COMMIT = "a" * 40


class CampaignAuthorityTests(unittest.TestCase):
    def test_exact_manual_context_catalog(self):
        for row in CASES["context_cases"]:
            environment = {**CASES["context"], **row["set"]}
            for name in row["remove"]:
                environment.pop(name, None)
            with self.subTest(case=row["name"]):
                if row["expect"] == "refused":
                    with self.assertRaises(ValueError):
                        AUTHORITY.manual_context(environment, "github-release.yml")
                else:
                    result = AUTHORITY.manual_context(environment, "github-release.yml")
                    self.assertEqual(result, {"branch": row["expect"], "commit": COMMIT,
                                              "campaign": row["expect"] == "feat/advanced-split"})

    def api(self, fixture, calls):
        def read(path, paginate=False):
            calls.append((path, paginate))
            if path == "actions/runs/123":
                return deepcopy(fixture["run"])
            if path == "actions/workflows/ci.yml":
                return deepcopy(fixture["workflow"])
            if path == "actions/runs/123/attempts/2/jobs?per_page=100" and paginate:
                return [{"jobs": deepcopy(fixture["jobs"])}]
            raise AssertionError("Unreviewed API call: " + path)
        return read

    def test_claimed_record_cannot_replace_actual_run_workflow_or_repository_observation(self):
        for row in CASES["live_cases"]:
            fixture = deepcopy(CASES["live"])
            for path, value in row["set"]:
                target = fixture
                for key in path[:-1]:
                    target = target[key]
                target[path[-1]] = value
            calls = []
            with self.subTest(case=row["name"]):
                if row["expect"] == "refused":
                    with self.assertRaises(ValueError):
                        AUTHORITY.verify_campaign_ci(self.api(fixture, calls), 123, COMMIT)
                else:
                    self.assertEqual(AUTHORITY.verify_campaign_ci(self.api(fixture, calls), 123, COMMIT), fixture["run"])
                self.assertIn(("actions/runs/123", False), calls)
                self.assertIn(("actions/workflows/ci.yml", False), calls)

    def test_every_independent_target_contract_and_container_job_is_required_once(self):
        for index, required in enumerate(CASES["live"]["jobs"]):
            for fault in ("missing", "duplicate", "failed", "skipped"):
                fixture = deepcopy(CASES["live"])
                if fault == "missing":
                    fixture["jobs"].pop(index)
                elif fault == "duplicate":
                    fixture["jobs"].append(deepcopy(required))
                else:
                    fixture["jobs"][index]["conclusion"] = fault
                with self.subTest(job=required["name"], fault=fault), self.assertRaises(ValueError):
                    AUTHORITY.verify_campaign_ci(self.api(fixture, []), 123, COMMIT)

    def test_actual_extended_native_step_success_is_required_on_every_target(self):
        required = CASES["qualification_steps"]["required"]
        self.assertIn("- name: " + required, (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8"))
        selected = [index for index, job in enumerate(CASES["live"]["jobs"]) if job["name"].startswith("Native / ")]
        self.assertEqual(len(selected), 4)
        for index in selected:
            for fault in CASES["qualification_steps"]["faults"]:
                fixture = deepcopy(CASES["live"])
                steps = fixture["jobs"][index]["steps"]
                if fault == "missing":
                    steps.clear()
                elif fault == "duplicate":
                    steps.append(deepcopy(steps[0]))
                else:
                    steps[0]["conclusion"] = fault
                with self.subTest(target=fixture["jobs"][index]["name"], fault=fault), self.assertRaisesRegex(ValueError, "qualification_step"):
                    AUTHORITY.verify_campaign_ci(self.api(fixture, []), 123, COMMIT)

    def test_live_publisher_origin_cannot_be_replaced_by_claimed_environment_values(self):
        def check(run):
            def api(path):
                if path == "actions/runs/789": return run
                if path == "actions/workflows/github-release.yml": return CASES["publisher"]["workflow"]
                raise AssertionError(path)
            return AUTHORITY.verify_campaign_publisher(api, "789", "1", COMMIT)
        self.assertEqual(check(CASES["publisher"]["run"]), CASES["publisher"]["run"])
        for row in CASES["publisher"]["cases"]:
            run = deepcopy(CASES["publisher"]["run"]); run[row["field"]] = row["value"]
            with self.subTest(field=row["field"], value=row["value"]), self.assertRaises(ValueError): check(run)

    def test_old_main_records_stay_unchanged_and_campaign_records_are_explicitly_versioned(self):
        for schema in ("airk-ci-producer/1", "airk-ci-producer/2", "airk-signed-release-producer/1"):
            record = {"schema": schema, "commit": COMMIT, "retained": {"future": [1, 2]}}
            before = json.dumps(record, separators=(",", ":"))
            self.assertEqual(AUTHORITY.record_branch(record), "main")
            self.assertEqual(json.dumps(record, separators=(",", ":")), before)
            with self.assertRaises(ValueError):
                AUTHORITY.record_branch({**record, "source_branch": "feat/advanced-split"})
        for schema in ("airk-ci-producer/3", "airk-signed-release-producer/2"):
            record = {"schema": schema, "repository": "iokaio/airk", "commit": COMMIT,
                      "event": "workflow_dispatch", "source_branch": "feat/advanced-split", "qualified_commit": COMMIT}
            self.assertEqual(AUTHORITY.record_branch(record), "feat/advanced-split")
            for key, value in (("source_branch", "other"), ("qualified_commit", "b" * 40),
                               ("event", "push"), ("repository", "foreign/airk")):
                with self.subTest(schema=schema, field=key), self.assertRaises(ValueError):
                    AUTHORITY.record_branch({**record, key: value})
            with self.subTest(schema=schema, invalid="main without qualification"), self.assertRaises(ValueError):
                AUTHORITY.record_branch({**record, "source_branch": "main", "qualified_commit": None})
            with self.subTest(schema=schema, invalid="campaign null qualification"), self.assertRaises(ValueError):
                AUTHORITY.record_branch({**record, "qualified_commit": None})
            if schema == "airk-ci-producer/3":
                for promotion in (None, {}, {"producer": {"source_commit": COMMIT}}):
                    with self.subTest(promotion=promotion), self.assertRaises(ValueError):
                        AUTHORITY.record_branch({**record, "promotion": promotion})


class EnvironmentCustodyTests(unittest.TestCase):
    def fixture(self):
        before = deepcopy(CASES["environments"])
        plan = AUTHORITY.environment_change_plan(before, COMMIT)
        current = deepcopy(before)
        created = []
        for index, environment in enumerate(current):
            policy_id = 201 + index
            environment["branch_policies"].append({"id": policy_id, "name": "feat/advanced-split", "type": "branch"})
            created.append({"environment": environment["name"], "policy_id": policy_id})
        return before, plan, created, current

    def test_plan_adds_only_exact_branch_and_preserves_all_existing_main_and_reviewers(self):
        before, plan, created, current = self.fixture()
        self.assertEqual(plan["before"], before)
        self.assertEqual(len(plan["additions"]), 5)
        for step in plan["additions"]:
            self.assertEqual(step["method"], "POST")
            self.assertEqual(step["body"], {"name": "feat/advanced-split", "type": "branch"})
        original = deepcopy(current)
        deletes = AUTHORITY.environment_restoration_plan(plan, created, current)
        self.assertEqual(current, original, "the pure plan does not mutate an environment")
        self.assertEqual(len(deletes), 5)
        for row, step in zip(created, deletes):
            self.assertEqual(step, {"environment": row["environment"], "method": "DELETE",
                                   "path": f"environments/{row['environment']}/deployment-branch-policies/{row['policy_id']}"})
        self.assertEqual(AUTHORITY.environment_restoration_plan(plan, created[:1], current), deletes[:1],
                         "a partially applied plan restores only successful creations")

    def test_existing_campaign_policy_is_never_adopted_for_removal(self):
        before = deepcopy(CASES["environments"])
        before[0]["branch_policies"].append({"id": 99, "name": "feat/advanced-split", "type": "branch"})
        plan = AUTHORITY.environment_change_plan(before, COMMIT)
        self.assertEqual(len(plan["additions"]), 4)
        self.assertNotIn(before[0]["name"], [step["environment"] for step in plan["additions"]])
        with self.assertRaises(ValueError):
            AUTHORITY.environment_restoration_plan(plan, [{"environment": before[0]["name"], "policy_id": 99}], before)

    def test_changed_reviewers_main_policy_created_id_or_receipt_refuses_automatic_restoration(self):
        for fault in ("reviewers", "main", "id", "renamed", "extra", "receipt", "duplicate"):
            before, plan, created, current = self.fixture()
            if fault == "reviewers":
                current[0]["protection_rules"] = []
            elif fault == "main":
                current[0]["branch_policies"].pop(0)
            elif fault == "id":
                created[0]["policy_id"] = 999
            elif fault == "renamed":
                current[0]["branch_policies"][-1]["name"] = "other"
            elif fault == "extra":
                current[0]["branch_policies"].append({"id": 999, "name": "unrelated", "type": "branch"})
            elif fault == "receipt":
                plan["before"][0]["protection_rules"] = []
            elif fault == "duplicate":
                created.append(deepcopy(created[0]))
            with self.subTest(fault=fault), self.assertRaises(ValueError):
                AUTHORITY.environment_restoration_plan(plan, created, current)

    def test_missing_environment_or_changed_branch_policy_mode_refuses_plan(self):
        snapshot = deepcopy(CASES["environments"])
        with self.assertRaises(ValueError):
            AUTHORITY.environment_change_plan(snapshot[:-1], COMMIT)
        snapshot[0]["deployment_branch_policy"] = {"protected_branches": True, "custom_branch_policies": False}
        with self.assertRaises(ValueError):
            AUTHORITY.environment_change_plan(snapshot, COMMIT)


if __name__ == "__main__":
    unittest.main()
