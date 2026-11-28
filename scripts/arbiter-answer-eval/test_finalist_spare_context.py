# SPDX-License-Identifier: Apache-2.0
"""Regression tests for the exact-boundary, multi-need finalist spare-byte fitter."""
import hashlib
import json
from types import SimpleNamespace
import unittest

import finalist_gather_contract as contract
import finalist_spare_context as spare


class SpareFrame(unittest.TestCase):
    def test_original_lines_links_and_frame_survive_unicode_and_digit_transitions(self):
        path = "docs/références.md"
        lines = [f'{n}: «one» "\\two"\t' + 'é相' * 160 for n in range(1, 121)]
        repo = SimpleNamespace(texts={path:lines}, digests={path:hashlib.sha256("\n".join(lines).encode()).digest()})
        at = f"{path}:99-100"
        supports = [{"need": 1, "scored_at": f"{path}:99-99"}, {"need": 2, "scored_at": f"{path}:100-100"}]
        candidate = SimpleNamespace(at=at,text="\n".join(lines[98:100])+"\n",supports=supports)
        for framing in ("line","header"):
            for cap in (8192,9200,12000):
                with self.subTest(framing=framing,cap=cap):
                    args = {"task":"Find the rule","needs":["Find the declaration","Find its use"],
                            "budget":{**contract.native_limits()["default_budget"],"max_bytes":cap}}
                    context = (args,repo,"index",{"scope_fingerprint":"f","publication":"rev"})
                    wire,initial = contract.render(*context,[candidate],1,421,framing)
                    expanded,final = spare.widen(*context,1,421,framing,wire,initial,passage_limit=10500)
                    self.assertEqual(len(final),1)
                    self.assertLessEqual(len(expanded),cap)
                    body=json.loads(json.loads(expanded.split(b"\r\n\r\n")[-1])["result"]["content"][0]["text"])
                    self.assertEqual(body["passages"][0]["support"],supports)
                    self.assertNotEqual(body["passages"][0]["at"],at)
                    self.assertIn(candidate.text,final[0].text)
                    self.assertLessEqual(len(final[0].text.encode()),10500)
                    if framing=="header":
                        header,payload=expanded.split(b"\r\n\r\n")
                        self.assertEqual(int(header.split(b":")[1]),len(payload))

    def test_empty_candidates_keep_typed_result(self):
        args={"task":"Where is it?"}
        repo=SimpleNamespace(texts={},digests={})
        ctx=(args,repo,"index",{"scope_fingerprint":"f","publication":"rev"},[],0,2,"line")
        self.assertEqual(spare.render(*ctx),contract.render(*ctx))


if __name__=="__main__":
    unittest.main()
