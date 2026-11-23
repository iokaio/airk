# SPDX-License-Identifier: Apache-2.0
"""Synthetic tests for gpu_fixture.py; no service, GPU or private input is used."""

import hashlib
import json
import tempfile
import unittest
from pathlib import Path

import gpu_fixture as gf


def write(path, value):
    data = (json.dumps(value) + "\n").encode()
    path.write_bytes(data)
    return hashlib.sha256(data).hexdigest()


class CudaEnvironmentTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        root = Path(self.temp.name)
        self.saved = dict(gf.MODELS["compact"])
        roster_sha = write(root / "roster.json", [{"profile": "p"}])
        graph_sha = write(root / "graph.json", {"adapter_before": {"uuid": list(range(1, 17))}})
        gf.MODELS["compact"].update(roster=root / "roster.json", roster_sha256=roster_sha,
                                    graph=root / "graph.json", graph_sha256=graph_sha)
        self.root = root

    def tearDown(self):
        gf.MODELS["compact"].clear()
        gf.MODELS["compact"].update(self.saved)
        self.temp.cleanup()

    def test_pins_and_adapter_reach_the_service_environment(self):
        env = gf.cuda_environment("compact", "focus")
        self.assertEqual(env["AIRK_ARBITER_EVALUATION_CUDA_ADAPTER_UUID"], json.dumps(list(range(1, 17))))
        self.assertEqual(env["AIRK_ARBITER_EVALUATION_CUDA_SEED_CPU"], "focus")
        self.assertEqual(env["AIRK_ARBITER_EVALUATION_CUDA_INPUTS_SHA256"], gf.MODELS["compact"]["roster_sha256"])
        # A restart passes no seed, so the variable is absent.
        self.assertNotIn("AIRK_ARBITER_EVALUATION_CUDA_SEED_CPU", gf.cuda_environment("compact"))

    def test_changed_pins_unknown_seed_and_missing_adapter_refuse(self):
        with self.assertRaises(ValueError):
            gf.cuda_environment("compact", "some")
        (self.root / "roster.json").write_text("[]\n")
        with self.assertRaises(ValueError):
            gf.cuda_environment("compact")
        gf.MODELS["compact"]["roster_sha256"] = hashlib.sha256(b"[]\n").hexdigest()
        gf.MODELS["compact"]["graph_sha256"] = write(self.root / "graph.json", {"adapter_before": {"uuid": [0] * 16}})
        with self.assertRaises(ValueError):
            gf.cuda_environment("compact")


class MathEnvironmentTests(unittest.TestCase):
    # The FP16 routes check G5 against the environment captured before ORT starts: GEMM option 2, the
    # product's default (s5 shakedown, 23 November 2026: the warm failed without it).
    def test_the_service_gets_gemm2_and_keeps_debug_unset(self):
        self.assertEqual(gf.math_environment({}), {"ORT_CUDA_GEMM_OPTIONS": "2"})
        self.assertEqual(gf.math_environment({"ORT_CUDA_GEMM_OPTIONS": "2", "PATH": "x"}),
                         {"ORT_CUDA_GEMM_OPTIONS": "2"})
        self.assertEqual(gf.math_environment({"ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO": "0"}),
                         {"ORT_CUDA_GEMM_OPTIONS": "2"})

    def test_conflicting_or_unknown_ort_values_refuse(self):
        for inherited in ({"ORT_CUDA_GEMM_OPTIONS": "0"}, {"ORT_CUDA_GEMM_OPTIONS": ""},
                          {"ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO": "1"},
                          {"ORT_GQA_DISABLE_FLASH_ATTENTION": "1"}):
            with self.assertRaises(ValueError, msg=inherited):
                gf.math_environment(inherited)

    def test_problems_name_a_handle_without_gemm2_or_top_score(self):
        good = {"ORT_CUDA_GEMM_OPTIONS": "2", "AIRK_ARBITER_EVALUATION_DECISION_MODE": "top_score",
                "AIRK_ARBITER_EVALUATION_IDLE_UNLOAD_SECONDS": "0"}
        self.assertEqual(gf.math_problems(good), [])
        self.assertEqual(len(gf.math_problems({"TEMP": "t"})), 3)
        self.assertEqual(len(gf.math_problems(dict(good, AIRK_ARBITER_EVALUATION_IDLE_UNLOAD_SECONDS="1800"))), 1)
        self.assertEqual(len(gf.math_problems(dict(good, AIRK_ARBITER_EVALUATION_DECISION_MODE="calibrated"))), 1)


class ChangeSetEnvironmentTests(unittest.TestCase):
    def test_change_set_cuda_variables_are_pinned_and_seeding_is_fresh_only(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            saved = dict(gf.CS1_BASES["compact"])
            gf.CS1_BASES["compact"].update(path=root / "base.json", sha256=write(root / "base.json", {"m": 1}))
            try:
                class Args:
                    model, layers, restart_from, seed_cpu = "compact", "all", None, "none"
                args = Args()
                args.change_set_inputs = str(root / "cs.json")
                args.change_set_inputs_sha256 = write(root / "cs.json", [])
                args.cuda_variant_inputs = str(root / "variant.json")
                args.cuda_variant_inputs_sha256 = write(root / "variant.json", [{}])
                env = gf.cs1_environment(args)
                self.assertEqual(env["AIRK_ARBITER_EVALUATION_CHANGE_SET_SEED_CPU"], "none")
                self.assertEqual(env["AIRK_ARBITER_EVALUATION_CUDA_INPUTS_SHA256"], args.cuda_variant_inputs_sha256)
                self.assertEqual(env["AIRK_ARBITER_EVALUATION_ANSWER_LAYERS"], "all")
                args.restart_from = "L9"
                self.assertNotIn("AIRK_ARBITER_EVALUATION_CHANGE_SET_SEED_CPU", gf.cs1_environment(args))
                args.cuda_variant_inputs_sha256 = "0" * 64
                with self.assertRaises(ValueError):
                    gf.cs1_environment(args)
            finally:
                gf.CS1_BASES["compact"].clear()
                gf.CS1_BASES["compact"].update(saved)


class ProfileTableTests(unittest.TestCase):
    def test_cuda_rows_join_the_cpu_rows_with_their_own_manifests(self):
        table = gf.profiles()
        self.assertEqual(table["context-compact-cuda"]["manifest"], gf.MODELS["compact"]["manifest"])
        self.assertEqual(table["context-large-cuda"]["manifest"], gf.MODELS["large"]["manifest"])
        self.assertEqual({table[name]["stage"] for name in gf.CUDA_PROFILES}, {"context"})
        self.assertIn("focus-static1024-cpu", table)
        self.assertNotIn("focus-static1024-cuda", table)  # no CUDA Focus selector exists


class EmitterEnvironmentTests(unittest.TestCase):
    def test_declaration_names_only_roots_outside_artifacts(self):
        handle = {"service_environment": {"AIRK_MCP_RESPONSE_MODE": "full"}}
        scopes = {"path": "s", "sha256": "0" * 64}
        inside = str(gf.FX.ROOT / "artifacts" / "x" / "synthetic")
        outside = r"D:\code\Github\bookbuilder"
        env = gf.emitter_environment(handle, scopes, "context", True, "m", "p", [inside])
        self.assertNotIn("AIRK_ARBITER_EVALUATION_EXTERNAL_SOURCES", env)
        env = gf.emitter_environment(handle, scopes, "context", True, "m", "p", [inside, outside])
        self.assertEqual(json.loads(env["AIRK_ARBITER_EVALUATION_EXTERNAL_SOURCES"])["source_roots"], [outside])


class DeviceEvidenceTests(unittest.TestCase):
    def test_loop_decisions_and_dispatch_census(self):
        """The reply is Answer /3: who took each decision is in ``decisions`` and the log's counts, not in the
        one-turn ``assessment_history``. The body is the Rust-emitted two-part case."""
        cases = json.loads((Path(gf.__file__).resolve().parents[2] / "fixtures" / "split" / "arbiter"
                            / "answer_loop_cases.json").read_text(encoding="utf-8"))
        body = next(c["response"] for c in cases["response_cases"] if c["id"] == "two_parts_valid")
        rows = [{"dispatch_identity": "d1", "profile_manifest": "a", "task": "t", "kind": "dispatch_admitted"},
                {"dispatch_identity": "d1", "profile_manifest": "a", "task": "t", "kind": "score_returned",
                 "outcome": "success", "finite_raw_scores": True, "raw_scores": [1.0], "work": {"inference_us": 5}}]
        evidence = gf.device_evidence(body, rows)
        self.assertEqual([(d["stage"], d["source"], d["model"]) for d in evidence["decisions"]],
                         [("split", "deterministic", None), ("coverage", "model", "context_compact"),
                          ("coverage", "abstained", "context_compact"), ("follow_up", "deterministic", None)])
        taken = [d for d in body["decisions"] if d["source"]["kind"] == "model"]
        self.assertEqual(evidence["cuda_decisions"] + evidence["cpu_decisions"], len(taken))
        self.assertEqual(evidence["cuda_decisions"], sum(1 for d in taken if d.get("device") == "cuda"))
        self.assertEqual(evidence["decision_counts"], body["decision_log"]["counts"])
        self.assertEqual(evidence["dispatch"][0]["kinds"], ["dispatch_admitted", "score_returned"])
        self.assertEqual(evidence["dispatch"][0]["work"], {"inference_us": 5})
        # A one-turn body shows no loop decision; nothing is read from its assessment history.
        old = gf.device_evidence({"assessment_history": [{"result": {"advice": {"runtime": gf.CUDA_RUNTIME}}}]}, [])
        self.assertEqual((old["decisions"], old["cuda_decisions"], old["cpu_decisions"]), ([], 0, 0))


class GateRootTests(unittest.TestCase):
    def test_only_the_gates_lifetime_may_spend_a_gate_roots_cuda_lifetime(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder) / "fixture-06-gate-lg-C"
            root.mkdir()
            base = "06-gate-lg-C-base"
            self.assertIsNone(gf.gate_root_problem(root, "06-gate-lg-C", base))
            # A spare root serves another arm under that arm's name.
            self.assertIsNone(gf.gate_root_problem(root, "06-gate-lg-L", base))
            for name in ("s1-shakedown", "trial-pair", "06-gate-lg-C-2", "06-gate-lg-C-base"):
                self.assertIn("is a gate root", gf.gate_root_problem(root, name, base))
            self.assertIn("restarts from the root's closed base", gf.gate_root_problem(root, "06-gate-lg-C", None))
            # Once a CUDA lifetime folder exists, the root cannot restart again, whatever the name.
            (root / "06-gate-lg-C").mkdir()
            (root / "06-gate-lg-C" / "gpu.json").write_text("{}", encoding="utf-8")
            self.assertIn("already had a CUDA lifetime (06-gate-lg-C)", gf.gate_root_problem(root, "06-gate-lg-L", base))
            # Any other root is free: a shakedown or trial root, and a fresh (seeded) lifetime in it.
            other = Path(folder) / "fixture-s1-shakedown"
            other.mkdir()
            self.assertIsNone(gf.gate_root_problem(other, "s3-shakedown", "s3-shakedown-base"))
            self.assertIsNone(gf.gate_root_problem(other, "anything", None))
            (other / "s3-shakedown").mkdir()
            (other / "s3-shakedown" / "gpu.json").write_text("{}", encoding="utf-8")
            self.assertIn("already had a CUDA lifetime", gf.gate_root_problem(other, "again", "s3-shakedown-base"))


class CompareTests(unittest.TestCase):
    def test_equal_and_different_evidence(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            write(root / "request.json", {"question": "q"})
            paths = []
            for label, digest in (("cpu", "d"), ("gpu", "d"), ("other", "e")):
                path = root / f"{label}.json"
                write(path, {"label": label, "is_error": False, "evidence_digest": digest, "elapsed_ms": 1.0,
                             "request": {"path": str(root / "request.json")}})
                paths.append(path)

            class Args:
                pass
            args = Args()
            args.records, args.output = paths[:2], root / "same.json"
            gf.compare(args)
            self.assertTrue(json.loads((root / "same.json").read_text())["equal_evidence"])
            args.records, args.output = [paths[0], paths[2]], root / "different.json"
            gf.compare(args)
            self.assertFalse(json.loads((root / "different.json").read_text())["equal_evidence"])


if __name__ == "__main__":
    unittest.main()
