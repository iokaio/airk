# SPDX-License-Identifier: Apache-2.0
"""Literal/in-memory CUDA image custody refusals; no file, native or GPU work."""
import copy
import importlib.util
import io
import json
from pathlib import Path
import unittest
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


if __name__ == '__main__': unittest.main()