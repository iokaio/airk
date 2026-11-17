# SPDX-License-Identifier: Apache-2.0
import copy,hashlib,json,unittest
import replay_logic as logic

class LiteralReplayTests(unittest.TestCase):
    def positive(self):
        text='The evidence paragraph directly answers the question.'
        original={'annotation_id':'humanA','answer':{'unanswerable':False,'evidence':[text]}}
        alt={'human_annotation_id':'humanA','original_record_sha256':hashlib.sha256(json.dumps(original,sort_keys=True,ensure_ascii=False).encode()).hexdigest(),'unanswerable':False,'original_evidence_count':1,'evidence':[{'source_evidence_sha256':hashlib.sha256(text.encode()).hexdigest(),'allowed_locations':['p0']}],'alignment':'all_original_evidence_maps_to_allowed_text'}
        task={'source_group':'qasper:1609.04186','dataset':'QASPER','split':'train','publisher_split':'train','human_alternatives':[alt]}
        docs={'p0':{'dataset':'QASPER','source_group':'qasper:1609.04186','license':'CC-BY-4.0','text':text,'paragraph_sha256':hashlib.sha256(text.encode()).hexdigest()}}
        return task,docs,{'humanA':original}
    def test_whole_original_human_evidence_positive(self):
        t,d,h=self.positive();self.assertEqual([(a['human_annotation_id'],p)for a,p in logic.positive_locations(t,d,h)],[('humanA','p0')])
    def test_unanswerable_and_incomplete_alternative_not_positive(self):
        for key,value in [('unanswerable',True),('alignment','unsupported_or_excluded_evidence_not_admitted')]:
            t,d,h=self.positive();t['human_alternatives'][0][key]=value;self.assertEqual(logic.positive_locations(t,d,h),[])
    def test_unknown_source_and_wrong_family_are_not_positive(self):
        for key,value in [('license','UNKNOWN'),('source_group','qasper:1701.08118'),('dataset','HotpotQA'),('text','unjudged paragraph')]:
            t,d,h=self.positive();d['p0'][key]=value
            with self.assertRaises(ValueError):logic.positive_locations(t,d,h)
    def test_altered_original_annotation_refuses(self):
        t,d,h=self.positive();h['humanA']['answer']['evidence'][0]='changed'
        with self.assertRaisesRegex(ValueError,'record changed'):logic.positive_locations(t,d,h)
    def test_nontrain_or_reserved_family_refuses(self):
        t,_,_=self.positive();logic.gate(t,set())
        for key,value in [('dataset','HotpotQA'),('split','test'),('publisher_split','validation')]:
            changed=dict(t,**{key:value})
            with self.assertRaises(ValueError):logic.gate(changed,set())
        with self.assertRaises(ValueError):logic.gate(t,{'qasper:1609.04186'})
    def test_utf8_span_boundaries_hash_and_bounds(self):
        raw='prefix é evidence suffix'.encode();start=len('prefix '.encode());end=len('prefix é evidence'.encode());wanted=hashlib.sha256('é evidence'.encode()).hexdigest();self.assertEqual(logic.full_window(raw,start,end,wanted),'é evidence'.encode())
        with self.assertRaises(UnicodeDecodeError):logic.full_window(raw,start+1,end,wanted)
        with self.assertRaises(ValueError):logic.full_window(raw,start,end,'00'*32)
        with self.assertRaises(ValueError):logic.full_window(raw,-1,end,wanted)
    def test_fixed45_balanced_family_and_permutation_stability(self):
        rows=[{'family':family,'id':family+':q'+str(i)}for family in ['qasper:1609.04186','qasper:1701.08118','qasper:1804.08782']for i in range(30)]
        selected=logic.round_robin(rows);self.assertEqual(len(selected),45);self.assertEqual(len({r['id']for r in selected}),45)
        self.assertEqual(selected,logic.round_robin(list(reversed(rows))))
        self.assertEqual(sorted(sum(r['family']==f for r in selected)for f in {r['family']for r in rows}),[15,15,15])
    def test_refused_rows_not_refilled_or_selected_from_score(self):
        rows=[{'family':'qasper:1609.04186','id':'q'+str(i),'native_fit':'UNOBSERVED'}for i in range(45)];selected=logic.round_robin(rows);changed=[dict(r,score=999-i,native_fit='refused'if i==0 else'accepted')for i,r in enumerate(rows)];self.assertEqual([r['id']for r in selected],[r['id']for r in logic.round_robin(changed)])
        with self.assertRaises(ValueError):logic.round_robin(rows[:-1])
    def test_duplicate_question_never_expands_draw_count(self):
        with self.assertRaisesRegex(ValueError,'duplicate'):logic.round_robin([{'family':'qasper:1609.04186','id':'q'},{'family':'qasper:1701.08118','id':'q'}],1)

if __name__=='__main__':unittest.main(verbosity=2)
