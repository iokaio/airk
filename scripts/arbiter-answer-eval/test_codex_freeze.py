# SPDX-License-Identifier: Apache-2.0
"""Synthetic Stage A freeze and pass generation checks with no runtime."""

import copy
import json
import unittest
from pathlib import Path
from unittest.mock import patch

import common
import judge
import make_loop_spec
import prompts_freeze


class StageAFreeze(unittest.TestCase):
    def setUp(self):
        self.root = Path.cwd() / "synthetic-stage-a"
        self.manifest_path = self.root / "codex-freeze.json"
        self.inputs = {
            "purpose": "windows-stage-a",
            "scenario_freeze": str(self.root / "scenario-freeze.json"),
            "executable": str(self.root / "codex.exe"),
            "executable_sha256": "a" * 64,
            "model_catalog": str(self.root / "models.json"),
            "model_catalog_sha256": "b" * 64,
            "tool_catalogs": {
                name: {"path": str(self.root / f"{name}-tools.json"), "sha256": digest * 64}
                for name, digest in zip(("search", "parse", "answer"), "cde", strict=True)
            },
        }
        self.freeze_sha = "f" * 64

    def current(self, purpose="windows-stage-a"):
        inputs = {**self.inputs, "purpose": purpose}
        tools = {tool: {} for roster in common.PATH_TOOLS.values() for tool in roster}
        with (patch.object(prompts_freeze, "pinned_input", side_effect=self.pin),
              patch.object(common, "load_catalog", return_value=tools)):
            return {"schema": prompts_freeze.CODEX_SCHEMA, **prompts_freeze.codex_current(inputs)}

    def pin(self, path, sha256=None):
        return {"path": path, "sha256": sha256 or "0" * 64}

    def spec(self, manifest, stage="baseline"):
        with (patch.object(prompts_freeze, "verify", return_value={"sha256": self.freeze_sha}),
              patch.object(common, "read_json", return_value=manifest),
              patch.object(Path, "is_file", return_value=True)):
            return make_loop_spec.stage_a_spec(
                "synthetic", self.root, make_loop_spec.PROFILES, list(common.REPOS), {}, "",
                freeze_path=self.manifest_path, codex_home=self.root / "home", handle=self.root / "handle.json",
                stage=stage,
            )

    def check(self, spec, manifest, catalogs=None):
        provenance = {
            "driver": manifest["runner"], "executable": self.inputs["executable"],
            "executable_sha256": self.inputs["executable_sha256"],
            "model_catalog_sha256": self.inputs["model_catalog_sha256"],
        }
        if catalogs is None:
            catalogs = {p["path"]: p["sha256"] for p in manifest["tool_catalogs"].values()}
        prompts_freeze.verify_stage_a_spec(spec, manifest, provenance, catalogs)

    def test_freeze_pins_strict_schema_driver_and_roles(self):
        frozen = self.current()
        self.assertEqual((frozen["provider"], frozen["agent_model"], frozen["agent_effort"]),
                         ("codex", "gpt-6.2-sol", "medium"))
        self.assertEqual((frozen["judge_model"], frozen["judge_effort"], frozen["scoring_judges"]),
                         ("gpt-6.2-astra", "medium", ["j1", "j2"]))
        self.assertEqual(json.loads(frozen["judge_schema"]["text"]), judge.codex_verdict_schema())
        self.assertEqual(frozen["tool_catalogs"], self.inputs["tool_catalogs"])
        self.assertEqual(frozen["files"]["codex_isolation_probe.py"],
                         common.sha256_file(Path(prompts_freeze.__file__).with_name("codex_isolation_probe.py")))
        with patch.object(prompts_freeze, "codex_current", return_value={
            key: val for key, val in frozen.items() if key != "schema"
        }):
            prompts_freeze.verify_codex(frozen)
            with self.assertRaisesRegex(common.HarnessError, "judge_effort"):
                prompts_freeze.verify_codex({**frozen, "judge_effort": "high"})

    def test_missing_tool_and_changed_catalog_are_refused(self):
        with (patch.object(prompts_freeze, "pinned_input", side_effect=self.pin),
              patch.object(common, "load_catalog", return_value={})):
            with self.assertRaisesRegex(common.HarnessError, "missing search tools"):
                prompts_freeze.codex_current(self.inputs)
        manifest = self.current()
        spec = self.spec(manifest)
        self.check(spec, manifest)
        hashes = {p["path"]: p["sha256"] for p in manifest["tool_catalogs"].values()}
        hashes[self.inputs["tool_catalogs"]["search"]["path"]] = "9" * 64
        with self.assertRaisesRegex(common.HarnessError, "search catalog differs"):
            self.check(spec, manifest, hashes)

    def test_normal_sol_and_astra_passes_remain_distinct(self):
        sol_manifest = self.current()
        astra_manifest = self.current("windows-stage-a-capable-panel")
        sol = self.spec(sol_manifest, "change-set-2")
        astra = self.spec(astra_manifest, "change-set-3")
        for spec, manifest, names in (
            (sol, sol_manifest, ["search", "parse", "focus", "compact", "arbiter"]),
            (astra, astra_manifest, ["search", "parse", "compact"]),
        ):
            self.check(spec, manifest)
            self.assertEqual(len(spec["scenario_ids"]), 20)
            self.assertEqual([entry["configuration"] for entry in spec["results"]], names)
            self.assertFalse(any(entry["reuse"] for entry in spec["results"]))
            self.assertNotIn("claude", spec)
            self.assertNotIn("cpu", {entry["device"] for entry in spec["results"]})
            self.assertEqual(spec["codex_config"]["freeze_sha256"], self.freeze_sha)
            plan = make_loop_spec.l7_plan(Path("spec.json"), spec, self.root)
            self.assertEqual(plan[0]["sessions"]["groups"], [[None, None, "search,parse"]])
        self.assertEqual((sol["agent_model"], astra["agent_model"]), ("gpt-6.2-sol", "gpt-6.2-astra"))
        with self.assertRaisesRegex(common.HarnessError, "Astra panel runs only"):
            self.spec(astra_manifest, "change-set-1")
        changed = copy.deepcopy(sol)
        changed["results"][2]["device"] = "cpu"
        with self.assertRaisesRegex(common.HarnessError, "arms differ"):
            self.check(changed, sol_manifest)
        shortened = {**sol, "scenario_ids": sol["scenario_ids"][:-1]}
        with self.assertRaisesRegex(common.HarnessError, "all 20"):
            self.check(shortened, sol_manifest)
        crossed = copy.deepcopy(sol)
        crossed["results"][0]["stages"]["focus"] = "unexpected"
        with self.assertRaisesRegex(common.HarnessError, "stage settings"):
            self.check(crossed, sol_manifest)

    def test_legacy_generation_retains_legacy_defaults(self):
        spec = make_loop_spec.l7_spec("archive", self.root, make_loop_spec.PROFILES, list(common.REPOS), {})
        self.assertNotIn("provider", spec)
        self.assertEqual(spec["agent_model"], common.AGENT_MODEL)
        self.assertEqual(spec["judge_model"], common.JUDGE_MODEL)


if __name__ == "__main__":
    unittest.main()