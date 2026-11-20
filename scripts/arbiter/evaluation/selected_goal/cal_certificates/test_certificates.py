# SPDX-License-Identifier: Apache-2.0
"""CAL read gain, independent alternatives, scope binding and refusal controls."""

import json
from pathlib import Path
import sys
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
from capture_closure import exchange, question_events, settled
from certify import certificate, original_gains
from reference_inputs import HERE, scoring_owners


class MemoryLedger:
    def __init__(self):
        self.spans = []

    def observe(self, body):
        self.spans.extend(body["ranges"])

    def visible_candidates(self):
        return self.spans


class CertificateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        _, utility = scoring_owners()
        cls.utility = staticmethod(utility)

    def setUp(self):
        self.pub = {"token": "published", "collection": "cal"}
        self.obs = {"fingerprint": "scope"}
        self.binding = Mock(index_id="idx", publication=self.pub,
                            row={"status_result": {"structuredContent": {"scope_observation": self.obs}}})
        self.scope = {"index_id": "idx", "publication": self.pub}
        self.basis = {"read_scope": {"source": "named", "index_ids": ["idx"]}, "scope_observation": self.obs,
                      "collection": "cal", "publication": self.pub}
        self.task = {"id": "q", "source_component": "group", "question": "Q", "alternatives": [
            {"human_annotation_id": "human-1", "facts": [{"locations": [self.span(0, 10)]}]},
            {"human_annotation_id": "human-2", "facts": [{"locations": [self.span(20, 25)]}]}]}
        self.request = {"scope": self.scope, "question": "Q", "read_attempt": {"request_id": "1"*32}, "ordinal": 1,
                        "remaining_ms": 17, "records": [{"scope": self.scope, "canonical_result":
                            {"publication": self.pub, "ranges": [self.span(0, 5)]}}],
                        "roster": {"basis": "b", "policy": "policy", "cards": [
                            {"id": "whole-a", "action": {"kind": "read", "choice": "a"}},
                            {"id": "stop", "action": {"kind": "sufficient", "choice": "a"}},
                            {"id": "whole-b", "action": {"kind": "read", "choice": "b"}}]}}
        self.audit = {"pipeline": "pipe", "profiles": {"arbiter-focus-qwen3-reranker-06b": "focus",
                                                       "arbiter-context-qwen3-17b": "compact"}}

    @staticmethod
    def span(a, b):
        return {"file": "doc", "byte_start": a, "byte_end": b}

    def probe(self, key, ranges, refused=False):
        return {"probe": {"choice": {"id": key}, "tool": "context", "publication": self.pub,
                          "arguments": {"response_mode": "full", "include_status": True}},
                "result": {"refused": refused, "body": {**self.basis, "ranges": ranges}}}

    def gain(self, probes):
        return original_gains(self.task, self.request, probes, self.binding, MemoryLedger, self.utility)

    def event(self, probes):
        record = {"schema": "airk.private-selected-Context-native-admission-observation/1",
                  "Focus_configuration": "ON", "Context_NN_dispatch": False, "actual_Focus_profile": "focus",
                  "actual_Context_profile": {"manifest": "compact"}, "MiniLM_pipeline": "pipe",
                  "request": self.request, "planned_input_digest": "digest",
                  "verified_probe_requests": [p["probe"] for p in probes], "compiler_census": [],
                  "remaining_ns_at_observation": 12345678}
        body = {**self.basis, "question": "Q", "request_id": "2"*32, "records": self.request["records"].copy(),
                "assessment": {"scope": self.scope}, "assessment_history": [
                    {"ordinal": 1, "basis": "b", "result": {"advice": {"input_digest": "digest",
                     "observed_input_tokens": 0, "work": {"input_tokens": 0}, "outcome": {"status": "unavailable"}}}}]}
        return {"id": "q", "scope": self.basis, "answer": {"refused": False, "body": body},
                "captured": [record], "probes": probes}

    def certify(self, event):
        return certificate(event, self.task, self.audit, self.binding, "policy", MemoryLedger, self.utility)

    def test_certify_complete_missing_fact_from_same_alternative_and_whole_read_card(self):
        row = self.certify(self.event([self.probe("a", [self.span(5, 10)])]))
        self.assertEqual(row["reference_proxies"]["valid_reference_proxy_whole_cards"], [
            {"card": "whole-a", "choice": "a", "proxy": "generic_read_original_reference_complement",
             "necessity_or_optimality_claim": False}])
        progress, = row["reference_proxies"]["counterfactual_reference_progress"]
        self.assertEqual([r["after_facts"] for r in progress["original_human_reference_utility"]["alternatives"]], [1, 0])
        self.assertEqual(row["remaining_ms_at_observation"], 12)
        self.assertFalse(row["human_semantic_gold"])

    def test_counterfactuals_never_pool_complementary_partial_ranges(self):
        certs, progress, _ = self.gain([self.probe("a", [self.span(5, 7)]), self.probe("b", [self.span(7, 10)])])
        self.assertEqual(certs, [])
        self.assertEqual([p["classification"] for p in progress], ["UNKNOWN", "UNKNOWN"])
        self.assertFalse(any(p["semantic_negative"] for p in progress))

    def test_refused_probe_and_undelivered_record_stay_unknown(self):
        certs, progress, unknown = self.gain([self.probe("a", [], True)])
        self.assertEqual((certs, progress, unknown[0]["classification"]), ([], [], "UNKNOWN"))
        event = self.event([])
        event["answer"] = {"refused": True, "body": {}}
        row = self.certify(event)
        self.assertIsNone(row["request"])
        self.assertEqual(row["reference_proxies"]["undelivered_observations_are_not_scored"][0]["association"],
                         "UNDELIVERED_NOT_A_REQUEST_OR_LABEL")

    def test_equal_ids_cannot_fix_foreign_records_and_digest_or_duplicate_basis(self):
        event = self.event([])
        body = event["answer"]["body"]
        body["request_id"] = self.request["read_attempt"]["request_id"]
        body["records"] = []
        with self.assertRaisesRegex(ValueError, "association"):
            self.certify(event)
        body["records"] = self.request["records"]
        body["assessment_history"] *= 2
        with self.assertRaisesRegex(ValueError, "basis"):
            self.certify(event)

    def test_foreign_probe_publication_and_source_scope_refused(self):
        p = self.probe("a", [self.span(5, 10)])
        p["result"]["body"]["publication"] = {"token": "different"}
        with self.assertRaisesRegex(ValueError, "publication"):
            self.gain([p])
        self.request["records"][0]["scope"] = {}
        with self.assertRaisesRegex(ValueError, "scope"):
            self.gain([])

    def test_all_13_phase_transcripts_are_required_with_original_exchange_order(self):
        ids = [str(i) for i in range(100)]
        root = Path("/nonexistent-unused-fixture")
        closure = {"instance": "service", "live_request": {"sha256": "request"}}
        census = {"phase_closures": list(range(13)), "probes": 0, "request_sha256": "request"}
        template = {"budget": "pinned"}
        tasks = {i: {"question": i} for i in ids}
        sources = [HERE.parent / "cal_capture" / n for n in ("worker.cjs","phase.cjs","controller.py","prepare.py")]
        sources.append(HERE.parents[3] / "provision-static-onnx-runtime.py")
        def owner(phase, path):
            return {"exit_code": 0, "timed_out": False, "log_oversized": False, "child_assigned_before_resume": True,
                    "owned_descendants_settled": True, "owned_job_forced_termination": False,
                    "owned_job_final_active_processes": 0, "owned_job_final_member_pids": [],
                    "phase": phase, "request_sha256": "request", "sources": [{"path": str(p)} for p in sources],
                    "transcript": {"path": str(root / "phases" / f"phase-{phase:02}.jsonl")}}
        def call(tool, args, body):
            return {"kind": "native_exchange", "argv": ["tools","call",tool,"--index","idx","--args",
                    json.dumps(args),"--output","json"], "result": {"exitCode": 0, "class": "success",
                    "stdout": json.dumps({"structuredContent": body})}}
        reject = False
        def transcript(ref):
            path = Path(ref["path"])
            if path.suffix != ".jsonl":
                return b"source already verified"
            phase = int(path.stem[-2:])
            chunk = ids[8*phase:8*(phase+1)]
            rows = []
            for key in chunk:
                response = {"question": key}
                rows.extend([call("status", {"response_mode":"full", "include_status":True}, self.basis),
                             call("answer", dict(template, question=key), response),
                             {"kind":"cal_question", "evidence":{"id":key,"scope":self.basis,"probes":[],
                               "captured":[],"answer":{"refused":False,"body":response}}}])
            if not (reject and phase == 12):
                rows.append({"kind":"phase_complete","phase":phase,"ids":chunk,"instance":"service","publication":self.pub})
            return b"\n".join(json.dumps(r).encode() for r in rows)
        with patch("capture_closure.read", side_effect=owner), patch("capture_closure.checked", side_effect=transcript):
            args = root, closure, census, ids, self.binding, template, tasks
            self.assertEqual([r["id"] for r in question_events(*args)], ids)
            reject = True
            with self.assertRaisesRegex(ValueError, "missing close"):
                list(question_events(*args))

    def test_unavailable_answer_or_failed_job_cannot_complete_a_capture(self):
        args = {"foo": "bar"}
        frame = {"argv": ["tools", "call", "context", "--index", "idx", "--args", json.dumps(args), "--output","json"],
                 "result": {"class": "refused", "exitCode": 1, "stdout": json.dumps({"isError": True,
                 "structuredContent": {"error": {"code": "verification_reply_budget", "retryable": False,
                                                 "verification_performed": False}}})}}
        self.assertTrue(exchange(frame, "context", args, "idx")["refused"])
        frame["result"]["class"] = "busy"
        with self.assertRaisesRegex(ValueError, "missing or failed"):
            exchange(frame, "context", args, "idx")
        self.assertFalse(settled({"exit_code": 1}))
        self.assertFalse(settled({"exit_code": 0, "timed_out": False, "log_oversized": False,
                                 "child_assigned_before_resume": True, "owned_descendants_settled": True,
                                 "owned_job_forced_termination": True}))


if __name__ == "__main__":
    unittest.main()
