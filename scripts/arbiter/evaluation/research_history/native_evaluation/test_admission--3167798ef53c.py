# SPDX-License-Identifier: Apache-2.0
import importlib.util
from pathlib import Path
import unittest
spec=importlib.util.spec_from_file_location("admission",Path(__file__).with_name("admission.py"))
a=importlib.util.module_from_spec(spec);spec.loader.exec_module(a)
def fixture():
    names=["EXPOSED","DEV","CAL_SEALED_UNOPENED","FINAL_SEALED_UNOPENED"]
    rows=[{"component_digest":f"{i:064x}","question_ids":[f"q{i}"],"article_digests":[f"{i+100:064x}"],"disposition":names[i%4],"exclusion":None}for i in range(16)]
    rows[14]["component_digest"] = f"{22:064x}"  # SHA256 prefix45: fixed odd-side fixture.
    return {"schema":"airk.private-a1-blind-metadata-audit/1","components":rows,
            "counts_before_unperformed_rights_alignment_checks":{n:{"questions":4}for n in names}}
class Controls(unittest.TestCase):
    def test_entire_old_final_dev_exposed_excluded(self):
        out=a.reserve(fixture());self.assertEqual(len(out["EXCLUDED"]),12)
        self.assertEqual({q for r in out["CAL"]+out["CONFIRMATION"] for q in r["question_ids"]},{"q2","q6","q10","q14"})
    def test_split_fixed_preoutcome(self):
        out=a.reserve(fixture());self.assertEqual([r["question_ids"] for r in out["CAL"]],[["q2"],["q6"],["q10"]]);self.assertEqual([r["question_ids"]for r in out["CONFIRMATION"]],[["q14"]])
    def test_input_order_irrelevant(self):
        f=fixture();before=a.reserve(f);f["components"].reverse();self.assertEqual(a.reserve(f),before)
    def test_shared_question_refuses(self):
        f=fixture();f["components"][1]["question_ids"]=["q0"]
        with self.assertRaisesRegex(ValueError,"overlap"):a.reserve(f)
    def test_shared_source_refuses_even_if_questions_distinct(self):
        f=fixture();f["components"][1]["article_digests"]=f["components"][0]["article_digests"]
        with self.assertRaisesRegex(ValueError,"overlap"):a.reserve(f)
    def test_population_drift_refuses(self):
        f=fixture();f["components"].pop()
        with self.assertRaisesRegex(ValueError,"population"):a.reserve(f)
    def test_unknown_disposition_refuses(self):
        f=fixture();f["components"][1]["disposition"]="READY"
        with self.assertRaisesRegex(ValueError,"disposition"):a.reserve(f)
    def test_excluded_cal_refuses(self):
        f=fixture();f["components"][2]["exclusion"]="new exposure"
        with self.assertRaisesRegex(ValueError,"excluded CAL"):a.reserve(f)
    def test_no_question_gold_fields_needed(self):
        self.assertEqual(sum(len(v)for v in a.reserve(fixture()).values()),16)
if __name__=="__main__":unittest.main()
