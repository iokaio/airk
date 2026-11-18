# SPDX-License-Identifier: Apache-2.0
"""Pure graph preparation/custody controls; no files, native process, model or Git."""
import copy
import datetime
import hashlib
import importlib.util
import io
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch, Mock

ROOT=Path(__file__).resolve().parents[2]
def module(name,path):
    spec=importlib.util.spec_from_file_location(name,path)
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value
g=module('graph_preparation_controls',ROOT/'scripts/cuda-component-execution/graph_provenance.py')
c=module('graph_wrapper_controls',ROOT/'scripts/cuda-component-execution.py')
DEADLINE=120.0

def pin(path,data=b'x'):
    return dict(path=str(path),bytes=len(data),sha256=hashlib.sha256(data).hexdigest())

def identity(data=b'x'):
    return dict(volume_serial=7,file_id='12'*16,links=1,bytes=len(data),last_write=19,
                attributes=0,delete_pending=False,directory=False)

def user():
    return dict(UserSid=g.SID,OwnerSid=g.SID,InteractiveOwnerSids=[g.SID])

class Opened:
    def __init__(self,path,data=b'x'):
        self.path=Path(path);self.stream=io.BytesIO(data);self.observed=identity(data);self.closed=False
    def identity(self):return dict(self.observed)
    def rewind(self):self.stream.seek(0)
    def seek(self,offset):self.stream.seek(offset)
    def read(self,count):return self.stream.read(count)
    def close(self):self.closed=True

def policy(plan,paths,deadline):
    return dict(identity=user(),policies={g.key(p):dict(owner_sid=g.SID,sddl='literal policy') for p in paths})

class Custody(unittest.TestCase):
    def setUp(self):
        self.clock=patch.object(g.time,'monotonic',return_value=1);self.clock.start();self.addCleanup(self.clock.stop)
        self.path=ROOT/'scripts/literal-graph-test.py';self.pin=pin(self.path);self.files=[]
    def opened(self,path):
        value=Opened(path);self.files.append(value);return value
    def lease(self,**kw):return g.Lease({},[self.pin],DEADLINE,opener=self.opened,observe=policy,**kw)
    def test_retained_reads_and_actual_final_rehash_close(self):
        lease=self.lease()
        self.assertEqual(lease.bytes(self.path),b'x');self.assertFalse(self.files[0].closed)
        lease.refresh();lease.close()
        self.assertTrue(self.files[0].closed);self.assertEqual(lease.closed_count,1)
        with self.assertRaises(ValueError):lease.bytes(self.path)
    def test_wrong_content_closes_acquired_handle(self):
        with patch.object(Opened,'read',return_value=b''):
            with self.assertRaisesRegex(ValueError,'bytes differ'):self.lease()
        self.assertTrue(self.files[0].closed)
    def test_initial_and_mid_hash_expiry_close_all(self):
        with patch.object(g.time,'monotonic',return_value=120):
            with self.assertRaises(ValueError):self.lease()
        self.assertEqual(self.files,[])
        with patch.object(g.time,'monotonic',side_effect=[1,1,1,120]):
            with self.assertRaises(ValueError):self.lease()
        self.assertTrue(self.files[0].closed)
    def test_links_reparse_pending_and_directory_refuse(self):
        for field,value in [('links',2),('attributes',0x400),('delete_pending',True),('directory',True)]:
            with self.subTest(field=field):
                def opened(path):
                    f=self.opened(path);f.observed[field]=value;return f
                with self.assertRaises(ValueError):g.Lease({},[self.pin],120,opener=opened,observe=policy)
                self.assertTrue(self.files[-1].closed)
    def test_reopened_file_identity_policy_and_bytes_are_not_cached_booleans(self):
        with self.lease() as prior:expected={g.key(self.path):copy.deepcopy(prior.records()[0])}
        for field,value in [('file_id','34'*16),('last_write',20)]:
            changed=copy.deepcopy(expected);changed[g.key(self.path)]['identity'][field]=value
            with self.assertRaises(ValueError):self.lease(expected=changed)
            self.assertTrue(self.files[-1].closed)
        changed=copy.deepcopy(expected);changed[g.key(self.path)]['policy']['sddl']='other'
        with self.assertRaises(ValueError):self.lease(expected=changed)
    def test_final_content_identity_acl_and_deadline_drift_refuse(self):
        for kind in ('content','identity','policy','deadline'):
            lease=self.lease()
            try:
                if kind=='content':self.files[-1].stream=io.BytesIO(b'z')
                if kind=='identity':self.files[-1].observed['file_id']='56'*16
                if kind=='policy':
                    lease.observe=lambda *args:dict(identity=user(),policies={g.key(self.path):dict(owner_sid=g.SID,sddl='changed')})
                if kind=='deadline':
                    with patch.object(g.time,'monotonic',return_value=120):
                        with self.assertRaises(ValueError):lease.refresh()
                else:
                    with self.assertRaises(ValueError):lease.refresh()
            finally:lease.close()
    def test_duplicate_pins_and_unheld_reads_refuse(self):
        with self.assertRaises(ValueError):g.Lease({},[self.pin,self.pin],120,opener=self.opened,observe=policy)
        self.assertTrue(self.files[-1].closed)
        with self.lease() as lease:
            with self.assertRaises(ValueError):lease.pin(ROOT/'unheld')
            with self.assertRaises(ValueError):lease.digest(ROOT/'unheld')
    def test_loaded_failure_restores_module_registration(self):
        data=b"raise ValueError('literal import refusal')\n"
        existing=SimpleNamespace(marker='prior');sys.modules['graph_failure_test']=existing
        lease=g.Lease({},[pin(self.path,data)],120,opener=lambda p:Opened(p,data),observe=policy)
        try:
            with self.assertRaises(ValueError):lease.load(self.path,'graph_failure_test')
        finally:lease.close()
        self.assertIs(sys.modules.pop('graph_failure_test'),existing)
    def test_source_load_reads_only_retained_bytes_and_restores_aliases(self):
        data=b'ANSWER=7\n'
        lease=g.Lease({},[pin(self.path,data)],120,opener=lambda p:Opened(p,data),observe=policy)
        with patch.object(Path,'read_bytes',side_effect=AssertionError('path source reread')):
            first=lease.load(self.path,'graph_module_a');second=lease.load(self.path,'graph_module_b')
        self.assertIs(first,second);self.assertEqual(first.ANSWER,7)
        lease.close();self.assertNotIn('graph_module_a',sys.modules);self.assertNotIn('graph_module_b',sys.modules)
    def test_close_failure_attempts_other_handles_and_records_truth(self):
        second=pin(ROOT/'scripts/second.py')
        lease=g.Lease({},[self.pin,second],120,opener=self.opened,observe=policy)
        self.files[-1].close=Mock(side_effect=OSError('close failed'))
        with self.assertRaises(ValueError):lease.close()
        self.assertTrue(self.files[0].closed);self.assertEqual(lease.closed_count,1)
        self.assertEqual(len(lease.close_errors),1)
    def test_finish_keeps_handles_until_output_proofs_and_closes_before_last_clock(self):
        lease=self.lease();receipt={'complete':True};events=[]
        paths={'result':ROOT/'result.json'}
        def output(path):events.append('output');self.assertFalse(lease.closed);return pin(path)
        def finalizer():events.append('release');g.finish_lease(lease,receipt)
        def clock():events.append('clock');self.assertTrue(lease.closed);return 2
        # Preparation pin is evidence recorded only; no path access in finalization.
        lease.plan={'graph_preparation':pin(ROOT/'preparation.json')}
        c.finish(receipt,paths,0,120,output,clock=clock,proofs={'result':8},finalizer=finalizer)
        self.assertEqual(events,['output','release','clock'])
        self.assertTrue(receipt['complete']);self.assertEqual(receipt['graph_custody']['closed_handles'],1)
    def test_failed_refresh_still_closes_and_cannot_complete(self):
        lease=self.lease();lease.plan={'graph_preparation':pin(ROOT/'preparation.json')}
        self.files[0].observed['file_id']='changed';receipt={'complete':True}
        c.finish(receipt,{},0,120,lambda _:None,clock=lambda:2,proofs={},
                 finalizer=lambda:g.finish_lease(lease,receipt))
        self.assertTrue(lease.closed);self.assertFalse(receipt['complete'])


def boundary_fixture():
    fields=('wrapper','custody_owner','contract','parent_python','powershell','checked_guard',
        'metadata_receipt','image_packet','native_build_receipt','native_image','request','native_input',
        'profile','descriptor','graph_preserver','graph_provenance_owner','graph_provenance_contract',
        'graph_output_guard','output_guard','output_guard_lock')
    plan={name:pin(ROOT/('literal/'+name)) for name in fields}
    binding={name:pin(ROOT/('literal/binding-'+name)) for name in
        ('environment','source_roster','archive','producer','manifest','runtime_producer','cargo_json','preserver_source')}
    binding['source_pins']=[pin(ROOT/'scripts/verifier.py')]
    proof={name:pin(ROOT/('literal/proof-'+name)) for name in
        ('build_receipt','producer_source','parent_producer','parent_build_proof','recipe',
         'new_source','new_object','new_session','new_aggregate')}
    proof['source_patch']={'base':pin(ROOT/'literal/base.cc'),'after':pin(ROOT/'literal/after.cc')}
    # These deliberately remain data in the proof, not current execution files.
    proof['compiler_dependencies']=[pin(ROOT/'old-build/header.h')]
    proof['session_input_pins']=[pin(ROOT/'old-build/unused.obj')]
    proof['aggregate_input_pins']=[pin(ROOT/'old-build/unused.lib')]
    binding['proof']=proof
    recipe={'helper_pins':[pin(ROOT/'scripts/build-helper.py')]}
    raw={'commands':[{'stdout':pin(ROOT/'literal/compile.log')}]}
    helper_root=ROOT/'scripts/cuda-native-execution/helpers'
    helper=pin(helper_root/'reference_worker.py')
    native_source=pin(ROOT/'crates/native.rs');weight=pin(ROOT/'literal/model.weights')
    source=pin(ROOT/'literal/model-source.json');fixture=pin(ROOT/'literal/fixture.json')
    producer=pin(ROOT/'literal/reference.json');old_plan=pin(ROOT/'literal/metadata-plan.json')
    old_build=pin(ROOT/'literal/old-build.json');output=pin(ROOT/'literal/metadata-output.json')
    docs={plan['metadata_receipt']['path']:{'plan':old_plan,'outputs':[output]},
          old_plan['path']:{'native_build_receipt':old_build},
          plan['native_input']['path']:{'files':{'weights':weight['path']},
              'source_manifest':source,'numeric_fixture':fixture},
          fixture['path']:{'reference_producer':producer}}
    plan.update(ownership_helper_root=str(helper_root),runtime_files=[pin(ROOT/'literal/runtime.dll')],
        required_native_sources={'crates/native.rs':native_source['sha256']},
        input_seals={x['path']:x['sha256'] for x in [helper,native_source,weight,source,fixture,producer,
                                                   pin(ROOT/'old-build/header.h')]})
    b=SimpleNamespace(pin=lambda path:pin(path),read=lambda path:copy.deepcopy(docs[str(path)]),
        HELPERS={'reference_worker.py':helper['sha256']},MAINTAINED_HELPER_ROOT=helper_root)
    return plan,binding,recipe,raw,b

class Boundary(unittest.TestCase):
    def test_current_closure_excludes_historical_build_tree_and_keeps_live_inputs(self):
        plan,binding,recipe,raw,b=boundary_fixture()
        rows=g.derive_boundary(plan,binding,recipe,raw,b);paths={r['path'] for r in rows}
        self.assertNotIn(str(ROOT/'old-build/header.h'),paths)
        self.assertNotIn(str(ROOT/'old-build/unused.obj'),paths)
        self.assertNotIn(str(ROOT/'old-build/unused.lib'),paths)
        self.assertTrue({str(ROOT/p) for p in ['literal/model.weights','literal/reference.json',
            'crates/native.rs','literal/old-build.json','literal/metadata-plan.json',
            'scripts/cuda-native-execution/helpers/reference_worker.py']}<=paths)
    def test_model_missing_seal_and_native_drift_refuse(self):
        for change in ('missing','native'):
            plan,binding,recipe,raw,b=boundary_fixture()
            if change=='missing':del plan['input_seals'][str(ROOT/'literal/model.weights')]
            else:plan['required_native_sources']['crates/native.rs']='0'*64
            with self.assertRaises(ValueError):g.derive_boundary(plan,binding,recipe,raw,b)
    def test_reopened_roster_must_include_each_derived_current_input(self):
        plan,binding,recipe,raw,b=boundary_fixture();rows=g.derive_boundary(plan,binding,recipe,raw,b)
        known={g.key(r['path']):{'pin':r} for r in rows}
        self.assertEqual(g.derive_boundary(plan,binding,recipe,raw,b,known),rows)
        del known[g.key(ROOT/'literal/model.weights')]
        with self.assertRaises(ValueError):g.derive_boundary(plan,binding,recipe,raw,b,known)
    def test_conflicting_alias_pin_and_oversized_closure_refuse(self):
        plan,binding,recipe,raw,b=boundary_fixture()
        binding['source_pins']=[dict(plan['wrapper'],sha256='0'*64)]
        with self.assertRaises(ValueError):g.derive_boundary(plan,binding,recipe,raw,b)
        plan,binding,recipe,raw,b=boundary_fixture()
        binding['source_pins']=[pin(ROOT/('scripts/source-'+str(i))) for i in range(512)]
        with self.assertRaises(ValueError):g.derive_boundary(plan,binding,recipe,raw,b)

def prepared_fixture():
    plan,binding,recipe,raw,b=boundary_fixture()
    plan.update(schema=c.GRAPH_SCHEMA,selector=c.GRAPH_SELECTOR)
    rows=[{'pin':r,'identity':identity(),'policy':dict(owner_sid=g.SID,sddl='p')}
          for r in g.derive_boundary(plan,binding,recipe,raw,b)]
    prepared=dict(schema=g.SCHEMA,status='PREPARED_UNQUALIFIED',plan=plan,binding=binding,
        qualification=False,product_admission=False,runtime_admitted=False,whole_build_reverified=True,
        handle_close_errors=[],opened_handles=len(rows),closed_handles=len(rows),boundary=rows,
        identity=dict(user(),SessionId=1),started_utc='2026-11-17T00:00:00+00:00',
        finished_utc='2026-11-17T00:00:02+00:00',original_deadline='2026-11-17T00:10:00+00:00',
        started_monotonic=1,finished_monotonic=3,preparation_deadline_monotonic=601,elapsed_seconds=2,
        preparation_source=plan['graph_provenance_owner'],parent_source=plan['wrapper'],
        preserver_source=plan['graph_preserver'],metadata_receipt=plan['metadata_receipt'],
        image_packet=plan['image_packet'])
    return prepared,pin(ROOT/'literal/preparation.json')

class PreparedPlan(unittest.TestCase):
    def test_separate_prepared_plan_drops_only_historical_census_and_pins_receipt(self):
        prepared,p=prepared_fixture();original=copy.deepcopy(prepared)
        plan=g.execution_plan(prepared,p)
        self.assertEqual(prepared,original)
        self.assertNotIn(str(ROOT/'old-build/header.h'),plan['input_seals'])
        self.assertEqual(plan['graph_preparation'],p)
        self.assertEqual(plan['input_seals'][p['path']],p['sha256'])
        self.assertEqual(plan['selector'],c.GRAPH_SELECTOR)
    def test_false_success_changed_source_clock_and_cleanup_refuse(self):
        for field,value in [('whole_build_reverified',False),('qualification',True),
                            ('closed_handles',0),('handle_close_errors',['failed']),
                            ('finished_monotonic',602),('elapsed_seconds',3),
                            ('started_monotonic',True),('preparation_deadline_monotonic',602),
                            ('preparation_source',pin(ROOT/'other'))]:
            prepared,p=prepared_fixture();prepared[field]=value
            with self.subTest(field=field),self.assertRaises(ValueError):g.execution_plan(prepared,p)
    def test_duplicate_boundary_and_preparation_alias_refuse(self):
        prepared,p=prepared_fixture();prepared['boundary'][1]=prepared['boundary'][0]
        with self.assertRaises(ValueError):g.execution_plan(prepared,p)
        prepared,p=prepared_fixture()
        with self.assertRaises(ValueError):g.execution_plan(prepared,prepared['boundary'][0]['pin'])
    def test_plan_changes_refuse_before_lease_acquisition(self):
        prepared,p=prepared_fixture();plan=g.execution_plan(prepared,p);plan['selector']='another'
        with patch.object(g.time,'monotonic',return_value=1),patch.object(g,'read_pinned',return_value=prepared),patch.object(g,'Lease') as lease:
            with self.assertRaises(ValueError):g.open_prepared(plan,None,None,120)
            lease.assert_not_called()
    def test_false_preparation_pin_rejected_before_decoding(self):
        p=pin(ROOT/'receipt.json');b=SimpleNamespace(pin=lambda _:dict(p,sha256='0'*64))
        with patch.object(Path,'read_bytes',side_effect=AssertionError('unapproved read')):
            with self.assertRaises(ValueError):g.read_pinned(p,b)
    def test_non_graph_wrapper_does_not_load_preparation_owner(self):
        with patch.object(c,'run_loaded',return_value={'ok':True}) as run,patch.object(c,'graph_owner',side_effect=AssertionError('wrong route')):
            self.assertEqual(c.run(SimpleNamespace(),placement=True),{'ok':True})
        run.assert_called_once_with(unittest.mock.ANY,diagnostic=False,placement=True)
    def test_graph_cannot_dispatch_without_prepared_custody(self):
        with patch.object(c,'base',return_value=SimpleNamespace(CAMPAIGN=datetime.datetime.max.replace(tzinfo=datetime.timezone.utc),pin=lambda _:None)):
            with self.assertRaisesRegex(ValueError,'retained prepared custody'):c.run_loaded(SimpleNamespace(),graph=True)


def replay_fixture():
    plan,binding,recipe,raw,b=boundary_fixture()
    proof=binding['proof'];proof.update(schema='airk.cuda-static-build-proof/3',
        qualification=False,activation_pins_available=False,cold_build_qualified=False,
        parent_producer=pin(ROOT/'parent',b'parent'),
        parent_aggregate=pin(ROOT/'old-aggregate',b'old'),compile_dependencies=[],
        source_inventory_sha256='d'*64)
    proof['producer_source']=pin(ROOT/'scripts/cuda-static-incremental.py')
    recipe.update(fresh_output=str(ROOT/'fresh'),repo_root=str(ROOT),
        producer_source=proof['producer_source'],compiler_dependencies=[],
        old_producer=proof['parent_producer'],old_aggregate=proof['parent_aggregate'],
        parent_build_proof=proof['parent_build_proof'],source_patch=proof['source_patch'],
        compiler_tool_files=[pin(ROOT/'compiler')])
    raw.update({k:copy.deepcopy(proof[k]) for k in ('producer_source','parent_producer',
        'parent_build_proof','parent_aggregate','source_patch','new_source','new_object',
        'new_session','new_aggregate','session_input_pins','aggregate_input_pins','recipe')})
    raw.update(schema='raw/1',status='unqualified',mode='incremental-finalgraph',
        failure=None,input_recheck_errors=[],limits={'whole_wall_seconds':600},
        qualification=False,cold_build_qualified=False,activation_pins_available=False,
        execution_user_sid=g.SID,default_owner_sid=g.SID,identity=dict(user(),SessionId=1),
        timing_scope='literal prior timing',archive_proof={'aggregate':{'member':'digest'}},
        commands=[{'argv':[str(ROOT/'compiler')],'owned':{'settled':True}}])
    binding.update(schema='airk.private.cuda-finalgraph-image-binding/1',
        preserver_source=plan['graph_preserver'],producer=proof['build_receipt'],
        archive=proof['new_aggregate'],build_receipt=plan['native_build_receipt'],
        manifest=plan['descriptor'],actual_static_ORT_rows=[{'literal':'link'}],
        image_exports={plan['native_image']['sha256']:['placement','graph']})
    binding['source_pins']=[pin(ROOT/'scripts/verifier.py')]
    aggregate=dict(producer_proof_sha256=g.canonical(proof),receipt_sha256=proof['build_receipt']['sha256'],
        plan_sha256=g.canonical(recipe),coff=raw['archive_proof']['aggregate'],
        output=dict(path='aggregate/onnxruntime.lib',bytes=proof['new_aggregate']['bytes'],
                    sha256=proof['new_aggregate']['sha256']),qualification=False)
    binding['aggregate']=aggregate
    parent={'schema':'airk.cuda-static-build-proof/2','build_receipt':proof['parent_producer'],
            'source_inventory_sha256':'d'*64}
    environment=dict(schema='airk.cuda-static-environment/5',build=proof,aggregate=aggregate,
        activation_pins_available=False,environment={'literal':'static'},
        request=dict(destination=recipe['fresh_output'],incremental_root=recipe['fresh_output'],
            incremental_proposal=str(Path(proof['recipe']['path']).parent),
            receipt_sha256=proof['build_receipt']['sha256']))
    documents={binding['environment']['path']:environment,proof['build_receipt']['path']:raw,
        proof['recipe']['path']:recipe,proof['parent_build_proof']['path']:parent,
        plan['descriptor']['path']:{'literal':'native-manifest'},
        binding['source_roster']['path']:{'scripts/verifier.py':pin(ROOT/'scripts/verifier.py')['sha256']}}
    pins={r['path']:r for r in [*plan.values(),*binding.values(),*proof.values()]
          if isinstance(r,dict) and set(r)=={'path','bytes','sha256'}}
    pins.update({r['path']:r for r in binding['source_pins']})
    owner=SimpleNamespace(RAW='raw/1',PASS='unqualified',LIMITS={'whole_wall_seconds':600},
        TIMING='literal prior timing',PARENT=proof['parent_producer']['sha256'],
        AGGREGATE=proof['parent_aggregate']['sha256'],ANCESTRY=('source_inventory_sha256',),
        validate_recipe=Mock(return_value=Path(recipe['fresh_output'])),validate_clocks=Mock(),
        validate_parent_toolchain=Mock(),settled=Mock(),canonical=g.canonical,
        verify_incremental_bundle=Mock(side_effect=AssertionError('full historical verifier called')))
    environment_owner=SimpleNamespace(environment=Mock(return_value={'literal':'static'}),
        verify_handoff=Mock(side_effect=AssertionError('full handoff verifier called')))
    preserver=SimpleNamespace(PROVENANCE_OWNERS=('scripts/verifier.py',),
        validate_graph_manifest=Mock(return_value=binding['runtime_producer']),
        cargo_rows=Mock(return_value=([{'literal':'link'}],[])),
        exported_collectors=Mock(return_value=['placement','graph']))
    data={plan['graph_output_guard']['path']:b'# SPDX-License-Identifier: Apache-2.0\n'+b'guard',
          plan['output_guard']['path']:b'guard',binding['cargo_json']['path']:b'cargo-json'}
    lease=SimpleNamespace(deadline=120,pin=lambda path,**kw:pins[str(path)],
        json=lambda path:documents[str(path)],bytes=lambda path,*args:data[str(path)],
        load=lambda path,name:owner if Path(path).name=='cuda-static-incremental.py' else environment_owner)
    return plan,binding,lease,preserver,documents,owner,environment_owner

class Replay(unittest.TestCase):
    def test_prepared_execution_replays_relationships_without_full_historical_verifier(self):
        plan,binding,lease,preserver,docs,owner,environment=replay_fixture()
        with patch.object(g.time,'monotonic',return_value=1):g.replay(plan,binding,lease,preserver)
        owner.validate_clocks.assert_called_once();owner.settled.assert_called_once()
        owner.verify_incremental_bundle.assert_not_called();environment.verify_handoff.assert_not_called()
        preserver.exported_collectors.assert_called_once_with(plan['native_image']['path'],deadline=120)
    def test_changed_native_manifest_cargo_exports_and_preserver_refuse(self):
        for kind in ('manifest','cargo','exports','preserver'):
            plan,binding,lease,p,docs,owner,env=replay_fixture()
            if kind=='manifest':p.validate_graph_manifest.return_value=pin(ROOT/'wrong-producer')
            if kind=='cargo':p.cargo_rows.return_value=([{'wrong':'link'}],[])
            if kind=='exports':p.exported_collectors.return_value=['placement']
            if kind=='preserver':plan['graph_preserver']=pin(ROOT/'wrong.py')
            with patch.object(g.time,'monotonic',return_value=1),self.subTest(kind=kind),self.assertRaises(ValueError):
                g.replay(plan,binding,lease,p)
    def test_producer_mode_identity_and_environment_retarget_refuse(self):
        for kind in ('mode','identity','timing','environment','recipe','aggregate'):
            plan,binding,lease,p,docs,owner,env=replay_fixture()
            raw=docs[binding['proof']['build_receipt']['path']]
            if kind=='mode':raw['mode']='other'
            if kind=='identity':raw['default_owner_sid']='foreign'
            if kind=='timing':raw['timing_scope']='extended'
            if kind=='environment':docs[binding['environment']['path']]['request']['incremental_root']=str(ROOT/'other')
            if kind=='recipe':docs[binding['proof']['recipe']['path']]['old_producer']=pin(ROOT/'foreign')
            if kind=='aggregate':binding['aggregate']['producer_proof_sha256']='0'*64
            with patch.object(g.time,'monotonic',return_value=1),self.subTest(kind=kind),self.assertRaises(ValueError):
                g.replay(plan,binding,lease,p)
    def test_original_deadline_reaches_replay_and_late_completion_refuses(self):
        plan,binding,lease,p,docs,owner,env=replay_fixture()
        with patch.object(g.time,'monotonic',return_value=120),self.assertRaises(ValueError):
            g.replay(plan,binding,lease,p)
        p.exported_collectors.assert_not_called()

class WrapperLifecycle(unittest.TestCase):
    def test_pre_dispatch_or_native_failure_closes_graph_boundary(self):
        for failure in ('pre-dispatch','native'):
            events=[]
            class Lease:
                def __enter__(self):events.append('held');return self
                def __exit__(self,*args):events.append('closed')
            gp=SimpleNamespace(open_prepared=Mock(return_value=Lease()))
            plan={'schema':c.GRAPH_SCHEMA,'selector':c.GRAPH_SELECTOR}
            b=SimpleNamespace(CAMPAIGN=datetime.datetime.max.replace(tzinfo=datetime.timezone.utc),
                              pin=lambda path:pin(path),read=lambda path:plan)
            args=SimpleNamespace(plan=ROOT/'plan.json',plan_sha256=pin(ROOT/'plan.json')['sha256'])
            def execute(*args,**kwargs):
                self.assertEqual(events,['held'])
                self.assertIsNotNone(kwargs['graph_context'])
                raise ValueError(failure)
            with patch.object(c,'base',return_value=b),patch.object(c,'graph_owner',return_value=gp),patch.object(c,'run_loaded',side_effect=execute):
                with self.assertRaisesRegex(ValueError,failure):c.run(args,graph=True)
            self.assertEqual(events,['held','closed'])
    def test_final_close_failure_does_not_erase_native_failure(self):
        receipt={'complete':False,'failure':{'message':'native failed'}}
        def close():raise OSError('close failed')
        c.finish(receipt,{},0,120,lambda _:None,clock=lambda:2,proofs={},finalizer=close)
        self.assertFalse(receipt['complete']);self.assertEqual(receipt['failure']['message'],'native failed')


class PreparationLifecycle(unittest.TestCase):
    def test_full_preparation_failure_closes_inputs_and_emits_no_receipt(self):
        plan,binding,recipe,raw,b=boundary_fixture();path=ROOT/'literal-held';files=[]
        def opened(p):
            value=Opened(p);files.append(value);return value
        owner=SimpleNamespace(guard=Mock(),preflight=Mock(side_effect=ValueError('ancestry refusal')),write_new=Mock())
        with patch.object(g.time,'monotonic',return_value=1),patch.object(Path,'exists',return_value=False),patch.object(g,'read_pinned',side_effect=[{'static_graph_binding':binding},recipe,raw]),patch.object(g,'derive_boundary',return_value=[pin(path)]):
            with self.assertRaisesRegex(ValueError,'ancestry refusal'):
                g.prepare(plan,b,ROOT/'prepared.json',owner,0,datetime.datetime.now(datetime.timezone.utc),600,opener=opened,observe=policy)
        owner.preflight.assert_called_once_with(plan,b,before=True,deadline=600)
        owner.write_new.assert_not_called();self.assertTrue(files[0].closed)
    def test_extra_boundary_and_omission_refuse_before_acquisition(self):
        for extra in (True,False):
            prepared,p=prepared_fixture();plan0,binding,recipe,raw,b=boundary_fixture()
            if extra:
                prepared['boundary'].append(dict(pin=pin(ROOT/'extra'),identity=identity(),policy={}))
            else:
                target=str(ROOT/'literal/model.weights')
                prepared['boundary']=[r for r in prepared['boundary'] if r['pin']['path']!=target]
            prepared['opened_handles']=prepared['closed_handles']=len(prepared['boundary'])
            plan=g.execution_plan(prepared,p)
            with patch.object(g.time,'monotonic',return_value=1),patch.object(g,'read_pinned',side_effect=[prepared,recipe,raw]),patch.object(g,'Lease') as acquire:
                with self.assertRaises(ValueError):g.open_prepared(plan,None,b,120)
                acquire.assert_not_called()
    def test_initial_observer_identity_and_policy_roster_refuse(self):
        for kind in ('identity','roster'):
            files=[]
            def opened(p):
                f=Opened(p);files.append(f);return f
            def observe(*args):
                value=policy(*args)
                if kind=='identity':value['identity']['OwnerSid']='foreign'
                else:value['policies']['extra']={}
                return value
            with patch.object(g.time,'monotonic',return_value=1),self.assertRaises(ValueError):
                g.Lease({},[pin(ROOT/'input')],120,opener=opened,observe=observe)
            self.assertTrue(files[0].closed)

class HeldInterfaces(unittest.TestCase):
    def test_actual_pe_reader_uses_retained_handle_not_path_reopen(self):
        controls=module('graph_pe_fixture',ROOT/'scripts/tests/test_preserve_cuda_test_images.py')
        data=controls.IncrementalPreservation().image()
        actual=module('graph_pe_owner',ROOT/'scripts/preserve-cuda-test-images.py')
        image=ROOT/'literal-image.exe';opened=Opened(image,data)
        lease=SimpleNamespace(deadline=120,files={g.key(image):opened},
            pin=lambda path,**kwargs:pin(path,data),load=lambda *args:actual)
        with patch.object(g.time,'monotonic',return_value=1):
            reader=g.held_preserver(lease,{'graph_preserver':pin(ROOT/'scripts/preserve-cuda-test-images.py')})
            with patch.object(Path,'open',side_effect=AssertionError('path reopen forbidden')):
                self.assertEqual(reader.exported_collectors(image,deadline=120),
                    ['airk_ort_collect_final_graph_metadata_v1','airk_ort_collect_final_placement_v1'])
        self.assertFalse(opened.closed)
    def test_output_policy_uses_pinned_shell_and_remaining_original_cutoff(self):
        owner=SimpleNamespace()
        lease=SimpleNamespace(deadline=120,load=lambda *args:owner,digest=lambda path:'a'*64)
        plan={'graph_output_guard':pin(ROOT/'guard.py'),'powershell':pin(ROOT/'pinned-pwsh.exe')}
        with patch.object(g.time,'monotonic',return_value=119),patch.object(g.subprocess,'run',return_value='result') as launch:
            guarded=g.output_policy_owner(lease,plan)
            self.assertEqual(guarded.subprocess.run(['pwsh','-NoProfile'],timeout=15),'result')
            self.assertEqual(launch.call_args.args[0][0],str(ROOT/'pinned-pwsh.exe'))
            self.assertEqual(launch.call_args.kwargs['timeout'],1)
        with patch.object(g.time,'monotonic',return_value=120),patch.object(g.subprocess,'run') as launch:
            with self.assertRaises(ValueError):guarded.subprocess.run(['pwsh'],timeout=15)
            launch.assert_not_called()

if __name__=='__main__':unittest.main()
