# SPDX-License-Identifier: Apache-2.0
"""Pure controls; no subprocess/model/compiler or temporary outputs."""
import copy
import importlib.util
import json
from pathlib import Path
import types
import unittest
from unittest.mock import patch

HERE=Path(__file__).resolve().parents[2]
SPEC=importlib.util.spec_from_file_location('incremental_owner',HERE/'scripts/cuda-static-incremental.py')
M=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)
RECIPE=Path(__file__).resolve()

def fixture():
    root=Path(Path(__file__).resolve().anchor)/'fixture'
    out=root/'fresh'
    base=str(root/'source/onnxruntime_c_api.cc')
    def row(path): return dict(path=str(path),bytes=1,sha256='a'*64)
    objects=[row(root/'old'/(str(i)+'.obj')) for i in range(39)]+[row(root/'old/onnxruntime_c_api.cc.obj')]
    libs=[row(root/'old'/(str(i)+'.lib')) for i in range(95)]+[row(root/'old/onnxruntime_session.lib')]
    original=[str(root/'cl.exe'),'/O2','-I'+str(root/'include'),'/Fo'+str(root/'old/a.obj'),
              '/Fd'+str(root/'old/a.pdb'),'/showIncludes','-c',base]
    actual=[str(root/'cl.exe'),'/O2','/I'+str(root/'source'),'-I'+str(root/'include'),
            '/Fo'+str(out/'objects/onnxruntime_c_api.cc.obj'),
            '/Fd'+str(out/'objects/onnxruntime_session.pdb'),'/showIncludes','-c',
            str(out/'source/onnxruntime_c_api.cc')]
    session=[str(root/'lib.exe'),'/nologo','/machine:x64','/OUT:'+str(out/'session/onnxruntime_session.lib'),
             '@'+str(out/'session-inputs.rsp')]
    aggregate=[str(root/'lib.exe'),'/NOLOGO','/OUT:'+str(out/'aggregate/onnxruntime.lib'),
               '@'+str(out/'aggregate-inputs.rsp')]
    result=dict(fresh_output=str(out),old_producer=dict(sha256='f9fac8eb2fe1a1cb5191118124503782792ca6d3aff14d02326167cd7276b178'),
        old_aggregate=dict(sha256='042c970193bf3d2fc7c80f53b055f74130c653d561f7735142fc01b01cebac00'),
        proposed_limits=dict(whole_wall_seconds=600,compile_wall_seconds=120,session_wall_seconds=60,
            aggregate_wall_seconds=300,proof_wall_seconds=120,process_bytes=17179869184,
            job_bytes=25769803776,log_bytes=67108864,cleanup_seconds=5,parallelism=1),
        compiler_dependencies=[row(root/'headers'/(str(i)+'.h')) for i in range(482)],
        session_objects=objects,aggregate_libraries=libs,
        session_input_paths=[x['path'] for x in objects[:-1]]+[str(out/'objects/onnxruntime_c_api.cc.obj')],
        aggregate_input_paths=[x['path'] for x in libs[:-1]]+[str(out/'session/onnxruntime_session.lib')],
        original_compile_argv=original,base_source=row(base),
        commands=dict(compile=actual,session=session,aggregate=aggregate,
            object_symbols=[str(root/'dumpbin.exe'),'/symbols',str(out/'objects/onnxruntime_c_api.cc.obj')],
            aggregate_directives=[str(root/'dumpbin.exe'),'/directives',str(out/'aggregate/onnxruntime.lib')]),
        required_exports=['airk_ort_collect_final_placement_v1','airk_ort_collect_final_graph_metadata_v1'],
        source_patch=dict(base=row(base),after=row(root/'after.cc'),patch=row(root/'patch')),
        new_source_sha256='a'*64,new_source_bytes=1,approved_new_dependencies=[],
        compiler_tool_files=[row(root/name) for name in ('cl.exe','lib.exe','dumpbin.exe')],
        environment=dict(CL='',_CL_='',VSLANG='1033',TEMP=str(out/'tmp'),TMP=str(out/'tmp')),
        response_files=[])
    for name,paths in [('session-inputs.rsp',result['session_input_paths']),
                       ('aggregate-inputs.rsp',result['aggregate_input_paths'])]:
        data=('\r\n'.join('"'+p+'"' for p in paths)+'\r\n').encode()
        result['response_files'].append(dict(name=name,future_path=str(out/name),
            bytes=len(data),sha256=M.hashlib.sha256(data).hexdigest()))
    return result


class JsonTests(unittest.TestCase):
    def test_duplicate_refused(self):
        with self.assertRaises(ValueError): M.parse_json(b'{"x":1,"x":2}')
    def test_nested_duplicate_refused(self):
        with self.assertRaises(ValueError): M.parse_json(b'{"a":{"x":1,"x":2}}')
    def test_nonfinite_refused(self):
        for value in ('NaN','Infinity','-Infinity'):
            with self.subTest(value=value), self.assertRaises(ValueError): M.parse_json(value)
    def test_valid(self):
        self.assertEqual(M.parse_json('{"a":[1,false,null]}'),{'a':[1,False,None]})

class RecipeTests(unittest.TestCase):
    def setUp(self): self.r=fixture()
    def test_exact_frozen_recipe(self): M.validate_recipe(self.r)
    def reject(self):
        with self.assertRaises(ValueError): M.validate_recipe(self.r)
    def test_omitted_header(self): self.r['compiler_dependencies'].pop(); self.reject()
    def test_duplicate_header(self): self.r['compiler_dependencies'][1]=self.r['compiler_dependencies'][0]; self.reject()
    def test_wrong_old_parent(self): self.r['old_producer']['sha256']='0'*64; self.reject()
    def test_wrong_old_aggregate(self): self.r['old_aggregate']['sha256']='0'*64; self.reject()
    def test_second_replaced_object(self): self.r['session_input_paths'][0]='foreign.obj'; self.reject()
    def test_library_omission(self): self.r['aggregate_input_paths'].pop(); self.reject()
    def test_original_output_overwrite(self): self.r['commands']['compile'][-1]=self.r['base_source']['path']; self.reject()
    def test_changed_compiler_flag(self): self.r['commands']['compile'].insert(1,'/Od'); self.reject()
    def test_second_source(self): self.r['commands']['compile'].append('extra.cc'); self.reject()
    def test_environment_injection(self): self.r['environment']['CL']='/Od'; self.reject()
    def test_raised_limit(self): self.r['proposed_limits']['process_bytes']+=1; self.reject()
    def test_provider_import_aggregate(self): self.r['aggregate_input_paths'][0]='provider.lib'; self.reject()
    def test_unapproved_headers(self): self.r['approved_new_dependencies']=[{}]; self.reject()
    def test_patch_base_lie(self): self.r['source_patch']['base']={}; self.reject()

class PhaseTests(unittest.TestCase):
    def test_audit_then_include_before_next_consumer(self):
        events=[]
        with patch.object(M.time,'monotonic',return_value=1):
            self.assertEqual(M.complete_stage('compile',lambda:events.append('audit'),
                lambda:events.append('includes') or [1],2),[1])
        events.append('next')
        self.assertEqual(events,['audit','includes','next'])
    def test_audit_failure_stops_include(self):
        events=[]
        def fail(): raise ValueError('owner')
        with self.assertRaises(ValueError):
            M.complete_stage('compile',fail,lambda:events.append('read'),2)
        self.assertEqual(events,[])
    def test_include_failure_stops_next_phase(self):
        events=[]
        def fail(): raise ValueError('unfrozen header')
        with self.assertRaises(ValueError):
            M.complete_stage('compile',lambda:events.append('audit'),fail,2)
            events.append('next')
        self.assertEqual(events,['audit'])
    def test_noncompile_does_not_parse_includes(self):
        with patch.object(M.time,'monotonic',return_value=1):
            self.assertIsNone(M.complete_stage('session',lambda:None,
                lambda:self.fail('unexpected parse'),2))
    def test_expired_final_observation_refuses(self):
        with patch.object(M.time,'monotonic',return_value=2), self.assertRaises(ValueError):
            M.complete_stage('compile',lambda:None,lambda:[],2)
    def test_deadline_reader_chunk_recheck_and_global_path_unchanged(self):
        fake=types.SimpleNamespace(Path=Path)
        original=Path.open
        bounded=M.bound_module(fake,2)
        with patch.object(M.time,'monotonic',return_value=1):
            with bounded.Path(RECIPE).open('rb') as f:
                self.assertTrue(f.read(1))
                with patch.object(M.time,'monotonic',return_value=2),self.assertRaises(ValueError):
                    f.read(1)
        self.assertIs(Path.open,original)
    def test_historical_writer_refused(self):
        fake=M.bound_module(types.SimpleNamespace(Path=Path),2)
        with self.assertRaises(ValueError): fake.Path(RECIPE).open('wb')

def owned():
    return dict(exit_code=0,timed_out=False,log_oversized=False,owned_descendants_settled=True,
        owned_job_forced_termination=False,owned_job_final_active_processes=0,owned_job_final_member_pids=[],
        parent_assigned_to_job=False,child_assigned_before_resume=True,unexpected_third_process=False,
        whole_process_deadline_not_refreshed=True,owned_total_processes=2,owned_leader_pid=123,
        peak_host_committed_bytes=1000,retained_member_exits=[dict(exit_observed=True,exit_code=0)],
        handle_close_proof=dict(forced=False,settled=True,final_active=0,members_before=[],final_member_pids=[],
            retained_exits=[dict(pid=123,exit_observed=True,exit_code=0)]),
        suspended_assignment=dict(pid=123,execution_user_sid=M.SID,default_owner_sid=M.SID,
            image=dict(path='compiler',sha256='a'*64),
            component_job=dict(process_bytes=16<<30,job_bytes=24<<30,flags=8968,active_limit=2)))

class OwnershipTests(unittest.TestCase):
    def setUp(self): self.value=owned(); self.exe=dict(path='compiler',bytes=4,sha256='a'*64)
    def reject(self):
        with self.assertRaises(ValueError): M.settled(self.value,self.exe)
    def test_actual_settlement(self): M.settled(self.value,self.exe)
    def test_natural_one_process(self): self.value['owned_total_processes']=1; M.settled(self.value,self.exe)
    def test_third_process(self): self.value['owned_total_processes']=3; self.reject()
    def test_forced(self): self.value['owned_job_forced_termination']=True; self.reject()
    def test_missing_handle(self): self.value['handle_close_proof']['retained_exits']=[]; self.reject()
    def test_wrong_handle_pid(self): self.value['handle_close_proof']['retained_exits'][0]['pid']=124; self.reject()
    def test_unsettled(self): self.value['owned_descendants_settled']=False; self.reject()
    def test_nonzero(self): self.value['exit_code']=1; self.reject()
    def test_owner_mismatch(self): self.value['suspended_assignment']['default_owner_sid']='foreign'; self.reject()
    def test_bounds_mismatch(self): self.value['suspended_assignment']['component_job']['job_bytes']+=1; self.reject()
    def test_bool_as_count(self): self.value['owned_total_processes']=True; self.reject()
    def test_image_drift(self): self.value['suspended_assignment']['image']['sha256']='b'*64; self.reject()


class ClockTests(unittest.TestCase):
    def setUp(self):
        self.r=fixture()
        self.v=dict(started_monotonic=10.,finished_monotonic=200.,original_deadline_monotonic=610.,
            elapsed_seconds=190.,started_utc='2026-11-17T00:00:00+00:00',
            finished_utc='2026-11-17T00:03:10+00:00',original_deadline='2026-11-17T00:10:00+00:00',commands=[])
        for index,(stage,cap) in enumerate(zip(
            ['compile','session','aggregate','object-symbols','aggregate-directives'],[120,60,300,120,120])):
            a=20.+index*20
            self.v['commands'].append(dict(stage=stage,argv=self.r['commands'][stage.replace('-','_')],
                cwd=self.r['fresh_output'],started_monotonic=a,finished_monotonic=a+1,
                absolute_deadline_monotonic=min(610.,a+cap),phase_limit_seconds=cap,
                started_utc='2026-11-17T00:00:01+00:00',finished_utc='2026-11-17T00:00:02+00:00'))
    def reject(self):
        with self.assertRaises(ValueError): M.validate_clocks(self.v,self.r)
    def test_clock_exact(self): M.validate_clocks(self.v,self.r)
    def test_refreshed_whole_deadline(self): self.v['original_deadline_monotonic']=611.; self.reject()
    def test_refreshed_phase(self): self.v['commands'][0]['absolute_deadline_monotonic']+=1; self.reject()
    def test_incomplete_commands(self): self.v['commands'].pop(); self.reject()
    def test_expired_final_clock(self): self.v['finished_monotonic']=610.; self.v['elapsed_seconds']=600.; self.reject()
    def test_nan_clock(self): self.v['started_monotonic']=float('nan'); self.reject()
    def test_wrong_command(self): self.v['commands'][0]['argv']=[]; self.reject()
    def test_wrong_cwd(self): self.v['commands'][0]['cwd']='foreign'; self.reject()
    def test_overlapping_phases(self): self.v['commands'][1]['started_monotonic']=19.; self.reject()

class IncludeTests(unittest.TestCase):
    def setUp(self):
        self.r=fixture()
        self.log=('\n'.join('Note: including file: '+x['path'] for x in self.r['compiler_dependencies'])+'\n').encode()
    def test_exact_complete_roster(self):
        self.assertEqual(len(M.includes(self.log,self.r)),482)
    def test_missing_header(self):
        with self.assertRaises(ValueError): M.includes(b'\n'.join(self.log.splitlines()[:-1]),self.r)
    def test_unapproved_header(self):
        with self.assertRaises(ValueError): M.includes(self.log+b'Note: including file: unexpected.h\n',self.r)
    def test_real_compiler_repeated_include_is_one_dependency(self):
        self.assertEqual(len(M.includes(self.log+self.log,self.r)),482)
    def test_invalid_encoding(self):
        with self.assertRaises(UnicodeError): M.includes(self.log+b'\xff',self.r)

class ReproductionTests(unittest.TestCase):
    def generator(self):
        spec=importlib.util.spec_from_file_location('freeze_reproduction',HERE/'scripts/cuda-static-incremental/freeze_inputs.py')
        module=importlib.util.module_from_spec(spec)
        with patch('subprocess.run',side_effect=AssertionError('import dispatched a tool')):
            spec.loader.exec_module(module)
        return module
    def test_generator_import_has_no_execution(self):
        self.assertTrue(callable(self.generator().generate))
    def test_generator_query_is_readonly_metadata_with_finite_bound(self):
        g=self.generator();g.NINJA=Path('ninja');g.B=Path('build')
        with patch.object(g.subprocess,'run',return_value=types.SimpleNamespace(stdout='result')) as call:
            self.assertEqual(g.query('deps','object'),'result')
        self.assertEqual(call.call_args.args[0],['ninja','-C','build','-t','deps','object'])
        self.assertEqual(call.call_args.kwargs['timeout'],30)
        self.assertIs(call.call_args.kwargs['check'],True)
    def test_wrong_seed_hash_refused_before_tool(self):
        g=self.generator()
        with patch.object(g,'pin',return_value={'sha256':'wrong'}),patch.object(g.subprocess,'run') as call:
            with self.assertRaises(ValueError): g.seed_inputs('seed','expected')
            call.assert_not_called()
    def test_external_verifier_deadline_refused_before_input_access(self):
        with patch.object(M,'pin',side_effect=AssertionError('late input access')):
            with self.assertRaises(ValueError): M.verify_incremental('x','x','x','x','x',deadline=0)
    def test_external_aggregate_deadline_refused_before_input_access(self):
        with patch.object(M,'check_pin',side_effect=AssertionError('late input access')):
            with self.assertRaises(ValueError): M.verify_aggregate('x',{},None,deadline=0)

class VerificationDeadlineTests(unittest.TestCase):
    def test_nonfinite_deadline_refused_before_read(self):
        with self.assertRaises(ValueError): M.verify_incremental('x','x','x','x','x',deadline=float('inf'))
    def test_external_deadline_cannot_widen_maximum(self):
        with patch.object(M.time,'monotonic',return_value=10), self.assertRaises(ValueError):
            M.verify_aggregate('x',{},None,deadline=611)

class ParentToolchainTests(unittest.TestCase):
    def setUp(self):
        self.r=fixture()
        self.r['ninja']=dict(path='ninja',bytes=1,sha256='b'*64)
        self.r['vcvars']=dict(path='vcvars',bytes=1,sha256='c'*64)
        self.r['environment'].update(VCTOOLSVERSION='14.44.35207',WINDOWSSDKVERSION='10.0.26100.0\\')
        self.parent=dict(toolchain_pins=[self.r['compiler_tool_files'][0],{},self.r['ninja'],{},{},self.r['vcvars']])
    def test_exact_parent_toolchain(self): M.validate_parent_toolchain(self.r,self.parent)
    def test_changed_compiler_cannot_inherit_old_toolchain(self):
        self.parent['toolchain_pins'][0]=dict(self.parent['toolchain_pins'][0],sha256='d'*64)
        with self.assertRaises(ValueError): M.validate_parent_toolchain(self.r,self.parent)
    def test_changed_sdk_refused(self):
        self.r['environment']['WINDOWSSDKVERSION']='different'
        with self.assertRaises(ValueError): M.validate_parent_toolchain(self.r,self.parent)

class BundleTests(unittest.TestCase):
    def test_one_actual_verification_supplies_both_proofs(self):
        build={'target':'x86_64-pc-windows-msvc','build_receipt':{'path':'raw','bytes':3,'sha256':'a'*64},
               'recipe':{'path':'recipe','bytes':4,'sha256':'b'*64},'new_aggregate':{'bytes':9,'sha256':'c'*64}}
        with patch.object(M.time,'monotonic',return_value=1),patch.object(M,'verify_incremental',return_value=build) as verify,\
             patch.object(M,'check_pin',side_effect=lambda row,deadline:row['path']),\
             patch.object(M,'read',side_effect=[{'archive_proof':{'aggregate':{'members':2}}},{'recipe':'data'}]):
            result=M.verify_incremental_bundle('out','out','proposal','a'*64,'repo',deadline=100)
        verify.assert_called_once_with('out','out','proposal','a'*64,'repo',deadline=100)
        self.assertEqual(result['build'],build)
        self.assertEqual(result['aggregate']['producer_proof_sha256'],M.canonical(build))
        self.assertEqual(result['aggregate']['receipt_sha256'],'a'*64)
        self.assertEqual(result['aggregate']['coff'],{'members':2})
        self.assertIs(result['aggregate']['qualification'],False)
    def test_late_full_verification_cannot_project_pass(self):
        with patch.object(M.time,'monotonic',return_value=1) as clock,patch.object(M,'read') as read:
            def late(*args,**kwargs):clock.return_value=2;return {}
            with patch.object(M,'verify_incremental',side_effect=late),self.assertRaises(ValueError):
                M.verify_incremental_bundle('out','out','proposal','sha','repo',deadline=2)
            read.assert_not_called()

if __name__=='__main__':
    unittest.main()
