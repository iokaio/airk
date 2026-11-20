# SPDX-License-Identifier: Apache-2.0
"""CAL read gain, independent alternatives, scope binding and refusal controls."""

import json
from pathlib import Path
import sys
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
from capture_closure import exchange, observed_searches, question_events, settled, window_binding
from certify import certificate, original_gains
from reference_inputs import AUDIT, CAMPAIGN, HERE, load_inputs, reference, scoring_owners


class MemoryLedger:
    def __init__(self):
        self.spans = []

    def observe(self, body):
        self.spans.extend(body["ranges"])

    def visible_candidates(self):
        return self.spans


class GroupedClockTests(unittest.TestCase):
    def test_grouped_schema_registers_exact_counts_and_partitions(self):
        from grouped_closure import PLAN, TWO_PLAN, FIXTURE, TWO_FIXTURE, PLAN_FOR_FIXTURE, shape
        self.assertEqual(PLAN_FOR_FIXTURE, {FIXTURE: PLAN, TWO_FIXTURE: TWO_PLAN})
        self.assertEqual(shape(TWO_PLAN), (tuple(range(7)),tuple(range(7,13))))
        self.assertEqual(shape(PLAN), (tuple(range(4)),tuple(range(4,8)),tuple(range(8,13))))
        with self.assertRaises(ValueError):
            shape("unregistered")

    def test_first_phase_pays_for_startup_and_final_phase_pays_for_shutdown(self):
        from grouped_closure import check_clocks
        lifetime = {"start_ms":10,"end_ms":200}
        rows = [{"start_ms":10,"end_ms":90,"deadline_ms":201},
                {"start_ms":95,"end_ms":195,"deadline_ms":201}]
        check_clocks(lifetime,rows,201)
        for i,field,value in ((0,"start_ms",11),(1,"start_ms",89),(1,"deadline_ms",202),(0,"end_ms",210)):
            changed = [dict(row) for row in rows]
            changed[i][field] = value
            with self.subTest(field=field,value=value), self.assertRaises(ValueError):
                check_clocks(lifetime,changed,201)
        with self.assertRaises(ValueError):
            check_clocks(dict(lifetime,end_ms=202),rows,201)

    def test_final_group_mutex_matches_owner_time_and_original_outer_receipt(self):
        import grouped_closure
        import reference_inputs
        root = Path("example")
        acquired = {"name":r"Local\AIRK-20261116-Focus1024-Reuse","owner_pid":7,"at_ms":2}
        plan = {"installation_mutex":{"path":"acquired","sha256":"a"*64},"start_ms":3}
        def reference(path,digest):
            return {"path":str(path),"sha256":digest}
        outer = reference(root/"outer-owned.json","c"*64)
        released = dict(acquired,at_ms=9,acquired=plan["installation_mutex"],outer=outer)
        with patch.object(reference_inputs,"reference",side_effect=reference):
            for changed,valid in (({},True),({"owner_pid":8},False),({"at_ms":7},False),
                                  ({"outer":{}},False),({"name":"foreign"},False)):
                with self.subTest(changed=changed), patch.object(grouped_closure,"read",
                        side_effect=[acquired,dict(released,**changed)]):
                    if valid:
                        grouped_closure.check_mutex(root,plan,"c"*64,"b"*64,8)
                    else:
                        with self.assertRaises(ValueError):
                            grouped_closure.check_mutex(root,plan,"c"*64,"b"*64,8)

    def test_long_group_keeps_individual_phase_ceiling(self):
        from grouped_closure import check_clocks
        rows = [{"start_ms":0,"end_ms":1600000,"deadline_ms":1800000},
                {"start_ms":1600010,"end_ms":3200000,"deadline_ms":3400010}]
        lifetime = {"start_ms":0,"end_ms":3200010}
        check_clocks(lifetime,rows,5000000)
        rows[0]["deadline_ms"] += 1
        with self.assertRaises(ValueError):
            check_clocks(lifetime,rows,5000000)


class PhasedClockTests(unittest.TestCase):
    def test_original_phase_and_whole_wall_limits(self):
        from phased_closure import phase_interval
        phase_interval(1000,1801000,1000,1801000)
        for arguments in ((999,1001,1000,1801000),(1000,1801001,1000,1801001),
                          (1000,1002,1000,1001),(1000,1000,1000,1001)):
            with self.assertRaises(ValueError):
                phase_interval(*arguments)


class CertificateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        _, utility = scoring_owners()
        cls.utility = staticmethod(utility)

    def setUp(self):
        self.pub = {"token": "published", "collection": "cal"}
        self.obs = {"fingerprint": "scope"}
        self.binding = Mock(index_id="idx", publication=self.pub, observation=self.obs, namespace="catalog",
                            row={"status_result": {"structuredContent": {"scope_observation": self.obs}}})
        self.scope = {"index_id": "idx", "publication": self.pub}
        self.basis = {"scope_observation": self.obs,
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
        census = {"phase_closures": list(range(13)), "probes": 0, "pipeline_searches": 100, "request_sha256": "request"}
        template = {"budget": "pinned"}
        tasks = {i: {"question": i} for i in ids}
        selection={"expected_catalog_revision":4,"index_ids":["idx"],"namespace":"catalog","project_ids":["project"]}
        self.binding.row["window_scope_selection"]=selection
        member={"index_id":"idx","source_root_ids":["root"],"source_revision":2,
                "policy_epoch":0,"policy_digest":None,"memberships":[]}
        original={"selection":{"owner":"agent","catalog_revision":4,"scope_identity":"a"*64},
                  "qualifier":"selected_subset","schema":1,"members":[member],"fingerprint":"f"*64}
        self.binding.row["status_result"]["structuredContent"]["scope_observation"]=original
        def status(i):
            obs=dict(original,fingerprint=f"{int(i)+1:064x}",selection={"owner":"window","catalog_revision":4,
                "scope_identity":"a"*64},members=[dict(member,memberships=[{"project_id":"project","membership_revision":1}])])
            return dict(self.basis, scope_observation=obs,files=753,indexed_vectors=767,semantic_readiness={"state":"ready"})
        def grant(i):
            return {"kind":"window_grant","question_id":i,"selection":selection,"result":{"status":"scope",
                    "scope_identity":"a"*64,"catalog_revision":4,"expires_at_ms":987654321}}
        fingerprints=set()
        window_binding(self.binding,grant("0"),status("0"),fingerprints)
        with self.assertRaisesRegex(ValueError,"reused"):
            window_binding(self.binding,grant("1"),status("0"),fingerprints)
        sources = [HERE.parent / "cal_capture" / n for n in ("worker.cjs","phase.cjs","window_scope.cjs","controller.py","prepare.py")]
        sources.append(HERE.parents[3] / "provision-static-onnx-runtime.py")
        def owner(phase, path):
            return {"exit_code": 0, "timed_out": False, "log_oversized": False, "child_assigned_before_resume": True,
                    "owned_descendants_settled": True, "owned_job_forced_termination": False,
                    "owned_job_final_active_processes": 0, "owned_job_final_member_pids": [],
                    "phase": phase, "request_sha256": "request", "sources": [{"path": str(p)} for p in sources],
                    "transcript": {"path": str(root / "phases" / f"phase-{phase:02}.jsonl")}}
        def call(tool, args, body):
            return {"kind": "native_exchange", "argv": ["tools","call",tool,"--args",
                    json.dumps(args),"--output","json"], "result": {"exitCode": 0, "class": "success",
                    "stdout": json.dumps({"structuredContent": body})}}
        reject = missing_release = False
        def transcript(ref):
            path = Path(ref["path"])
            if path.suffix != ".jsonl":
                return b"source already verified"
            phase = int(path.stem[-2:])
            chunk = ids[8*phase:8*(phase+1)]
            rows = []
            for key in chunk:
                response = {"question": key, "records": [{"operation": "search", "canonical_result": {
                    "publication": self.pub, "semantic_readiness": {"state": "ready", "pipeline_digest": "pipeline"},
                    "execution": {"served_mode": "hybrid", "incomplete": False}}}]}
                rows.extend([grant(key),call("status", {"response_mode":"full", "include_status":True}, status(key)),
                             call("answer", dict(template, question=key), response),
                             {"kind":"cal_question", "evidence":{"id":key,"scope":status(key),"probes":[],
                               "captured":[],"pipeline_searches":1,"answer":{"refused":False,"body":response}}},
                             {"kind":"window_release","question_id":key,"result":{"status":"scope_released"}}])
            if missing_release and phase == 12:
                rows.pop()
            if not (reject and phase == 12):
                rows.append({"kind":"phase_complete","phase":phase,"ids":chunk,"instance":"service","publication":self.pub})
            return b"\n".join(json.dumps(r).encode() for r in rows)
        with patch("capture_closure.read", side_effect=owner), patch("capture_closure.checked", side_effect=transcript):
            args = root, closure, census, ids, self.binding, template, tasks, "pipeline"
            self.assertEqual([r["id"] for r, _ in question_events(*args)], ids)
            missing_release = True
            with self.assertRaisesRegex(ValueError, "phase close"):
                list(question_events(*args))
            missing_release = False
            reject = True
            with self.assertRaisesRegex(ValueError, "missing close"):
                list(question_events(*args))

    def test_original_512_audit_cannot_issue_active_certificates(self):
        legacy = reference(CAMPAIGN / "cal100-launch-input-audit-r1.json", AUDIT)
        with self.assertRaisesRegex(ValueError, "fresh active"):
            load_inputs(legacy, "unrelated")

    def test_native_window_exchange_refuses_a_named_index_selector(self):
        event = {"argv":["tools","call","status","--index","idx","--args","{}","--output","json"]}
        with self.assertRaisesRegex(ValueError,"canonical call arguments"):
            exchange(event,"status",{})

    def test_delivered_probe_requires_original_semantic_pipeline(self):
        result = {"publication": self.pub, "semantic_readiness": {"state": "ready", "pipeline_digest": "original"},
                  "execution": {"served_mode": "vector", "incomplete": False}}
        event = {"answer": {"refused": False, "body": {"records": []}}, "pipeline_searches": 1,
                 "probes": [{"probe": {"tool": "search"}, "result": {"refused": False, "body": result}}]}
        self.assertEqual(observed_searches(event, self.pub, "original"), 1)
        with self.assertRaisesRegex(ValueError, "pipeline"):
            observed_searches(event, self.pub, "different")
        result["execution"]["incomplete"] = True
        with self.assertRaisesRegex(ValueError, "pipeline"):
            observed_searches(event, self.pub, "original")

    def test_unavailable_answer_or_failed_job_cannot_complete_a_capture(self):
        args = {"foo": "bar"}
        frame = {"argv": ["tools", "call", "context", "--args", json.dumps(args), "--output","json"],
                 "result": {"class": "refused", "exitCode": 1, "stdout": json.dumps({"isError": True,
                 "structuredContent": {"error": {"code": "verification_reply_budget", "retryable": False,
                                                 "verification_performed": False}}})}}
        self.assertTrue(exchange(frame, "context", args)["refused"])
        frame["result"]["class"] = "busy"
        with self.assertRaisesRegex(ValueError, "missing or failed"):
            exchange(frame, "context", args)
        self.assertFalse(settled({"exit_code": 1}))
        self.assertFalse(settled({"exit_code": 0, "timed_out": False, "log_oversized": False,
                                 "child_assigned_before_resume": True, "owned_descendants_settled": True,
                                 "owned_job_forced_termination": True}))


if __name__ == "__main__":
    unittest.main()
