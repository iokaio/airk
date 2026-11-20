# SPDX-License-Identifier: Apache-2.0
"""Model-free controls for the R7 protected Compact launcher input seam."""
import copy
import hashlib
from pathlib import Path
import runpy
from types import SimpleNamespace
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
OWNER = runpy.run_path(str(HERE/"compact_cutlass_cal_r7.py"))
BASE = OWNER["BASE"]


class R7Inputs(unittest.TestCase):
    def setUp(self):
        self.ids = [f"q{index}" for index in range(100)]
        self.source = "a"*64
        self.publication = {"token": "original"}
        self.focus, self.pipeline = "original-focus", "original-pipeline"
        self.rows = [{"ordinal": index, "id": key, "status": "no_native_assessment_request", "captures": []}
                     for index, key in enumerate(self.ids)]
        self.batch = {"model": BASE["MODEL"], "original_manifest": BASE["ORIGINAL"],
                      "phase_plan_sha256": self.source, "launch_audit_sha256": OWNER["AUDIT"],
                      "NN_invocations": 0, "original_requests": 100, "rows": self.rows,
                      "phases": list(range(13)), "statuses": {"no_native_assessment_request": 100}}

    def test_exact_terminal_phase_and_retained_refusals(self):
        rows, phases = OWNER["validate_population"](self.batch, self.ids, 100, self.source)
        self.assertEqual(len(phases), 13)
        self.assertEqual([row["ordinal"] for row in rows[96:]], [96, 97, 98, 99])
        self.assertTrue(all("compiled" not in row for row in rows))

    def test_historical_or_reordered_projection_refuses(self):
        changed = [
            self.batch | {"launch_audit_sha256": BASE["AUDIT"]},
            self.batch | {"phase_plan_sha256": BASE["PLAN5"]},
            self.batch | {"rows": self.rows[:2][::-1] + self.rows[2:]},
        ]
        for batch in changed:
            with self.subTest(batch=batch), self.assertRaisesRegex(ValueError, "chronological R7"):
                OWNER["validate_population"](batch, self.ids, 100, self.source)

    def test_r7_prefix_cannot_dispatch_terminal_or_unsealed_phase(self):
        for phase, projection in ((7, "sha"), (0, None)):
            args = SimpleNamespace(cal_epoch="r7-first56", cal_phase=phase, cal_projection_receipt_sha256=projection,
                                   cal_compiler_sha256="sha")
            with self.subTest(phase=phase), self.assertRaisesRegex(ValueError, "outside the sealed projection"):
                OWNER["inputs"](args, None, None, None, audit=None)

    def test_phase_owner_and_scope_are_bound_to_the_closed_lifetime(self):
        phases = list(range(7, 13))
        lifetime = {"phases": phases, "phase_closures": [f"owner-{i}" for i in phases]}
        pin = {"sha256": "c"*64}
        rows = self.rows[96:]
        report = {"phase": 12, "phase_owner": "owner-12", "plan_sha256": self.source,
                  "input_audit_sha256": OWNER["AUDIT"], "closed_questions": 4, "certificate_count": 0,
                  "lifetime_sha256": pin["sha256"],
                  "rows": [dict(ordinal=r["ordinal"], question_id=r["id"], captures=[]) for r in rows]}
        OWNER["validate_phase"](report, 12, self.source, pin, lifetime, rows)
        for substitute in ({"phase_owner": "owner-11"}, {"lifetime_sha256": "0"*64}, {"input_audit_sha256": BASE["AUDIT"]}):
            with self.subTest(substitute=substitute), self.assertRaisesRegex(ValueError, "foreign R7 phase"):
                OWNER["validate_phase"](report | substitute, 12, self.source, pin, lifetime, rows)

    def test_exact_capture_compiler_allowance_and_publication(self):
        row = self.rows[0] | {"status": "eligible", "compiled": {"input_ids": [1, 2, 3]}, "remaining_ms": 8,
                              "roster": {"cards": [1]}, "remaining_input_tokens": 10}
        req = {"scope": {"publication": self.publication}, "roster": row["roster"],
               "remaining_ms": 10, "remaining_input_tokens": 10}
        cap = {"schema": "airk.private-selected-Context-native-admission-observation/1",
               "Context_NN_dispatch": False, "Focus_configuration": "ON", "actual_Focus_profile": self.focus,
               "actual_Context_profile": {"manifest": BASE["ORIGINAL"]}, "MiniLM_pipeline": self.pipeline,
               "request": req, "remaining_ns_at_observation": 8_100_000,
               "compiler_census": [{"model": BASE["MODEL"], "profile": BASE["ORIGINAL"], "compiled": row["compiled"],
                                    "fits_remaining_allowance": True, "NN_invocations": 0}]}
        OWNER["validate_capture"](cap, row, self.publication, self.focus, self.pipeline)
        changed = copy.deepcopy(cap)
        changed["request"]["scope"]["publication"]["token"] = "historical"
        with self.assertRaisesRegex(ValueError, "foreign R7 native admission"):
            OWNER["validate_capture"](changed, row, self.publication, self.focus, self.pipeline)
        with self.assertRaisesRegex(ValueError, "budget or card roster changed"):
            OWNER["validate_capture"](cap, row | {"remaining_ms": 10}, self.publication, self.focus, self.pipeline)

    def test_r7_selector_rejects_legacy_source_with_same_abi(self):
        files = dict(zip(OWNER["SOURCE_KEYS"][:2], OWNER["REGRESSION_SOURCES"], strict=True))
        build = {"groups": {"native": {"files": files}}}
        OWNER["scorer_sources"](build)
        files[OWNER["SOURCE_KEYS"][1]] = BASE["CORRECTED_SELECTOR"]
        with self.assertRaisesRegex(ValueError, "candidate-audit or phase-local"):
            OWNER["scorer_sources"](build)

    def test_launcher_joins_compiler_and_first_closed_lifetime(self):
        campaign = HERE/"synthetic-campaign"
        capture = campaign/"cal100-focus1024-capture-r7"
        life = capture/"lifetime-00"
        projection = campaign/"r7-chronological-first56-r1"
        compilation = campaign/"r7-chronological-native-first56-r1"
        build = campaign/"build/large-image-r1.json"
        image = campaign/"build/native.exe"
        index = {}

        def add(path, body, sha=None):
            digest = sha or hashlib.sha256(str(path).encode()).hexdigest()
            pin = {"path": str(path), "bytes": 10, "sha256": digest}
            index[path] = (pin, body)
            return pin

        census = add(campaign/"census.json", {"question_ids": self.ids})
        audit = add(campaign/"cal100-fused-candidate-launch-r1/cal100-launch-audit.json",
                    {"census": census, "profiles": {"arbiter-focus-qwen3-reranker-06b": self.focus},
                     "pipeline": self.pipeline}, OWNER["AUDIT"])
        acquired = add(capture/"control-mutex-acquired.json", {})
        prior = {"path": "anchor"}
        plan = add(capture/"lifetime-plan.json", {"schema": OWNER["PLAN"], "input_audit": audit,
                   "candidate_service": {"image": {"sha256": OWNER["SERVICE"]}}, "installation_mutex": acquired,
                   "installation": {"closed": prior}, "phase_groups": [list(range(7)), list(range(7, 13))],
                   "deadline_ms": 99}, self.source)
        phase_job = {"exit_code": 0, "child_assigned_before_resume": True, "timed_out": False,
                     "log_oversized": False, "owned_job_forced_termination": False,
                     "owned_descendants_settled": True, "owned_job_final_active_processes": 0,
                     "owned_job_final_member_pids": []}
        phase_pins = [add(life/f"phase-{i:02}.owned.json", phase_job | {"phase": i, "transcript":
                          add(life/f"phase-{i:02}.jsonl", {})}) for i in range(7)]
        closed = add(life/"lifetime-closure.json", {"schema": "airk.private.cal100-group-lifetime/1",
                     "group": 0, "plan": plan, "phases": list(range(7)), "prior": prior,
                     "phase_closures": phase_pins, "end_ms": 98, "publication": self.publication})
        reports = []
        for i,pin in enumerate(phase_pins):
            rows = self.rows[i*8:(i+1)*8]
            report = {"phase": i, "plan_sha256": self.source, "input_audit_sha256": OWNER["AUDIT"],
                      "closed_questions": 8, "certificate_count": 0, "lifetime_sha256": closed["sha256"],
                      "phase_owner": pin, "source_transcript": index[Path(pin["path"])][1]["transcript"],
                      "rows": [{"ordinal": r["ordinal"], "question_id": r["id"], "captures": []} for r in rows]}
            reports.append(add(projection/f"phase-{i:02}-independent-inputs.json", report))
        compact = add(projection/"compact.json", self.batch | {"original_requests": 56, "rows": self.rows[:56],
                      "phases": reports, "statuses": {"no_native_assessment_request": 56}})
        large = add(projection/"large.json", {})
        projection_owner = add(OWNER["RAW"]/"r7_chronological_inputs.py", {})
        compiler_owner = add(OWNER["RAW"]/"run_r7_compiler_batch.py", {})
        add(OWNER["LEGACY"], {}, OWNER["LEGACY_SHA"])
        projection_receipt = add(projection/"projection-receipt.json", {"schema": OWNER["PROJECTION"],
            "service_sha256": OWNER["SERVICE"], "original_requests": 56, "NN_invocations": 0, "gain_certificates": 0,
            "lifetimes": [closed["sha256"]], "terminal": None, "plan": plan, "plan_sha256": self.source,
            "acquired_sha256": acquired["sha256"], "launch_audit": audit, "models": {"compact": compact, "large": large},
            "source_sha256": projection_owner["sha256"]})
        sources = dict(zip(OWNER["SOURCE_KEYS"][:2], OWNER["REGRESSION_SOURCES"], strict=True))
        native_image = add(image, {})
        image_ref = {key: native_image[key] for key in ("path", "sha256")}
        counts = {"compact": "compact-count", "large": "large-count"}
        expected = {"AIRK_CUDA_CAL_LAUNCH_AUDIT": audit["path"],
                    "AIRK_CUDA_CAL_CHRONOLOGICAL_COMPACT": compact["path"],
                    "AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE": large["path"]}
        receipt = {"projection": projection_receipt, "passed": True, "NN_invocations": 0,
                   "launch_audit_sha256": OWNER["AUDIT"], "source_sha256": compiler_owner["sha256"],
                   "build": add(build, {"groups": {"native": {"files": sources}}}),
                   "build_log": add(build.with_suffix(".log"), {}),
                   "build_owner": add(build.parent/"owned-image/execution.json", {}),
                   "image": image_ref, "log_sha256": add(compilation/"native.log", {})["sha256"],
                   "job": {"suspended_assignment": {"pid": 7, "image": image_ref, "execution_user_sid": "human"},
                           "owned_leader_pid": 7},
                   "summary": counts,
                   "command": [str(image), "--exact", "compiler", "--ignored", "--nocapture", "--test-threads=1"]}
        compiler = add(compilation/"native.json", receipt)
        runner = {"BUILD": build, "emitted_image": lambda *args: image, "SELECTOR": "compiler",
                  "projection_inputs": lambda pin: (expected, counts), "parent": SimpleNamespace(settled=lambda job: True)}
        base = SimpleNamespace(read=lambda path, *args: index[Path(path)][1], SID="human")

        def exact(path, sha, _):
            pin, _ = index[path]
            self.assertEqual(pin["sha256"], sha)
            return pin

        args = SimpleNamespace(cal_epoch="r7-first56", cal_phase=6, build=build,
                               cal_compiler_sha256=compiler["sha256"], cal_projection_receipt_sha256=projection_receipt["sha256"])
        logfile = "compact-count\nlarge-count\ntest result: ok. 1 passed; 0 failed;\n"
        with patch.object(OWNER["runpy"], "run_path", return_value=runner), patch.object(Path, "read_text", return_value=logfile):
            refs, rows, env = OWNER["inputs"](args, campaign, base, exact, audit=lambda paths: None)
            self.assertIn(closed, refs)
            self.assertEqual([r["ordinal"] for r in rows], list(range(48, 56)))
            self.assertEqual(env, {k: v for k,v in expected.items() if "LARGE" not in k} | {"AIRK_CUDA_CAL_PHASE": "6"})
            receipt["projection"] = projection_receipt | {"sha256": BASE["PLAN5"]}
            with self.assertRaisesRegex(ValueError, "projection or native compiler receipt differs"):
                OWNER["inputs"](args, campaign, base, exact, audit=lambda paths: None)

    def test_native_capture_job_must_settle_naturally(self):
        job = {"exit_code": 0, "child_assigned_before_resume": True, "timed_out": False,
               "log_oversized": False, "owned_job_forced_termination": False,
               "owned_descendants_settled": True, "owned_job_final_active_processes": 0,
               "owned_job_final_member_pids": []}
        self.assertTrue(OWNER["settled_capture"](job))
        for change in ({"owned_job_forced_termination": True}, {"owned_job_final_member_pids": [1776]}):
            with self.subTest(change=change):
                self.assertFalse(OWNER["settled_capture"](job | change))

    def test_parent_preserves_historical_host_caps_and_r7_16_gib(self):
        caps = runpy.run_path(str(HERE/"probe_compact_cutlass.py"))["host_cap"]
        self.assertEqual(caps("cal", "fixed", "r4-prefix64"), 32 << 30)
        for epoch in (*OWNER["LAYOUT"], "r5-first56", "r5-full100"):
            self.assertEqual(caps("cal", "fixed", epoch), 16 << 30)


if __name__ == "__main__":
    unittest.main()
