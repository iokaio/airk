# SPDX-License-Identifier: Apache-2.0
import copy,json,tempfile,unittest,types
from pathlib import Path
from chunk_contract import *
ROOT=Path(__file__).resolve().parents[6]
class ProofFixture:
    def __init__(self,root):
        self.root=Path(root)
        def put(name,value):
            p=self.root/name;p.write_text(json.dumps(value));return {'path':str(p),'sha256':sha(p)}
        self.put=put
        original=ROOT/'artifacts/advanced-split/models/actual-composed-service-r2/CAL-capture-next-corrected-r1/canonical-calls/native-session/i2-transcript'
        bindings=json.loads((original/'index-bindings.json').read_bytes());identity=json.loads((original/'session-ready.json').read_bytes())['identity'];grant=json.loads((original/'session-ready.json').read_bytes())['scope_result']
        self.binding=put('bindings.json',bindings);identity['bindings_sha256']=self.binding['sha256'];ready=json.loads((original/'fixture-ready.json').read_bytes());self.ready=put('ready.json',ready);identity['ready_sha256']=self.ready['sha256'];self.identity=identity
        source={'files':[{'file':'sources/example.txt','bytes':3,'sha256':'11'*32}]};self.audit=source['files'];self.source=put('manifest.json',source)
        scopes={'questions':[{'id':'synthetic:'+str(i),'question':'synthetic question '+str(i)}for i in range(100)]};self.ids=[r['id']for r in scopes['questions']]
        scopes_pin=put('scopes.json',scopes)
        protocol=json.loads((Path(__file__).with_name('protocol.proposed.json')).read_bytes());protocol['frozen_question_scopes']=scopes_pin;protocol['frozen_source_manifest']=self.source;self.protocol=put('protocol.json',protocol)
        evidence=put('start.json',{'started_unix':1600000000});self.origin=1600000000000;self.expiry=self.origin+10800000
        self.bound=put('bound.json',{'original_start_unix_ms':self.origin,'absolute_deadline_unix_ms':self.expiry,'original_start_evidence':evidence,'original_start_field':'started_unix'})
        helper=Path(__file__).resolve().parents[2]/'source'/'selected_cal_refusals.py'
        release={'approved':True,'Context_task_forward':False,'maximum_questions':100,'maximum_probes':500,'chunk_protocol_sha256':self.protocol['sha256'],'task_policy':'p','Focus_configuration':'ON','Focus_profile':'f','MiniLM_pipeline':'m','source_manifest_sha256':self.source['sha256'],'corpus_sha256':'c','frozen_configuration':{'opaque_native_configuration':'preserved'},'paths':{'index_bindings':self.binding['path'],'fixture_ready':self.ready['path']},'original_fixture_bound':self.bound,'input_pins':{self.binding['path']:self.binding['sha256'],self.ready['path']:self.ready['sha256'],str(helper):sha(helper)},'absent_build_inputs':{}}
        self.release=put('release.json',release);runtime=put('runtime-source.json',{});owner={'path':str(ROOT/'scripts/provision-static-onnx-runtime.py'),'sha256':CAL_OWNER_SHA}
        self.phase_packets=[]
        for i,(start,end)in enumerate(ranges()):self.phase_packets.append(dict(put('packet'+str(i)+'.json',{'chunk_index':i,'owner_seconds':1800,'question_start':start,'question_end':end,'questions':self.ids[start:end],'release':self.release,'protocol':self.protocol,'fixture_bound':self.bound,'runtime_python':runtime,'capture_driver':runtime,'owner':owner}),chunk_index=i))
        self.controller=put('controller.json',{'schema':'airk.private-selected-cal-chunk-controller-packet/1','protocol':self.protocol,'release':self.release,'fixture_bound':self.bound,'phase_packets':self.phase_packets})
        self.decision=put('decision.json',{'schema':'airk.private-selected-cal-chunk-controller-decision/1','approved':True,'controller_packet_sha256':self.controller['sha256']})
        self.members=[]
        for i,(start,end)in enumerate(ranges()):
            began=self.origin+i*10000;ended=began+9000;rows=[]
            for offset,n in enumerate(range(start,end)):
                management={'after_question_id':self.ids[n-1]if offset else None,'canonical_read':False,'management_deadline_seconds':20,'started_unix_ms':began+offset*100,'completed_unix_ms':began+offset*100+1,'original_native_scope_result':{'scope_result':copy.deepcopy(grant),'identity':copy.deepcopy(identity),'launch_count':1}}
                call={'returncode':0,'validated':True,'envelope':{'isError':False},'identity_error':None,'parse_error':None,'timed_out':False,'elapsed_seconds':1,'deadline_seconds':30,'actual_service_identity':{'actual_native_health':{'instance':bindings['instance']}},'native_binding_sha256':self.binding['sha256'],'full_canonical_value':{'opaque_native_fingerprint':'fresh-nonce-'+str(n)}}
                rows.append({'id':self.ids[n],'question':scopes['questions'][n]['question'],'split':'calibration','native_capture_status':'no_native_assessment_request','request':None,'admission':None,'scope_management':management,'answer_call':call,'counterfactual_choices':[],'counterfactual_refusals':[]})
            close={'kind':'closed','helper_exit_observed':True,'helper_exit_code':0,'child_exit_observed':True,'child_exit_code':0,'child_signal_code':None,'poisoned':None,'launch_count':1,'identity':identity}
            capture={key:release[key]for key in HEADER};capture.update(schema=CHUNK,complete=True,stopped=False,chunk_index=i,question_start=start,question_end=end,total_census_questions=100,Context_task_NN_invocations=0,human_semantic_gold_count=0,rows=rows,chunk_protocol_sha256=self.protocol['sha256'],release_sha256=self.release['sha256'],phase_started_unix_ms=began,phase_finished_unix_ms=ended,fixture_absolute_deadline_unix_ms=self.expiry,source_audit_before=self.audit,source_audit_after=self.audit,native_session_close=close,scope_management=[r['scope_management']for r in rows],stage_before={'full_native_response':{'configuration':release['frozen_configuration']}},stage_after={'full_native_response':{'configuration':release['frozen_configuration']}})
            cp=put('capture'+str(i)+'.json',capture)
            owned={'exit_code':0,'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,'owned_job_forced_termination':False,'owned_job_final_active_processes':0,'owned_job_final_member_pids':[],'parent_assigned_to_job':False,'child_assigned_before_resume':True}
            receipt={'schema':OWNED,'complete':True,'chunk_index':i,'protocol_sha256':self.protocol['sha256'],'post_input_drift':[],'controller_packet':self.controller,'decision':self.decision,'packet':self.phase_packets[i],'capture':cp,'owned':owned,'started_unix_ms':began,'finished_unix_ms':ended,'elapsed_seconds':9,'input_seals':dict(release['input_pins'],**{runtime['path']:runtime['sha256'],owner['path']:owner['sha256']}),'absent_build_inputs':{}}
            self.members.append(put('execution'+str(i)+'.json',receipt))
    def validate(self):return validate_members(self.protocol,self.controller,self.decision,self.members)
    def change(self,index,mutate,capture=True):
        row=self.members[index];receipt=json.loads(Path(row['path']).read_bytes())
        if capture:
            value=json.loads(Path(receipt['capture']['path']).read_bytes());mutate(value);receipt['capture']=self.put('capture'+str(index)+'.json',value)
        else:mutate(receipt)
        self.members[index]=self.put('execution'+str(index)+'.json',receipt)

def aggregate_fixture(f):
    checked=f.validate();reports=checked['reports'];release=checked['release'];rows=[copy.deepcopy(row)for report in reports for row in report['rows']]
    capture={key:release[key]for key in HEADER};capture.update(schema=CAPTURE,complete=True,phase_count=13,chunk_protocol_sha256=f.protocol['sha256'],release_sha256=f.release['sha256'],rows=rows,Context_task_NN_invocations=0,human_semantic_gold_count=0)
    cp=f.put('aggregate-capture.json',capture)
    receipt={'schema':AGGREGATE,'complete':True,'qualification_granted':False,'post_input_drift':[],'protocol':f.protocol,'controller_packet':f.controller,'decision':f.decision,'capture':cp,'phases':f.members,'instance':checked['instance'],'native_execution_phases':13}
    return capture,receipt,cp,f.put('aggregate-execution.json',receipt)

class AggregateTests(unittest.TestCase):
    def run_case(self,mutation=None):
        with tempfile.TemporaryDirectory()as root:
            f=ProofFixture(root)
            if mutation:mutation(f)
            return f.validate()
    def test_exact_thirteen_and_fresh_fingerprints_preserved(self):self.assertEqual(len(self.run_case()['ids']),100)
    def test_missing_phase_refuses(self):
        with self.assertRaises(ValueError):self.run_case(lambda f:f.members.pop())
    def test_duplicate_or_reordered_id_refuses(self):
        for mode in ('duplicate','reverse'):
            def mutate(f):
                f.change(0,lambda c:c['rows'].__setitem__(0,copy.deepcopy(c['rows'][1]))if mode=='duplicate'else c['rows'].reverse())
            with self.assertRaises(ValueError):self.run_case(mutate)
    def test_stopped_partial_or_foreign_question_refuses(self):
        for field,value in [('stopped',True),('complete',False)]:
            with self.assertRaises(ValueError):self.run_case(lambda f:f.change(0,lambda c:c.__setitem__(field,value)))
        with self.assertRaises(ValueError):self.run_case(lambda f:f.change(0,lambda c:c['rows'][0].__setitem__('question','foreign')))
    def test_forced_or_unsettled_phase_refuses(self):
        for field,value in [('owned_job_forced_termination',True),('owned_job_final_active_processes',1)]:
            with self.assertRaises(ValueError):self.run_case(lambda f:f.change(0,lambda r:r['owned'].__setitem__(field,value),False))
    def test_missing_input_seal_refuses(self):
        with self.assertRaises(ValueError):self.run_case(lambda f:f.change(0,lambda r:r.__setitem__('input_seals',{}),False))
    def test_phase_or_global_deadline_refuses(self):
        with self.assertRaises(ValueError):self.run_case(lambda f:f.change(0,lambda r:r.__setitem__('elapsed_seconds',1800),False))
        with self.assertRaises(ValueError):self.run_case(lambda f:f.change(0,lambda r:r.__setitem__('finished_unix_ms',f.expiry+1),False))
    def test_configuration_or_source_changed_refuses(self):
        with self.assertRaises(ValueError):self.run_case(lambda f:f.change(0,lambda c:c['source_audit_after'].clear()))
        with self.assertRaises(ValueError):self.run_case(lambda f:f.change(0,lambda c:c['stage_after']['full_native_response'].__setitem__('configuration',{})))
    def test_scope_changed_or_midquestion_refresh_refuses(self):
        with self.assertRaises(ValueError):self.run_case(lambda f:f.change(0,lambda c:c['rows'][1]['scope_management'].__setitem__('after_question_id','foreign')))
        with self.assertRaises(ValueError):self.run_case(lambda f:f.change(0,lambda c:c['rows'][1]['scope_management']['original_native_scope_result']['scope_result'].__setitem__('scope_identity','foreign')))
    def test_unknown_call_error_cannot_become_complete_chunk(self):
        with self.assertRaises(ValueError):self.run_case(lambda f:f.change(0,lambda c:c['rows'][0]['answer_call'].__setitem__('returncode',1)))
    def test_changed_profile_or_context_forward_refuses(self):
        with self.assertRaises(ValueError):self.run_case(lambda f:f.change(0,lambda c:c.__setitem__('Focus_profile','foreign')))
        with self.assertRaises(ValueError):self.run_case(lambda f:f.change(0,lambda c:c.__setitem__('Context_task_NN_invocations',1)))
    def test_actual_aggregate_metadata_chain_accepts_without_single_owned_claim(self):
        with tempfile.TemporaryDirectory()as root:
            f=ProofFixture(root);capture,receipt,cp,ep=aggregate_fixture(f);self.assertTrue(validate_aggregate(ep,cp,f.release))
    def test_aggregate_cannot_normalize_actual_native_fingerprints(self):
        with tempfile.TemporaryDirectory()as root:
            f=ProofFixture(root);capture,receipt,cp,ep=aggregate_fixture(f);capture['rows'][0]['answer_call']['full_canonical_value']['opaque_native_fingerprint']='normalized';cp=f.put('aggregate-capture.json',capture);receipt['capture']=cp;ep=f.put('aggregate-execution.json',receipt)
            with self.assertRaises(ValueError):validate_aggregate(ep,cp,f.release)
    def test_no_fabricated_single_phase_owned_proof(self):
        with tempfile.TemporaryDirectory()as root:
            f=ProofFixture(root);capture,receipt,cp,ep=aggregate_fixture(f);receipt['owned']={'exit_code':0};ep=f.put('aggregate-execution.json',receipt)
            with self.assertRaises(ValueError):validate_aggregate(ep,cp,f.release)
    def test_old_foreign_instance_call_cannot_be_reused(self):
        with self.assertRaises(ValueError):self.run_case(lambda f:f.change(0,lambda c:c['rows'][0]['answer_call']['actual_service_identity']['actual_native_health'].__setitem__('instance','old_failed_fixture')))
class ConsumerTests(unittest.TestCase):
    def consumer(self):
        target=ROOT/'artifacts/advanced-split/models/selected-native-goal-driver-r1/native-cal-owned-r1/run_raw_reviewed.py'
        proposal=Path(__file__).with_name('run_raw_reviewed.py.proposed')
        module=types.ModuleType('proposed_chunk_consumer');module.__file__=str(target)
        exec(compile(proposal.read_bytes(),str(target),'exec'),module.__dict__)
        return module
    def test_certificate_and_raw_use_explicit_aggregate_branch(self):
        with tempfile.TemporaryDirectory()as root:
            f=ProofFixture(root);capture,receipt,cp,ep=aggregate_fixture(f)
            pins=self.consumer().capture_execution(ep['path'],ep['sha256'],cp,f.release)
            self.assertIn(str(Path(__file__).with_name('chunk_contract.py')),pins)
            self.assertTrue(all(row['path']in pins for row in f.members))
    def test_consumer_refuses_missing_member(self):
        with tempfile.TemporaryDirectory()as root:
            f=ProofFixture(root);capture,receipt,cp,ep=aggregate_fixture(f);receipt['phases'].pop();ep=f.put('aggregate-execution.json',receipt)
            with self.assertRaises(ValueError):self.consumer().capture_execution(ep['path'],ep['sha256'],cp,f.release)
if __name__=='__main__':unittest.main()
