# SPDX-License-Identifier: Apache-2.0
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from readout import compare_menus, measure, sha


def fixture():
    transcript, journal = [], []
    ids = list("abcd")
    for n, q in enumerate(ids):
        transcript.append(dict(kind="window_grant", question_id=q, result={"scope_identity": q}))
        menu = ["context","search","search","search","search"] if n < 3 else ["context"]
        for k, tool in enumerate(["status", "answer", *menu]):
            t = (7*n+k)*20
            argv = ["tools", "call", tool, "--args", json.dumps({"k":k}), "--output","json"]
            result = dict(stdout='{"structuredContent":{"ok":true}}', stderr="", exitCode=0, **{"class": "success"})
            transcript.append(dict(kind="timing_exchange", ordinal=96+n, question_id=q, scope=q,
                arm=["full","local4","local4","full"][n], pass_index=0, cli_pid=11, service_pid=12,
                service_instance="instance", exchange=k+1, tool=tool, native_delivery=True,
                argv_sha256=sha(json.dumps(argv,separators=(",",":"))), stdout_sha256=sha(result["stdout"]),
                stderr_sha256=sha(""), exit_code=0, start=dict(unix_ms=t,monotonic_ns=str(t*1000000)),
                end=dict(unix_ms=t+12,monotonic_ns=str((t+12)*1000000)), **{"class": "success"}))
            transcript.append(dict(kind="native_exchange", argv=argv, result=result))
            if tool in ("answer","search"):
                base=dict(observer_id="original",dispatch_identity=str(t),task="airk.focus_rank.v1",input_digest=str(t))
                journal.extend([dict(base, kind="dispatch_admitted", unix_ms=t+1),
                                dict(base, kind="score_returned", unix_ms=t+11, return_observed=True,
                                     outcome="success",work={"inference_us":9700})])
        body = {"ok":True}
        delivery = {"refused":False,"body":body}
        evidence = dict(id=q,scope=body,answer=delivery,probes=[dict(probe={"tool":tool,"arguments":{"k":j+2}},result=delivery) for j,tool in enumerate(menu)])
        transcript.extend([dict(kind="cal_question",evidence=evidence),dict(kind="timing_question_completed",question_id=q,exchanges=2+len(menu)),
                           dict(kind="window_release",question_id=q)])
    return transcript,journal,ids


class Readout(unittest.TestCase):
    def test_actual_intervals(self):
        rows=measure(*fixture(),0)
        self.assertEqual(len(rows),24)
        compare_menus(rows, list(rows))
        with self.assertRaises(ValueError): compare_menus(rows, rows[:-1])
        self.assertEqual(rows[1]["other_within_exchange_us"],2000)
        self.assertEqual(rows[1]["native_inference_us"],9700)
        self.assertEqual(rows[1]["outside_exchange_gap_us"],8000)

    def test_refuses_incomplete_or_changed_evidence(self):
        for issue in ("missing","orphan","partial","changed"):
            with self.subTest(issue=issue):
                transcript,journal,ids=fixture()
                if issue=="missing": journal.clear()
                if issue=="orphan": journal.append(dict(journal[0],unix_ms=15,dispatch_identity="foreign"))
                if issue=="partial": journal.pop()
                if issue=="changed": transcript[2]["result"]["stdout"]="changed"
                with self.assertRaises(ValueError): measure(transcript,journal,ids,0)


if __name__ == "__main__":
    unittest.main()
