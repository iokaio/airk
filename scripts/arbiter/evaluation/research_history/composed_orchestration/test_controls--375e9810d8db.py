# SPDX-License-Identifier: Apache-2.0
"""Private pure/own-file controls only; no child, model, ORT or native selector."""
import copy,hashlib,json,subprocess,unittest
from pathlib import Path
from unittest.mock import patch
from capacity import Capacity,initial_free,GRAPH,WEIGHTS,RESERVATION,FREE,WindowsRead
from runner import final_observation,native_result,unaliased,SID
from security import observe
BASE=Path(__file__).resolve().parent
def row(size=1):
    return {'bytes':size,'identity':[1,2,3],'regular':True,'reparse':False,'links':1}
def memory(rss=1,commit=1):return {'peak_rss':rss,'peak_job_commit':commit}
class Controls(unittest.TestCase):
    def test_graph_and_external_exact_bounds_then_one_byte_refusals(self):
        Capacity().observe({'graph':row(GRAPH),'weights':row(WEIGHTS)},memory())
        for name,cap in [('graph',GRAPH),('weights',WEIGHTS)]:
            with self.subTest(name=name),self.assertRaises(ValueError):Capacity().observe({name:row(cap+1)},memory())
    def test_peak_rss_and_job_commit_independently_refuse(self):
        Capacity().observe({},memory(RESERVATION,RESERVATION))
        for m in [memory(RESERVATION+1,1),memory(1,RESERVATION+1)]:
            c=Capacity()
            with self.assertRaises(ValueError):c.observe({},m)
            self.assertEqual(c.proof()['peak_process_rss_bytes'],m['peak_rss'])
            self.assertEqual(c.proof()['peak_job_committed_bytes'],m['peak_job_commit'])
    def test_memory_unknown_zero_boolean_or_negative_never_passes(self):
        for key in ['peak_rss','peak_job_commit']:
            for value in [None,0,True,-1]:
                m=memory();m[key]=value
                with self.subTest(key=key,value=value),self.assertRaises(ValueError):Capacity().observe({},m)
    def test_reparse_hardlink_and_nonfile_refuse(self):
        for key,value in [('reparse',True),('links',2),('regular',False)]:
            changed=row();changed[key]=value
            with self.subTest(key=key),self.assertRaises(ValueError):Capacity().observe({'graph':changed},memory())
    def test_output_replacement_truncation_and_disappearance_refuse(self):
        for mutation in ['replacement','truncate','missing']:
            c=Capacity();c.observe({'graph':row(3)},memory());v=row(4)
            if mutation=='replacement':v['identity']=[9,9,9]
            if mutation=='truncate':v['bytes']=2
            with self.subTest(mutation=mutation),self.assertRaises(ValueError):c.observe({} if mutation=='missing' else {'graph':v},memory())
    def test_free_storage_original9gib_boundary(self):
        initial_free(FREE)
        for value in [FREE-1,None,True,-1]:
            with self.subTest(value=value),self.assertRaises(ValueError):initial_free(value)
    def test_native_file_sizes_and_peak_observations_retained(self):
        c=Capacity();c.observe({'graph':row(1)},memory(10,20));c.observe({'graph':row(2)},memory(9,19))
        p=c.proof();self.assertEqual(p['peak_process_rss_bytes'],10);self.assertEqual(p['peak_job_committed_bytes'],20)
        self.assertEqual(p['last_files']['graph']['bytes'],2);self.assertEqual(p['observations'],2)
    def test_final_hash_late_prevents_success(self):
        current=[9.];p=BASE/'capacity.py';r={'complete':True,'failure':None,'post_input_drift':[]}
        def hashed(path):current[0]=11.;return 'retained-hash'
        final_observation(r,0.,10.,p,clock=lambda:current[0],hash_file=hashed)
        self.assertFalse(r['complete']);self.assertTrue(r['original_phase_expired']);self.assertEqual(r['stdout']['sha256'],'retained-hash')
    def test_prior_failure_survives_final_late_hash(self):
        r={'complete':False,'failure':{'type':'OriginalNativeRefusal','message':'original'},'post_input_drift':[]}
        final_observation(r,0.,10.,BASE/'capacity.py',clock=lambda:11.,hash_file=lambda p:'hash')
        self.assertEqual(r['failure']['type'],'OriginalNativeRefusal');self.assertFalse(r['complete'])
    def test_input_alias_relative_and_dot_ancestry_refused(self):
        for path in ['relative.py',str(BASE/'controls')+'/../capacity.py']:
            with self.subTest(path=path),self.assertRaises(ValueError):unaliased(path)
    def test_native_single_row_identity_and_numerical_failure_controls(self):
        plan={'native_input':{'sha256':'input'},'profile':'new-profile','numeric_input_digest':'original8192'}
        request={'source_manifest':{'path':'original-source','sha256':'source'},'profile':{'sha256':'profile-source'},'policy':{'native':'unchanged'},'policy_digest':'policy'}
        owner={'owned_leader_pid':17,'suspended_assignment':{'image':{'sha256':'new-image'}}}
        body={'kind':'compact_attention_diagnostic','complete':True,'profiling_enabled':True,'diagnostic_measurement_identity':'compact-fp32-gqa-profiled/1','process_id':17,'native_binary_sha256':'new-image','input_sha256':'input','profile':'new-profile','execution_user':SID,
            'source_manifest':request['source_manifest'],'profile_source_sha256':'profile-source','resource_policy':request['policy'],'resource_policy_digest':'policy',
            'rows':[{'input_digest':'original8192','tokens':8192,'options':8,'raw_scores':[1.,2.,3.,4.,5.,6.,7.,8.]}],
            'original_fp32_comparison':{'raw':.001,'mass':.05,'argmax':True}}
        native_result(body,plan,request,owner)
        for key,value in [('process_id',18),('native_binary_sha256','old-image'),('execution_user','foreign'),('profile','old-profile'),('rows',[]),('rows',body['rows']*2)]:
            changed=copy.deepcopy(body);changed[key]=value
            with self.subTest(key=key),self.assertRaises(ValueError):native_result(changed,plan,request,owner)
        for key,value in [('raw',.0011),('mass',.051),('argmax',False),('raw',False)]:
            changed=copy.deepcopy(body);changed['original_fp32_comparison'][key]=value
            with self.subTest(key=key),self.assertRaises(ValueError):native_result(changed,plan,request,owner)
    def test_actual_windows_owned_file_metadata_and_parent_security(self):
        path=BASE/'controls'/'owned-file-metadata.bin'
        with path.open('xb') as f:f.write(b'owned native metadata control')
        r=WindowsRead();value=r.file(path)
        self.assertEqual(value['bytes'],29);self.assertEqual(value['links'],1);self.assertTrue(value['regular']);self.assertFalse(value['reparse'])
        self.assertEqual(observe(BASE.parent)['owner_sid'],SID)
if __name__=='__main__':
    called=[]
    def nochild(*a,**k):called.append(1);raise AssertionError('No process creation in ownership controls')
    with patch.object(subprocess,'Popen',nochild):
        result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Controls))
    (BASE/'controls'/'result.json').write_text(json.dumps({'selected':result.testsRun,'passed':result.testsRun-len(result.errors)-len(result.failures),'errors':len(result.errors),'failures':len(result.failures),'child_process_calls':len(called),'model_kernel_or_cargo_runs':0},indent=2)+'\n')
    raise SystemExit(0 if result.wasSuccessful() and not called else 1)
