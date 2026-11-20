# SPDX-License-Identifier: Apache-2.0
import runpy
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
interval = runpy.run_path(str(ROOT / "scripts/arbiter/analysis/public_source_admission/verify_techqa_delivered_evidence.py"))["returned_interval"]
covered = runpy.run_path(str(ROOT / "tools/retrieval-train/goal_rubric.py"))["covered_location"]


def hit(text, start=1, end=1):
    return {"text": text, "citation": {"start_line": start, "end_line": end,
            "location": {"Lines": {"start": start, "end": end}}}}


class DeliveredEvidence(unittest.TestCase):
    def test_repeated_text_in_same_citation_is_unresolved(self):
        self.assertIsNone(interval(b"aaaa", hit("aaa")))

    def test_truncated_text_does_not_cover_whole_citation(self):
        self.assertEqual(interval(b"abcdef", hit("abc")), (0, 3))

    def test_unicode_lines_use_original_byte_offsets(self):
        self.assertEqual(interval("á\n€!".encode(), hit("€", 2, 2)), (3, 6))

    def test_coverage_can_span_adjacent_delivered_fragments(self):
        def span(start, end):
            return {"file": "original", "byte_start": start, "byte_end": end}
        self.assertTrue(covered(span(2, 6), [span(0, 4), span(4, 9)]))
        self.assertFalse(covered(span(2, 6), [span(0, 4), span(5, 9)]))


class PairedDeliveredEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.owner = runpy.run_path(str(ROOT/"scripts/arbiter/analysis/public_source_admission/score_techqa_answer_pair.py"))

    def test_missing_or_changed_pair_cannot_issue_outcome(self):
        compare = self.owner["compare"]
        def arm(backend,result):
            return {"backend":backend,"precision":"float16" if backend=="cuda" else "float32","manifest":backend,"binding":{"scope":"a","inputs":[["candidate",32]],
                    "budget":{"output_bytes":4000}},"trajectory":{"stop":"covered","inputs":[1]},"evidence":{"documented_set_covered":result}}
        cuda,cpu = arm("cuda",True),arm("cpu",False)
        self.assertTrue(compare(cuda,cpu)["paired_improvement"])
        for change in (dict(cpu,binding={"scope":"b"}),dict(cpu,precision="float16"),dict(cpu,manifest="cuda"),
                       arm("cpu",None)):
            with self.assertRaises(ValueError):
                compare(cuda,change)
        with self.assertRaises(KeyError):
            compare(cuda,{})

    def test_no_forward_timeout_and_divergent_trajectories_remain_paired(self):
        import hashlib
        compare = self.owner["compare"]
        terminal = self.owner["terminal_packet"]
        trajectory = self.owner["trajectories"]
        sha = hashlib.sha256(b"").hexdigest()
        summary = {"records":0,"last_sequence":0}
        scores,attempts = trajectory([],summary,"profile",sha)
        self.assertEqual((scores["work"],attempts),([],[]))
        with self.assertRaises(ValueError):
            trajectory([],dict(summary,records=1),"profile",sha)
        scope = {"publication":{"token":"original"}}
        data = {"assessment":{"scope":scope,"termination":"deadline"},"records":[],"paragraphs":[],
                "scope_observation":{"index":"original"},"assessment_history":[],"stop":"deadline"}
        self.assertEqual(terminal(data),"deadline")
        with self.assertRaises(ValueError):
            terminal(dict(data,assessment_history=None))
        body = b"abcdef"
        target = {"original_path":"one.txt","byte_start":1,"byte_end_exclusive":4,
                  "span_sha256":hashlib.sha256(b"bcd").hexdigest(),"source_sha256":hashlib.sha256(body).hexdigest()}
        members = [{"path":"one.txt","original_sha256":target["source_sha256"]}]
        score = lambda packet:self.owner["coverage"](packet,scope["publication"],members,target,body)
        miss = score(data)
        self.assertFalse(miss["documented_set_covered"])
        h = hit("abcde")
        h["citation"].update(path="one.txt",content_hash=target["source_sha256"])
        packet = dict(data,records=[{"scope":scope,"operation":"search","canonical_result":{
            "scope_observation":data["scope_observation"],"publication":scope["publication"],"hits":[h]}}])
        delivered = score(packet)
        self.assertTrue(delivered["documented_set_covered"])
        h["text"] = "ab"
        self.assertFalse(score(packet)["documented_set_covered"])
        common = {"request":"identical","original_menu":["one.txt"],"scope":scope}
        a = {"manifest":"half","precision":"float16","backend":"cuda","binding":common,"evidence":miss,
             "trajectory":{"stop":"deadline","native_attempts":[]}}
        b = {"manifest":"single","precision":"float32","backend":"cpu","binding":common,"evidence":delivered,
             "trajectory":{"stop":"covered","native_attempts":["initial","later"]}}
        result = compare(a,b)
        self.assertTrue(result["paired_regression"])
        self.assertEqual(result["fp16_trajectory"],a["trajectory"])
        self.assertEqual(result["fp32_trajectory"],b["trajectory"])

    def test_installed_collection_rejects_fixture_before_opening_population(self):
        live = runpy.run_path(str(ROOT/"scripts/arbiter/analysis/public_source_admission/collect_installed_techqa_answer.py"))
        with self.assertRaisesRegex(ValueError,"unfrozen installed issuer"):
            live["collect"]({"native_issuer":"private_service_fixture_through_cli"},"unused","fp16",Path("."))

    def test_installed_command_replay_refuses_fixture_incomplete_or_crossed_calls(self):
        installed = runpy.run_path(str(ROOT/"scripts/arbiter/analysis/public_source_admission/installed_techqa_answer.py"))
        native = Path("installed")/"airkctl.exe"
        command, stdout = Path("commands/a.command.json"), Path("commands/a.stdout")
        argv = [str(native),"--output","json","--service-runtime-dir","control","arbiter","status"]
        original = {"argv":argv,"state":"observed","exit_code":0,"pid":11,"start_ns":5,"end_ns":8,
                    "elapsed_ns":3,"stdout":str(stdout),"stdout_sha256":"digest"}
        def replay(row):
            def read(pin):
                if pin == {"path":str(command)}:
                    return row
                self.assertEqual(pin, {"path":str(stdout),"sha256":"digest"})
                return {"native":"original"}
            return installed["OriginalCommands"]([{"path":str(command)}],native,"control",read)
        actual = replay(original)
        self.assertEqual(actual.call("a",["arbiter","status"]),{"native":"original"})
        with self.assertRaises(ValueError):
            actual.call("a",["arbiter","status"])
        for delta in ({"argv":["fixture.exe",*argv[1:]]},{"state":"timeout"},{"pid":0},{"elapsed_ns":1},
                      {"stdout":str(Path("foreign")/"a.stdout")}):
            with self.subTest(delta=delta), self.assertRaises(ValueError):
                replay(original | delta).call("a",["arbiter","status"])

    def test_initial_search_order_and_terminal_refusal(self):
        identity = self.owner["initial_search_identity"]
        a,b = hit("one"),hit("two")
        self.assertNotEqual(identity({"hits":[a,b]}),identity({"hits":[b,a]}))
        with self.assertRaises(ValueError):
            identity({"hits":[{"text":"unattributed"}]})
        packet = {"stop":"refused","assessment":{"termination":"refused"},"records":[],"assessment_history":[]}
        self.assertEqual(self.owner["terminal_packet"](packet),"refused")

    def test_foreign_citation_and_unresolved_target_are_not_misses(self):
        import hashlib
        digest = hashlib.sha256(b"abcabc").hexdigest()
        pub = {"token":"original"}
        scope = {"publication":pub}
        target = {"byte_start":0,"byte_end_exclusive":3,"original_path":"source.txt","source_sha256":digest,
                  "span_sha256":hashlib.sha256(b"abc").hexdigest()}
        h = hit("abc")
        h["citation"].update(path="source.txt",content_hash=digest)
        result = {"publication":pub,"scope_observation":{"index":"i"},"hits":[h]}
        packet = {"assessment":{"scope":scope},"scope_observation":result["scope_observation"],"paragraphs":[],
                  "records":[{"scope":scope,"operation":"search","canonical_result":result}]}
        members = [{"path":"source.txt","original_sha256":digest}]
        coverage = self.owner["coverage"]
        self.assertIsNone(coverage(packet,pub,members,target,b"abcabc")["documented_set_covered"])
        h["citation"]["content_hash"] = "wrong"
        with self.assertRaises(ValueError):
            coverage(packet,pub,members,target,b"abcabc")


if __name__ == "__main__":
    unittest.main()
