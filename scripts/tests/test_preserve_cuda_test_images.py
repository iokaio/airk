# SPDX-License-Identifier: Apache-2.0
"""Literal/in-memory CUDA image custody refusals; no file, native or GPU work."""
import copy
import importlib.util
import io
import json
from pathlib import Path
import unittest
import struct
from types import SimpleNamespace
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('preserve_cuda_images', Path(__file__).resolve().parents[1]/'preserve-cuda-test-images.py')
m = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(m)


def receipt():
    return {'schema':'airk-readiness-command/1','status':'completed','passed':True,'exit_code':0,'runner_exit_code':0,'inputs_unchanged':True,'runner_unchanged':True,'runtime_unchanged':True,'command_inputs_unchanged':True,'changed_groups':[],'groups':{'native':{'sha256':'a'*64}},'incremental':'profile default; environment unset','log':'actual.log','command':['cargo','test','-p','airk-retrieval','-p','airk-inference','--lib','--all-features','--locked','--offline','--no-run','--message-format=json','--timings']}


def cargo():
    rows = [{'reason':'build-finished','success':True}, {'reason':'build-script-executed','package_id':'registry#ort-sys@2.0.0-rc.10','linked_libs':['static=onnxruntime'],'linked_paths':['native='+str(m.ARCHIVE.parent)]}]
    for crate in ['airk-inference','airk-retrieval']:
        rows.append({'reason':'compiler-artifact','profile':{'test':True},'executable':'literal.exe','target':{'kind':['lib'],'name':crate.replace('-','_')},'manifest_path':str(m.ROOT/'crates'/crate/'Cargo.toml'),'features':['gpu-cuda']})
    return rows


def encoded(rows):
    return '\n'.join(json.dumps(row) for row in rows)


def custody(name='image.exe', file_id=10):
    return {'pin':{'path':str(Path('C:/literal')/name),'bytes':12,'sha256':'a'*64},'identity':{'volume':1,'file_id':file_id,'links':1,'bytes':12,'mtime_ns':123},'policy':{'owner_sid':m.SID,'sddl':'literal','rules':[]}}


class PreservationControls(unittest.TestCase):
    def test_exact_current_receipt_accepts_observed_context(self):
        m.validate_receipt(receipt(), 'a'*64, 'actual.log')

    def test_failed_or_changed_receipt_refuses(self):
        for key in ['passed','inputs_unchanged','runner_unchanged','runtime_unchanged','command_inputs_unchanged']:
            value=receipt(); value[key]=False
            with self.assertRaises(ValueError): m.validate_receipt(value,'a'*64,'actual.log')
        for key,value in [('status','started'),('exit_code',1),('runner_exit_code',1),('changed_groups',['native'])]:
            r=receipt(); r[key]=value
            with self.assertRaises(ValueError): m.validate_receipt(r,'a'*64,'actual.log')

    def test_no_old_incremental_or_command_reinterpretation(self):
        for change in ['old-incremental','missing-offline','wrong-group','wrong-log']:
            value=receipt()
            if change=='old-incremental': value['incremental']='0'
            elif change=='missing-offline': value['command'].remove('--offline')
            elif change=='wrong-group': value['groups']['native']['sha256']='b'*64
            else: value['log']='old.log'
            with self.assertRaises(ValueError): m.validate_receipt(value,'a'*64,'actual.log')

    def test_exact_two_gpu_artifact_and_static_link_rows(self):
        ort, rows=m.cargo_rows(encoded(cargo()))
        self.assertEqual([name for name,_ in rows],['airk-inference','airk-retrieval'])
        self.assertEqual(len(ort),1)

    def test_absent_duplicate_or_failed_finish_refuses(self):
        for finish in [[],[{'reason':'build-finished','success':False}],[{'reason':'build-finished','success':True}]*2]:
            with self.assertRaises(ValueError): m.cargo_rows(encoded(finish+cargo()[1:]))

    def test_mixed_static_link_or_ort_version_refuses(self):
        for key,value in [('linked_libs',['dylib=onnxruntime']),('linked_paths',['native=C:/other']),('package_id','registry#ort-sys@2.0.0-rc.9')]:
            rows=cargo(); rows[1][key]=value
            with self.assertRaises(ValueError): m.cargo_rows(encoded(rows))

    def test_duplicate_missing_or_cpu_artifact_refuses(self):
        for mutation in ['missing','duplicate','cpu','wrong-manifest']:
            rows=cargo()
            if mutation=='missing': rows.pop()
            elif mutation=='duplicate': rows.append(copy.deepcopy(rows[-1]))
            elif mutation=='cpu': rows[-1]['features']=[]
            else: rows[-1]['manifest_path']=str(m.ROOT/'crates/other/Cargo.toml')
            with self.assertRaises(ValueError): m.cargo_rows(encoded(rows))

    def test_payload_trusted_principals_do_not_extend_inventory_policy(self):
        policy={'owner_sid':m.SID,'rules':[{'sid':'S-1-5-18','type':'Allow','rights':2032127}]}
        m.owner_policy(policy,payload=True)
        with self.assertRaises(ValueError): m.owner_policy(policy)

    def test_foreign_owner_and_foreign_write_refuse(self):
        for policy in [{'owner_sid':'foreign','rules':[]},{'owner_sid':m.SID,'rules':[{'sid':'foreign','type':'Allow','rights':2032127}]}]:
            with self.assertRaises(ValueError): m.owner_policy(policy,payload=True)

    def test_foreign_readonly_does_not_become_foreign_write(self):
        m.owner_policy({'owner_sid':m.SID,'rules':[{'sid':'foreign','type':'Allow','rights':0x1200a9}]},payload=True)

    def test_only_one_declared_runtime_addition_permitted(self):
        before=[{'name':'old.exe',**custody('old.exe')}]
        extra={'name':'new.exe',**custody('new.exe',11)}
        m.unchanged_runtime(before,before)
        m.unchanged_runtime(before,[extra,*before],extra)
        for after in [[],[extra],before+[extra,extra]]:
            with self.assertRaises(ValueError): m.unchanged_runtime(before,after,extra)

    def test_preexisting_runtime_content_stat_acl_drift_refuses(self):
        before=[{'name':'old.exe',**custody('old.exe')}]
        for section,key,value in [('pin','sha256','b'*64),('identity','file_id',99),('identity','mtime_ns',99),('policy','sddl','changed')]:
            after=copy.deepcopy(before); after[0][section][key]=value
            with self.assertRaises(ValueError): m.unchanged_runtime(before,after)

    def test_manifest_producer_legal_and_dll_roster_required(self):
        rows=[{'name':'airk-cuda-runtime.json','pin':{'sha256':m.MANIFEST_SHA}}, {'name':'gpu-checkpointed-build-receipt.json'}, {'name':'airk-cuda-notice-cuda-eula.txt'}, {'name':'provider.dll'}]
        m.validate_runtime(rows)
        for i in range(len(rows)):
            with self.assertRaises(ValueError): m.validate_runtime(rows[:i]+rows[i+1:])
        rows[0]['pin']['sha256']='b'*64
        with self.assertRaises(ValueError): m.validate_runtime(rows)

    def test_census_binds_old_executable_custody_not_only_names(self):
        rows=[{'name':'old.exe',**custody('old.exe')}]
        changed=copy.deepcopy(rows); changed[0]['policy']['sddl']='changed'
        self.assertNotEqual(m.census_digest(rows),m.census_digest(changed))
        self.assertEqual(m.census_digest(rows),m.census_digest(copy.deepcopy(rows)))

    def test_copy_audits_destination_before_read_and_preserves_original(self):
        before=custody(); after=custody('new.exe',11); events=[]
        def observe(path):
            events.append(('snapshot',str(path)))
            return after if str(path).endswith('new.exe') else before
        def guard(path,root,sha,created=False): events.append(('guard',created))
        with patch.object(m,'snapshot',side_effect=observe), patch.object(m,'guard',side_effect=guard), patch.object(m.Path,'open',side_effect=lambda *a,**k:io.BytesIO()), patch.object(m.shutil,'copyfileobj'):
            result=m.copy_one(before,Path('C:/literal/new.exe'),Path('C:/literal'),'x')
        self.assertEqual(result,after)
        self.assertEqual(events[1:4],[('guard',False),('guard',True),('snapshot',str(Path('C:/literal/new.exe')))])
        self.assertEqual(events[-1],('snapshot',before['pin']['path']))

    def test_copy_rejects_identical_file_id_or_corrupt_bytes(self):
        before=custody()
        for mutation in ['same-id','bytes']:
            after=custody('new.exe',10 if mutation=='same-id' else 11)
            if mutation=='bytes': after['pin']['sha256']='b'*64
            with patch.object(m,'snapshot',side_effect=[before,after]), patch.object(m,'guard'), patch.object(m.Path,'open',side_effect=lambda *a,**k:io.BytesIO()), patch.object(m.shutil,'copyfileobj'):
                with self.assertRaises(ValueError): m.copy_one(before,Path('C:/literal/new.exe'),Path('C:/literal'),'x')

    def test_create_only_mode_is_never_replaced_with_overwrite(self):
        before=custody(); modes=[]
        def opened(path, mode):
            modes.append(mode)
            if mode=='xb': raise FileExistsError('preserve existing output')
            return io.BytesIO()
        with patch.object(m,'snapshot',return_value=before), patch.object(m,'guard'), patch.object(m.Path,'open',opened):
            with self.assertRaises(FileExistsError): m.copy_one(before,Path('C:/literal/new.exe'),Path('C:/literal'),'x')
        self.assertEqual(modes,['rb','xb'])

    def test_pe64_rejects_dll_and_wrong_machine(self):
        data=bytearray(90); data[:2]=b'MZ'; data[60:64]=(64).to_bytes(4,'little'); data[64:68]=b'PE\0\0'; data[68:70]=(0x8664).to_bytes(2,'little'); data[88:90]=(0x20b).to_bytes(2,'little')
        with patch.object(m.Path,'open',return_value=io.BytesIO(data)): m.pe(Path('unused'))
        for offset,value in [(68,0x14c),(86,0x2000)]:
            bad=bytearray(data); bad[offset:offset+2]=value.to_bytes(2,'little')
            with patch.object(m.Path,'open',return_value=io.BytesIO(bad)):
                with self.assertRaises(ValueError): m.pe(Path('unused'))


class RuntimePolicyBatchControls(unittest.TestCase):
    def packet(self):
        paths = [Path('C:/literal/a.dll'), Path('C:/literal/b.dll')]
        value = {'schema':'airk.runtime-policy-batch/1',
                 'audit':{'schema':'airk.human-output-audit/1','expired':False,'elapsed_seconds':0.1,
                          'results':[{'index':0,'passed':True,'error':None},{'index':1,'passed':True,'error':None}]},
                 'policies':[{'index':i,'path':str(p),'policy':custody()['policy']} for i,p in enumerate(paths)]}
        return paths, value

    def test_exact_policy_values_preserved_in_order(self):
        paths, value = self.packet()
        value['policies'][1]['policy'] = {'owner_sid':m.SID,'sddl':'different literal SDDL',
            'rules':[{'sid':'S-1-5-18','rights':2032127,'type':'Allow'},
                     {'sid':'foreign','rights':2,'type':'Deny'}]}
        self.assertEqual(m.validate_policy_batch(value,paths,0.2),
                         [custody()['policy'],value['policies'][1]['policy']])

    def test_partial_duplicate_reordered_or_drifted_paths_refuse(self):
        for mutation in ['partial','duplicate','reorder','path','audit-index','missing-audit']:
            paths,value=self.packet()
            if mutation=='partial': value['policies'].pop()
            elif mutation=='duplicate': value['policies'][1]=copy.deepcopy(value['policies'][0])
            elif mutation=='reorder': value['policies'].reverse()
            elif mutation=='path': value['policies'][0]['path']='C:/other/a.dll'
            elif mutation=='audit-index': value['audit']['results'][1]['index']=0
            else: value['audit']['results'].pop()
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                m.validate_policy_batch(value,paths,0.2)

    def test_audit_refusal_expiry_and_metadata_errors_refuse(self):
        for mutation in ['refusal','expiry','both','sddl','ace','owner']:
            paths,value=self.packet()
            if mutation in ['refusal','both']:
                value['audit']['results'][0].update(passed=False,error='token group roster changed')
            if mutation in ['expiry','both']: value['audit']['expired']=True
            if mutation=='sddl': value['policies'][0]['policy']['sddl']=None
            if mutation=='ace': value['policies'][0]['policy']['rules']=[{'sid':'literal','rights':None,'type':'Allow'}]
            if mutation=='owner': value['policies'][0]['policy']['owner_sid']='foreign'
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                m.validate_policy_batch(value,paths,0.2)

    def test_deadline_is_bounded_and_nonfinite_times_refuse(self):
        for elapsed in [10,11,float('inf'),float('nan'),-1]:
            paths,value=self.packet()
            with self.assertRaises(ValueError): m.validate_policy_batch(value,paths,elapsed)
        paths,value=self.packet(); value['audit']['elapsed_seconds']=float('nan')
        with self.assertRaises(ValueError): m.validate_policy_batch(value,paths,0.2)

    def test_one_readonly_process_has_timeout_and_frozen_guard(self):
        paths,value=self.packet()
        with patch.object(m,'exact_pin') as pinned, patch.object(m.subprocess,'check_output',return_value=json.dumps(value)) as called, patch.object(m.time,'monotonic',side_effect=[10,10.2]):
            self.assertEqual(len(m.runtime_policies(paths,'frozen')),2)
        pinned.assert_called_once_with(m.GUARD,'frozen')
        self.assertEqual(called.call_count,1)
        self.assertEqual(called.call_args.kwargs['timeout'],10)
        command=called.call_args.args[0][-1]
        self.assertIn('Assert-AirkOutputBatch',command)
        self.assertIn('Assert-AirkCurrentToken $identity',command)
        self.assertIn('(10 - $clock.Elapsed.TotalSeconds)',command)

    def test_timeout_has_no_retry_or_partial_fallback(self):
        paths,_=self.packet()
        with patch.object(m,'exact_pin'), patch.object(m.subprocess,'check_output',side_effect=m.subprocess.TimeoutExpired('readonly',10)) as called:
            with self.assertRaises(m.subprocess.TimeoutExpired): m.runtime_policies(paths,'frozen')
        self.assertEqual(called.call_count,1)

    def test_census_reobserves_every_identity_after_metadata(self):
        root=Path('C:/literal'); paths=[root/'a.dll',root/'b.dll']; events=[]
        def observed(path):
            events.append(('identity',path.name))
            return custody(path.name)['identity']
        def policies(*args):
            events.append(('batch',None))
            return [custody()['policy'],custody()['policy']]
        with patch.object(m.Path,'resolve',return_value=root), patch.object(m.Path,'is_dir',return_value=True), patch.object(m.Path,'iterdir',return_value=iter(paths)), patch.object(m,'pin',side_effect=lambda p:custody(p.name)['pin']), patch.object(m,'identity',side_effect=observed), patch.object(m,'regular'), patch.object(m,'runtime_policies',side_effect=policies):
            # Each directory observation gets its own iterator.
            with patch.object(m.Path,'iterdir',side_effect=lambda:iter(paths)):
                rows=m.runtime_census(root,'frozen')
        self.assertEqual(len(rows),2)
        self.assertEqual(events,[('identity','a.dll'),('identity','b.dll'),('batch',None),('identity','a.dll'),('identity','b.dll')])

    def test_census_postbatch_stat_drift_refuses(self):
        root=Path('C:/literal'); path=root/'a.dll'; old=custody()['identity']
        for key in ['file_id','mtime_ns','bytes','links','volume']:
            changed={**old,key:999}
            with patch.object(m.Path,'resolve',return_value=root), patch.object(m.Path,'is_dir',return_value=True), patch.object(m.Path,'iterdir',side_effect=lambda:iter([path])), patch.object(m,'pin',return_value=custody()['pin']), patch.object(m,'identity',side_effect=[old,changed]), patch.object(m,'regular'), patch.object(m,'runtime_policies',return_value=[custody()['policy']]):
                with self.subTest(key=key), self.assertRaises(ValueError):
                    m.runtime_census(root,'frozen')


class IncrementalPreservation(unittest.TestCase):
    def setUp(self):
        self.runtime=m.USER_ROOT/'fresh-runtime'
        self.producer={'path':str(m.USER_ROOT/'fresh-producer/gpu-incremental-build-receipt.json'),'bytes':100,'sha256':'b'*64}
        self.archive={'path':str(m.USER_ROOT/'fresh-producer/aggregate/onnxruntime.lib'),'bytes':1000,'sha256':'c'*64}
        self.manifest={'schema':'airk.native.cuda-runtime-manifest/1','ort_version':'1.30.0','static_core_build':'b'*64,
                       'producer_name':'gpu-incremental-build-receipt.json','producer_bytes':100,'producer_sha256':'b'*64}
        self.manifest_pin={'path':str(self.runtime/'airk-cuda-runtime.json'),'bytes':200,'sha256':'d'*64}
        self.runtime_producer=dict(self.producer,path=str(self.runtime/self.manifest['producer_name']))

    def test_actual_incremental_cargo_link_path_and_old_mode_separate(self):
        rows=cargo();rows[1]['linked_paths']=['native='+str(Path(self.archive['path']).parent)]
        self.assertEqual(len(m.cargo_rows(encoded(rows),Path(self.archive['path']))[1]),2)
        with self.assertRaises(ValueError):m.cargo_rows(encoded(rows))
        with self.assertRaises(ValueError):m.cargo_rows(encoded(cargo()),Path(self.archive['path']))

    def test_fresh_manifest_matches_actual_raw_producer(self):
        with patch.object(m,'pin',return_value=self.runtime_producer):
            actual=m.validate_graph_manifest(self.manifest,self.manifest_pin,self.runtime,{'producer':self.producer})
        self.assertEqual(actual,self.runtime_producer)

    def test_old_wrong_or_retagged_producer_refuses(self):
        for key,bad in [('static_core_build',m.OLD_PRODUCER),('producer_sha256','f'*64),('producer_bytes',99),
                        ('producer_name','../producer.json'),('schema','wrong')]:
            with self.subTest(key=key):
                changed=dict(self.manifest);changed[key]=bad
                with patch.object(m,'pin',return_value=self.runtime_producer),self.assertRaises(ValueError):
                    m.validate_graph_manifest(changed,self.manifest_pin,self.runtime,{'producer':self.producer})
        with patch.object(m,'pin',return_value=dict(self.runtime_producer,sha256='f'*64)),self.assertRaises(ValueError):
            m.validate_graph_manifest(self.manifest,self.manifest_pin,self.runtime,{'producer':self.producer})

    def test_verifier_source_roster_is_exact_and_path_closed(self):
        names=['scripts/cuda-runtime-environment.py','scripts/cuda-static-runtime.py','scripts/cuda-static-aggregate.py',
               'scripts/cuda-static-incremental.py','scripts/cuda-static-archive.py','scripts/cuda-static-checkpointed.py',
               'scripts/cuda-static-continuation-r12.py',
               'scripts/cuda-static-runtime/verification/checkpoint_sections.py',
               'scripts/cuda-static-runtime/verification/materialized.py',
               'scripts/cuda-component-execution.py','scripts/provision-static-onnx-runtime.py','scripts/static-onnx-runtime-lock.json']
        data={name:'a'*64 for name in names}
        with patch.object(m,'exact_pin',side_effect=lambda path,sha:{'path':str(path),'bytes':10,'sha256':sha}),patch.object(Path,'read_bytes',return_value=json.dumps(data).encode()):
            _,pins=m.source_roster(Path('C:/literal/roster.json'),'a'*64)
            self.assertEqual({x['path'] for x in pins},{str(m.ROOT/name) for name in names})
        data['scripts/foreign.py']='a'*64
        with patch.object(m,'exact_pin',return_value={'bytes':10}),patch.object(Path,'read_bytes',return_value=json.dumps(data).encode()),self.assertRaises(ValueError):
            m.source_roster(Path('C:/literal/roster.json'),'a'*64)

    def test_environment_calls_dedicated_verifier(self):
        proof={'schema':'airk.cuda-static-build-proof/3','new_aggregate':self.archive,'build_receipt':self.producer}
        aggregate={'schema':'airk.cuda-static-aggregate-proof/1'}
        environment={'LIBONNXRUNTIME_NO_PKG_CONFIG':'1','ORT_SKIP_DOWNLOAD':'1','ORT_PREFER_DYNAMIC_LINK':'0',
                     'ORT_LIB_LOCATION':str(Path(self.archive['path']).parent),'ORT_LIB_PROFILE':'Release'}
        value={'schema':'airk.cuda-static-environment/5','build':proof,'aggregate':aggregate}
        calls=[]
        def verify(*args,**kwargs):
            calls.append((args,kwargs))
            return {'gpu_build':proof,'gpu_aggregate':aggregate,'environment':environment,'activation_pins_available':False}
        owner=SimpleNamespace(verify_handoff=verify)
        spec=SimpleNamespace(loader=SimpleNamespace(exec_module=lambda module:None))
        rows={self.archive['path']:self.archive,self.producer['path']:self.producer}
        patches=[patch.object(m,'source_roster',return_value=({'path':'roster','bytes':1,'sha256':'a'*64},[])),
                 patch.object(m,'exact_pin',return_value={'path':'environment','bytes':100,'sha256':'a'*64}),
                 patch.object(Path,'read_bytes',return_value=json.dumps(value).encode()),
                 patch.object(m.importlib.util,'spec_from_file_location',return_value=spec),
                 patch.object(m.importlib.util,'module_from_spec',return_value=owner),patch.object(m,'pin',side_effect=lambda path,**kwargs:rows[path])]
        from contextlib import ExitStack
        with ExitStack() as stack:
            for item in patches:stack.enter_context(item)
            result=m.verify_environment(m.USER_ROOT/'environment.json','a'*64,Path('roster'),'a'*64)
            self.assertEqual(result['proof'],proof);self.assertEqual(len(calls),1)
            with patch.object(m.time,'monotonic',return_value=1):
                m.verify_environment(m.USER_ROOT/'environment.json','a'*64,Path('roster'),'a'*64,deadline=120)
            self.assertEqual(calls[-1][1],{'deadline':120})
            def refusal(*args):raise ValueError('raw producer invalid')
            owner.verify_handoff=refusal
            with self.assertRaisesRegex(ValueError,'raw producer invalid'):m.verify_environment(m.USER_ROOT/'environment.json','a'*64,Path('roster'),'a'*64)

    def image(self):
        data=bytearray(2560);data[:2]=b'MZ';struct.pack_into('<I',data,60,128);data[128:132]=b'PE\0\0'
        struct.pack_into('<HH',data,132,0x8664,1);struct.pack_into('<H',data,148,240);struct.pack_into('<H',data,152,0x20b)
        struct.pack_into('<I',data,260,16);struct.pack_into('<II',data,264,0x1100,0x100)
        struct.pack_into('<IIII',data,400,2048,0x1000,2048,512);struct.pack_into('<I',data,428,0x60000020)
        struct.pack_into('<IIIII',data,788,2,2,0x1130,0x1140,0x1150)
        struct.pack_into('<II',data,816,0x1000,0x1010);struct.pack_into('<II',data,832,0x1160,0x11a0);struct.pack_into('<HH',data,848,0,1)
        for offset,name in [(864,b'airk_ort_collect_final_placement_v1'),(928,b'airk_ort_collect_final_graph_metadata_v1')]:data[offset:offset+len(name)+1]=name+b'\0'
        return data

    def exports(self,data):
        with patch.object(Path,'open',side_effect=lambda *a,**k:io.BytesIO(data)),patch.object(Path,'stat',return_value=SimpleNamespace(st_size=len(data))):
            return m.exported_collectors(Path('C:/literal/image.exe'))

    def test_actual_pe_has_both_static_collectors(self):
        self.assertEqual(self.exports(self.image()),['airk_ort_collect_final_graph_metadata_v1','airk_ort_collect_final_placement_v1'])

    def test_forwarded_missing_or_nonexecutable_collector_refuses(self):
        for mutation in ['forwarded','missing','data','ordinal']:
            data=self.image()
            if mutation=='forwarded':struct.pack_into('<I',data,820,0x1100)
            elif mutation=='missing':data[928]=ord('x')
            elif mutation=='data':struct.pack_into('<I',data,428,0x40000040)
            else:struct.pack_into('<H',data,850,3)
            with self.subTest(mutation=mutation),self.assertRaises(ValueError):self.exports(data)



class OriginalProvenanceDeadline(unittest.TestCase):
    def test_environment_refuses_expired_before_source_load(self):
        with patch.object(m.time,'monotonic',return_value=2),patch.object(m,'source_roster') as read:
            with self.assertRaises(ValueError):m.verify_environment('env','sha','roster','sha',deadline=2)
            read.assert_not_called()
    def test_binding_refuses_expired_before_pin(self):
        with patch.object(m.time,'monotonic',return_value=2),patch.object(m,'pin') as read:
            with self.assertRaises(ValueError):m.verify_graph_binding({}, {}, {}, {},deadline=2)
            read.assert_not_called()
    def test_pe_refuses_expired_before_read(self):
        with patch.object(m.time,'monotonic',return_value=2),patch.object(m,'pe') as read:
            with self.assertRaises(ValueError):m.exported_collectors('image',deadline=2)
            read.assert_not_called()
    def test_hash_checks_deadline_after_each_chunk(self):
        reads=[]
        with patch.object(m.time,'monotonic',return_value=1) as clock:
            class Stream(io.BytesIO):
                def read(self,*args):
                    reads.append(args);clock.return_value=2;return b'data'
            with patch.object(Path,'open',return_value=Stream()):
                with self.assertRaises(ValueError):m.digest('input',deadline=2)
        self.assertEqual(reads,[(4<<20,)])


class FreshPreservation(unittest.TestCase):
    def setUp(self):
        self.root = m.USER_ROOT / 'inert-fresh'
        self.runtime = m.USER_ROOT / 'inert-runtime'
        self.env_path = self.root / 'environment.json'
        self.deadline = 500
        self.files = {}
        self.producer = self.row(self.root / 'fresh-compile.json', '1')
        self.configured = self.row(self.root / 'fresh-configure.json', '2')
        self.configure = self.row(self.root / 'configure-recipe.json', '3')
        self.aggregate = self.row(self.root / 'fresh-aggregate.json', '4')
        self.aggregate_recipe = self.row(self.root / 'archive-recipe.json', '5')
        self.driver = self.row(m.ROOT / 'scripts/cuda-static-runtime/build/fresh_compile.py', '6')
        self.env_pin = self.row(self.env_path, '7')
        self.roster = self.row(self.root / 'sources.json', '8')
        self.archive = self.row(self.root / 'gpu-aggregate/onnxruntime.lib', '9')
        self.providers = [
            self.row(self.root / 'b/Release/onnxruntime_providers_shared.dll', 'a'),
            self.row(self.root / 'b/Release/onnxruntime_providers_cuda.dll', 'b'),
        ]
        self.proof = {
            'schema': 'airk.private.cuda-fresh-static-build-proof/1',
            'build_receipt': self.producer, 'configure_receipt': self.configured,
            'configure_recipe': self.configure, 'producer_source': self.driver,
            'provider_dlls': self.providers, 'qualification': False, 'activation_pins_available': False,
        }
        self.files[self.producer['path']] = {
            'schema': 'airk.private.cuda-fresh-compiled/1', 'status': 'COMPILED_UNQUALIFIED', 'qualification': False,
            'configured': self.configured, 'recipe': self.configure, 'driver': self.driver, 'providers': self.providers,
        }
        self.files[self.configured['path']] = {
            'schema': 'airk.private.cuda-fresh-configured/1', 'status': 'CONFIGURED_UNQUALIFIED',
            'qualification': False, 'recipe': self.configure,
        }
        self.files[self.configure['path']] = {
            'schema': 'airk.private.cuda-fresh-configure-recipe/1', 'msvc_ciso646_compat': True,
            'output_audit_owner': self.driver, 'fresh_output': str(self.root),
            'execution_authorized': True, 'qualification': False,
        }
        self.files[self.aggregate_recipe['path']] = {
            'schema': 'airk.private.cuda-fresh-aggregate-recipe/1', 'compile_receipt': self.producer,
            'execution_authorized': True, 'qualification': False,
        }
        plan = {'compile_receipt': self.producer, 'producer_proof_sha256': m.census_digest(self.proof)}
        output = {**self.archive, 'path': 'gpu-aggregate/onnxruntime.lib'}
        self.raw = {
            'schema': 'airk.private.cuda-fresh-aggregate/1', 'status': 'AGGREGATED_UNQUALIFIED',
            'recipe': self.aggregate_recipe, 'compile_receipt': self.producer, 'configure_receipt': self.configured,
            'plan': plan, 'plan_sha256': m.census_digest(plan), 'output': output,
            'coff': {'kind': 'static'}, 'qualification': False, 'error': None,
        }
        self.files[self.aggregate['path']] = self.raw
        self.aggregate_proof = {
            'schema': 'airk.cuda-static-aggregate-proof/1', 'qualification': False,
            'receipt_sha256': self.aggregate['sha256'], 'producer_proof_sha256': plan['producer_proof_sha256'],
            'plan_sha256': self.raw['plan_sha256'], 'coff': self.raw['coff'], 'output': output,
        }
        self.env = {
            'schema': 'airk.cuda-static-environment/6', 'activation_pins_available': False,
            'request': {'destination': str(self.root), 'aggregate_receipt': self.aggregate},
            'build': self.proof, 'aggregate': self.aggregate_proof,
            'environment': {'LIBONNXRUNTIME_NO_PKG_CONFIG': '1', 'ORT_SKIP_DOWNLOAD': '1',
                            'ORT_PREFER_DYNAMIC_LINK': '0', 'ORT_LIB_LOCATION': str(self.root / 'gpu-aggregate'),
                            'ORT_LIB_PROFILE': 'Release'},
        }
        self.files[self.env_pin['path']] = self.env
        self.sources = [self.driver]
        self.manifest_pin = self.row(self.runtime / 'airk-cuda-runtime.json', 'c')
        self.runtime_producer = {**self.producer, 'path': str(self.runtime / 'fresh-compile.json')}
        self.staged = {self.runtime_producer['path']: self.runtime_producer}
        self.manifest = {
            'schema': 'airk.native.cuda-runtime-manifest/1', 'ort_version': '1.30.0', 'fits': [],
            'producer_name': 'fresh-compile.json', 'producer_bytes': self.producer['bytes'],
            'producer_sha256': self.producer['sha256'], 'static_core_build': self.producer['sha256'],
            'files': [],
        }
        for row in self.providers:
            name = Path(row['path']).name
            staged = {**row, 'path': str(self.runtime / name)}
            self.staged[staged['path']] = staged
            self.manifest['files'].append({'name': name, 'bytes': row['bytes'], 'sha256': row['sha256']})

    @staticmethod
    def row(path, char):
        return {'path': str(path), 'bytes': 42, 'sha256': char * 64}

    def doc(self, row, maximum, deadline):
        self.assertEqual(deadline, self.deadline)
        return self.files[row['path']]

    def fixture_pin(self, path, *, deadline):
        self.assertEqual(deadline, self.deadline)
        return {self.archive['path']: self.archive, **self.staged}[str(path)]

    def test_explicit_mode_deadline_and_no_mixed_lineage(self):
        args = SimpleNamespace(fresh_finalgraph=True, fresh_environment=self.env_path,
                               fresh_environment_sha256=self.env_pin['sha256'], provenance_source_roster='sources',
                               provenance_source_roster_sha256='reviewed', manifest_sha256='reviewed',
                               verification_seconds=600)
        with patch.object(m.time, 'monotonic', return_value=100):
            self.assertEqual(m.preservation_mode(args), (False, 700))
            for fields in ({'incremental_finalgraph': True}, {'incremental_environment': 'old'},
                           {'verification_seconds': 601}, {'verification_seconds': True},
                           {'verification_seconds': float('inf')}, {'fresh_finalgraph': False}):
                mixed = SimpleNamespace(**{**vars(args), **fields})
                with self.subTest(fields=fields), self.assertRaises(ValueError):
                    m.preservation_mode(mixed)

    def test_environment_uses_exact_full_owner_once_and_reopens_distinct_documents(self):
        expected = {'gpu_build': self.proof, 'gpu_aggregate': self.aggregate_proof,
                    'environment': self.env['environment'], 'activation_pins_available': False}
        with patch.object(m.time, 'monotonic', return_value=100), \
                patch.object(m, 'fresh_source_roster', return_value=(self.roster, self.sources)) as roster, \
                patch.object(m, 'fresh_reviewed_pin', return_value=self.env_pin), \
                patch.object(m, 'fresh_source_loader') as loader, \
                patch.object(m, 'fresh_json', side_effect=self.doc) as docs, \
                patch.object(m, 'pin', side_effect=self.fixture_pin):
            owner = loader.return_value.return_value
            owner.verify_handoff.return_value = expected
            observed = m.verify_fresh_environment(self.env_path, self.env_pin['sha256'], self.roster['path'],
                                                  self.roster['sha256'], deadline=500)
        owner.verify_handoff.assert_called_once_with(self.env_path, self.env_pin['sha256'], m.ROOT, deadline=500)
        self.assertEqual(roster.call_count, 2)
        self.assertEqual(observed['aggregate_receipt'], self.aggregate)
        self.assertEqual(observed['aggregate_recipe'], self.aggregate_recipe)
        self.assertEqual(observed['configure_receipt'], self.configured)
        self.assertEqual(observed['configure_recipe'], self.configure)
        self.assertEqual(observed['archive'], self.archive)
        self.assertEqual({call.args[0]['path'] for call in docs.call_args_list}, set(self.files))

    def test_cross_generation_resealing_and_raw_receipt_alias_refuse(self):
        with patch.object(m.time, 'monotonic', return_value=100), patch.object(m, 'fresh_json', side_effect=self.doc), \
                patch.object(m, 'pin', side_effect=self.fixture_pin):
            bad = copy.deepcopy(self.env)
            bad['request']['aggregate_receipt']['sha256'] = self.producer['sha256']
            with self.assertRaisesRegex(ValueError, 'distinct same-build'):
                m.fresh_chain(bad, deadline=500)
            self.raw['plan']['producer_proof_sha256'] = 'f' * 64
            self.raw['plan_sha256'] = m.census_digest(self.raw['plan'])
            self.aggregate_proof['plan_sha256'] = self.raw['plan_sha256']
            self.aggregate_proof['producer_proof_sha256'] = 'f' * 64
            with self.assertRaisesRegex(ValueError, 'aggregate receipt or plan'):
                m.fresh_chain(self.env, deadline=500)

    def test_loader_uses_exact_source_bytes_and_refuses_outside_roster(self):
        owner_pin = self.row(m.ROOT / 'scripts/cuda-runtime-environment.py', 'a')
        stub = b"def verify_handoff(*args): return 'from authenticated bytes'"
        with patch.object(m.time, 'monotonic', return_value=100), patch.object(m, 'fresh_bytes', return_value=stub) as read:
            load = m.fresh_source_loader([owner_pin], 500)
            owner = load(owner_pin['path'], 'inert_fresh_environment')
            self.assertEqual(owner.verify_handoff(), 'from authenticated bytes')
            with self.assertRaisesRegex(ValueError, 'unreviewed fresh executable'):
                load(m.ROOT / 'scripts/unreviewed.py', 'inert_unreviewed')
            read.assert_called_once_with(owner_pin, 2 << 20, 500)

    def test_bounded_read_rejects_changed_source_before_execution(self):
        path = m.ROOT / 'scripts/cuda-runtime-environment.py'
        raw = b"raise RuntimeError('should never execute')"
        pin = {**self.row(path, 'a'), 'bytes': len(raw)}
        with patch.object(m.time, 'monotonic', return_value=100), patch.object(m, 'regular', return_value=path), \
                patch.object(Path, 'stat', return_value=SimpleNamespace(st_size=len(raw))), \
                patch.object(Path, 'open', return_value=io.BytesIO(raw)):
            with self.assertRaisesRegex(ValueError, 'consumed fresh provenance bytes'):
                m.fresh_source_loader([pin], 500)(path, 'inert_changed')

    def test_manifest_and_staged_provider_match_actual_original_pair(self):
        provenance = {'producer': self.producer, 'proof': self.proof}
        with patch.object(m.time, 'monotonic', return_value=100), patch.object(m, 'pin', side_effect=self.fixture_pin):
            actual = m.validate_fresh_graph_manifest(self.manifest, self.manifest_pin, self.runtime, provenance, deadline=500)
            self.assertEqual(actual, self.runtime_producer)
            self.manifest['files'][0]['sha256'] = 'f' * 64
            with self.assertRaisesRegex(ValueError, 'staged graph provider bytes'):
                m.validate_fresh_graph_manifest(self.manifest, self.manifest_pin, self.runtime, provenance, deadline=500)
            self.manifest['files'][0]['sha256'] = self.providers[0]['sha256']
            self.staged[str(self.runtime / self.manifest['files'][1]['name'])]['bytes'] += 1
            with self.assertRaisesRegex(ValueError, 'staged graph provider bytes'):
                m.validate_fresh_graph_manifest(self.manifest, self.manifest_pin, self.runtime, provenance, deadline=500)

    def test_public_graph_preparation_dispatches_only_the_exact_fresh_owner(self):
        binding = {'schema': 'airk.private.cuda-fresh-finalgraph-image-binding/1',
                   'environment': self.env_pin, 'source_roster': self.roster}
        with patch.object(m.time, 'monotonic', return_value=100), \
                patch.object(m, 'verify_fresh_environment', return_value={'lineage': 'fresh'}) as fresh, \
                patch.object(m, 'verify_environment') as older, patch.object(m, 'verify_graph_inputs') as current:
            m.verify_graph_binding(binding, 'manifest', 'image', 'build', deadline=500)
        older.assert_not_called()
        fresh.assert_called_once_with(self.env_pin['path'], self.env_pin['sha256'], self.roster['path'],
                                      self.roster['sha256'], deadline=500)
        current.assert_called_once_with(binding, 'manifest', 'image', 'build', {'lineage': 'fresh'}, deadline=500)

    def test_current_preservation_rechecks_without_a_second_full_ancestry(self):
        binding = {'environment': self.env_pin, 'source_roster': self.roster}
        with patch.object(m, 'fresh_source_roster', return_value=(self.roster, self.sources)), \
                patch.object(m, 'fresh_json', side_effect=self.doc), patch.object(m, 'fresh_chain', return_value={}), \
                patch.object(m, 'verify_graph_inputs') as current, patch.object(m, 'verify_fresh_environment') as whole:
            m.verify_fresh_current(binding, 'manifest', 'image', 'build', deadline=500)
        whole.assert_not_called()
        current.assert_called_once_with(binding, 'manifest', 'image', 'build',
                                        {'environment': self.env_pin, 'source_roster': self.roster,
                                         'source_pins': self.sources}, deadline=500)


if __name__ == '__main__': unittest.main()