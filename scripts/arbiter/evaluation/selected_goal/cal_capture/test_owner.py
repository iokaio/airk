# SPDX-License-Identifier: Apache-2.0
import io
import json
import unittest
from unittest.mock import Mock

from controller import summary
from launch import wait_cal_publication, wait_stage


class OwnerTests(unittest.TestCase):
    def make_log(self, events):
        log = Mock()
        log.stat.return_value.st_size = 100
        log.open.return_value = io.BytesIO(
            b"".join(json.dumps(value).encode() + b"\n" for value in events))
        return log

    def test_summary_keeps_refusals_in_denominator(self):
        ids = [f"q{i}" for i in range(4)]
        rows = []
        for key in ids:
            rows += [{"kind":"window_grant","question_id":key},
                     {"kind":"cal_question","evidence":{"id":key,"probes":[],"pipeline_searches":0,
                       "scope":{"scope_observation":{"fingerprint":key}}}},
                     {"kind":"window_release","question_id":key}]
        rows.append({"kind": "phase_complete", "phase": 12, "ids": ids,
                     "instance": "original", "publication": {"token":"p"}})
        self.assertEqual(summary(self.make_log(rows), 12, ids, "original", {"token":"p"}, set()), (0, 0))
        with self.assertRaisesRegex(ValueError, "incomplete"):
            summary(self.make_log(rows[:-1]), 12, ids, "original", {"token":"p"}, set())
        with self.assertRaisesRegex(ValueError, "incomplete"):
            summary(self.make_log(rows), 12, ids, "changed", {"token":"p"}, set())

    def test_counterfactual_limit_preserves_all_five_not_six(self):
        item = {"kind":"cal_question", "evidence":{"id":"q","probes":[{}]*6}}
        with self.assertRaisesRegex(ValueError, "five"):
            summary(self.make_log([item]),0,["q"],"s",{},set())

    def test_wait_stage_uses_matching_observer_and_original_revision(self):
        from unittest.mock import patch
        work = Mock()
        journal = Mock()
        journal.stat.return_value.st_size = 500
        work.__truediv__ = Mock(return_value=journal)
        event = {"stage":"focus","accepted_configure_revision":3,"profile_manifest":"f",
                 "observer_id":"original","outcome":"ready"}
        def inspect(_parent, observe, _child, deadline, _phase, **_kw):
            self.assertEqual(deadline, 784)
            return observe()
        def feed(rows):
            journal.open.return_value = io.BytesIO(b"".join(json.dumps(r).encode()+b"\n" for r in rows))
        with patch("launch.native.wait_observed", side_effect=inspect):
            feed([dict(event, accepted_configure_revision=2), event])
            self.assertEqual(wait_stage(work,None,{"warm_observer_id":"original"},"focus","f",3,784),event)
            feed([dict(event,observer_id="foreign")])
            with self.assertRaisesRegex(ValueError,"identity"):
                wait_stage(work,None,{"warm_observer_id":"original"},"focus","f",3,784)


class PublicationChecks(unittest.TestCase):
    def test_scoped_publication_busy_reuses_original_call_and_budget(self):
        from unittest.mock import MagicMock, patch
        from time import monotonic
        scope = {"selection": {"owner": "agent"}, "members": [{"index_id": "one"}]}
        busy = {"isError": True, "structuredContent": {"scope_observation": scope,
                "code": "publication_busy", "retryable": True, "status": "retry",
                "action": "repeat_same_request", "retry_after_ms": 25}}
        success = {"structuredContent": {"read_scope": {"source": "named", "index_ids": ["one"]},
                   "scope_observation": scope, "semantic_readiness": {"state": "ready"},
                   "publication": {"token": "p"}}}
        root = MagicMock()
        root.__truediv__.return_value.is_file.return_value = True
        root.__truediv__.return_value.stat.return_value.st_size = 512
        cli = Mock(side_effect=[ValueError("native exit 1"), success])
        with patch("launch.native.read_json", return_value=busy), patch("time.sleep") as delay:
            result = wait_cal_publication(cli, root, "one", Mock(), monotonic() + 400)
        delay.assert_called_once_with(0.025)
        self.assertEqual(cli.call_args_list[0].args[1:], cli.call_args_list[1].args[1:])
        self.assertEqual(result, success["structuredContent"])
        foreign = dict(busy, structuredContent=dict(busy["structuredContent"], scope_observation={}))
        with patch("launch.native.read_json", return_value=foreign):
            with self.assertRaisesRegex(ValueError, "authority"):
                wait_cal_publication(Mock(side_effect=ValueError()), root, "one", Mock(), monotonic() + 400)


if __name__ == "__main__":
    unittest.main()
