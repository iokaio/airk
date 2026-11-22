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
    def test_advice_runtimes_and_dispatch_census(self):
        body = {"assessment_history": [
            {"result": {"status": "abstained", "advice": {"profile": "a", "runtime": gf.CUDA_RUNTIME + "x",
                                                          "observed_input_tokens": 900}}},
            {"result": {"status": "abstained", "advice": {"profile": "b", "runtime": "onnxruntime-cpu/1.30.0"}}}]}
        rows = [{"dispatch_identity": "d1", "profile_manifest": "a", "task": "t", "kind": "dispatch_admitted"},
                {"dispatch_identity": "d1", "profile_manifest": "a", "task": "t", "kind": "score_returned",
                 "outcome": "success", "finite_raw_scores": True, "raw_scores": [1.0], "work": {"inference_us": 5}}]
        evidence = gf.device_evidence(body, rows)
        self.assertEqual((evidence["cuda_advice"], evidence["cpu_advice"]), (1, 1))
        self.assertEqual(evidence["dispatch"][0]["kinds"], ["dispatch_admitted", "score_returned"])
        self.assertEqual(evidence["dispatch"][0]["work"], {"inference_us": 5})


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
