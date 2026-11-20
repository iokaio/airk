# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch


grader = runpy.run_path(str(Path(__file__).with_name("train_delivered_evidence.py")))
delivered = grader["delivered"]


class ReturnedTrainSupport(unittest.TestCase):
    body = b"foo\nbart\n"
    document = {"sha256": "pinned"}
    goal = {"file": "original.txt", "byte_start": 2, "byte_end": 6}

    def result(self, snippets, publication="original", extra=False):
        hits = [{"text": snippet, "citation": {"path": "original.txt", "content_hash": "pinned",
                 "start_line": 1, "end_line": 2, "location": {"Lines": {"start": 1, "end": 2}}}}
                for snippet in snippets]
        records = [{"operation": "search", "canonical_result": {"publication": publication, "hits": hits}}]
        if extra:
            records.append({"operation": "other", "canonical_result": {}})
        return delivered({"records": records, "paragraphs": []}, "original", self.body, self.document, self.goal)[0]

    def test_adjacent_returns_cover_required_publisher_bytes(self):
        self.assertEqual(self.result(["foo\n", "ba"]), "covered")

    def test_positive_document_alone_cannot_count_as_complete(self):
        self.assertEqual(self.result(["foo"]), "incomplete")

    def test_unhandled_evidence_stays_unresolved(self):
        self.assertEqual(self.result(["foo"], extra=True), "unresolved")

    def test_ambiguous_literal_does_not_cover_either_occurrence(self):
        self.body = b"foo\nfoo\n"
        self.goal = {"file": "original.txt", "byte_start": 0, "byte_end": 3}
        self.assertEqual(self.result(["foo"]), "unresolved")

    def test_swapped_scope_or_attempt_command_refuses(self):
        run = Path("fixture-attempt")
        request = Path("question/request.json")
        publication = {"token": "p", "revision": "r"}
        scope = {"source": "named", "index_ids": ["i"]}
        status = {"isError": False, "structuredContent": {"read_scope": scope, "publication": {"token": "p"},
                                                        "scope_observation": {"fingerprint": "a"}}}
        command = {"exit_code": 0, "argv": ["cli", "--output", "json", "--service-runtime-dir", str(run / "fixture/control"),
                                           "tools", "call", "airk_answer", "--index", "i", "--args-file", str(request)]}
        answer = {"read_scope": scope, "scope_observation": {"fingerprint": "a"},
                  "assessment": {"scope": {"publication": publication}}}
        verify = grader["verify_delivery_scope"]
        control = run / "fixture/control"
        self.assertEqual(verify(control, request, status, command, publication, answer), "i")
        with self.assertRaises(ValueError):
            verify(control, request, status, command, publication, answer | {"scope_observation": {"fingerprint": "b"}})
        with self.assertRaises(ValueError):
            verify(Path("other-attempt"), request, status, command, publication, answer)

    def test_shared_receipt_control_reaches_answer_consumer(self):
        verify = grader["collect"]["verified_control"]
        binding = grader["collect"]["inputs"]
        root = Path("campaign")
        run = root / "compact-development-TRAIN_Q564-focus1024-run-r3"
        fixed = root / "cal100-focus1024-capture-r3/fixture/control"
        r4 = root / "cal100-focus1024-capture-r4"
        plan_sha = "9259b4be9e340ac8e050060e904f53acffee27e3807eca6145a9a8f3be3e299d"
        pool = {}
        def add(path, obj, seal=None):
            seal = seal or str(len(pool))
            pool[(path, seal)] = obj
            return {"path": str(path), "sha256": seal}
        cfg = {"revision": 2}
        shared = {"control_root": str(fixed), "anchor_plan_sha256": plan_sha}
        health = {"executable_sha256": "service", "instance": "a"}
        ready = {"profiles": ["focus1024", "context"], "inputs_sha256": "roster", "health": health,
                 "control_root": str(fixed)}
        original = {"ready": add(fixed.parent / "ready.json", ready),
                    "closed": add(fixed.parent / "closed.json", {}),
                    "status": add(fixed.parents[1] / "commands/cal-ready.stdout", {"configuration": cfg})}
        add(r4 / "lifetime-plan.json", {"installation": original}, plan_sha)
        shared["previous"] = add(r4 / "outer-owned.json", {"owned_descendants_settled": True,
                                 "owned_job_final_active_processes": 0, "owned_job_final_member_pids": []})
        shared["ready"] = add(run / "fixture/ready.json", ready | {"health": health | {"instance": "b"}})
        shared["closed"] = add(run / "fixture/closed.json", {"instance": "b", "control_root": str(fixed),
                               "shutdown_completed": True, "service_lease_reacquired": True})
        saved = {"configuration": cfg, "control": str(fixed)}
        shared["saved"] = add(run / "fixture/restart-owner.json", saved)
        shared["restart_request"] = add(run / "restart-request.json", dict(original))
        shared["status"] = add(run / "commands/reuse-final-status.stdout",
                               {k: {"queued": 0, "running": 0} for k in ("focus", "context")})
        request = Path("request")
        cmd = {"exit_code": 0, "argv": ["cli", "--output", "json", "--service-runtime-dir", str(fixed),
               "tools", "call", "airk_answer", "--index", "i", "--args-file", str(request)]}
        shared["answer_command"] = add(run / "commands/answer.command.json", cmd)
        own = add(run / "owned.json", {"shared_installation": shared})
        obs = {"fixture_instance": "b", "case_id": "TRAIN_Q564"}
        with patch.dict(binding, {"CAMPAIGN": root, "load": lambda path, seal: pool[path, seal]}):
            control = verify(run, own["sha256"], obs)
            publication = {"token": "p"}
            scope = {"read_scope": {"index_ids": ["i"]}, "publication": publication, "scope_observation": {}}
            answer = {"assessment": {"scope": {"publication": publication}}, "read_scope": scope["read_scope"],
                      "scope_observation": {}}
            verify_scope = grader["verify_delivery_scope"]
            self.assertEqual(verify_scope(control, request, {"isError": False, "structuredContent": scope},
                                          cmd, publication, answer), "i")
            with patch.dict(saved, {"control": str(run / "foreign")}):
                with self.assertRaisesRegex(ValueError, "native shared control"):
                    verify(run, own["sha256"], obs)
            with patch.dict(shared, {"control_root": str(run / "alias")}):
                with self.assertRaisesRegex(ValueError, "installation anchor"):
                    verify(run, own["sha256"], obs)
            with patch.dict(shared["answer_command"], {"path": str(root / "other-command.json")}):
                with self.assertRaisesRegex(ValueError, "foreign shared-control evidence"):
                    verify(run, own["sha256"], obs)
            with patch.dict(cmd, {"argv": cmd["argv"][:4] + [str(run / "fixture/control")] + cmd["argv"][5:]}):
                with self.assertRaisesRegex(ValueError, "native shared control"):
                    verify(run, own["sha256"], obs)

    def test_wrong_publication_refuses(self):
        with self.assertRaisesRegex(ValueError, "publication differs"):
            self.result(["foo\nbart"], publication="other")


if __name__ == "__main__":
    unittest.main()
