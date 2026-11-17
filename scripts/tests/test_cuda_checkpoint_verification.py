# SPDX-License-Identifier: Apache-2.0
"""Literal read-only checkpoint validation controls; no filesystem fixtures."""
import ast,copy,importlib.util,subprocess,unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock,patch
HERE=Path(__file__).resolve().parents[2] if Path(__file__).parent.name=='tests' else Path(__file__).resolve().parent
def load(name):
 p=HERE/'scripts'/name;s=importlib.util.spec_from_file_location(name.replace('/','_').replace('.','_'),p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
sections=load('cuda-static-runtime/verification/checkpoint_sections.py');material=load('cuda-static-runtime/verification/materialized.py');owner=load('cuda-static-checkpointed.py');tree=load('cuda-static-continuation-r12.py')
def row(name,sha='a'):return {'path':'C:/literal/'+name,'bytes':3,'sha256':sha*64}
def section():
 v={'schema':'airk.private.cuda-checkpoint-section/1','status':'PASS_TARGET_SECTION_ONLY_UNQUALIFIED','qualification':False,'section_id':'flash-remainder','section_index':0,'plan':row('plan'),'runner':row('runner'),'input_contract':row('input'),'started_utc':'2026-11-17T00:00:00+00:00','completed_utc':'2026-11-17T00:01:00+00:00','absolute_cutoff_utc':'2026-11-17T03:00:00+00:00','original_section_seconds':10800,'elapsed_seconds':60,'command':{'actual_tree_result':{'schema':'airk.private.process-tree-result/1','exit_code':0,'wait':'Start-Process-Wait-process-and-descendants'},'owned_job_final_active_processes':0,'owned_job_final_member_pids':[]}}
 v['runner']['sha256']='b382a216d1a46a1245b6fc3f78289e9abb61a027eed5f86699713c0b4db0d619';v['input_contract']['sha256']='68a5f3109e3ad78a5453a966fd1d00db4217096536f7b7f7253ef309d9a396eb'
 for key in ['warm_checkpoint_before','warm_checkpoint_after','source_manifest_before','source_manifest_after']:v[key]=row(key)
 return v
def core():return SimpleNamespace(plain=lambda p,**kw:Path(p),verify_file=Mock(),read_json=lambda p:{'same':'source'},successful=lambda c:True)
class VerificationControls(unittest.TestCase):
 def test_historical_header_pins_remain_authority(self):
  v={'schema':'airk.private.cuda-checkpointed-build/1','status':'PASS_CHECKPOINTED_PRODUCER_ONLY_UNQUALIFIED','mode':'checkpointed-warm','qualification':False,'cold_build_qualified':False,'producer_source':{'sha256':owner.COMPOSER},'input_contract':{'sha256':owner.INPUT},'plan':{'sha256':owner.PLAN},'original_failed_build':{'sha256':owner.R12}}
  owner.require_header(v)
  for key in ['producer_source','input_contract','plan','original_failed_build']:
   bad=copy.deepcopy(v);bad[key]['sha256']='0'*64
   with self.assertRaises(ValueError):owner.require_header(bad)
 def test_section_exact_original_deadline_and_tree(self):
  v=section();sections.verify_section(v,0,core(),'a'*64)
  for key,value in [('elapsed_seconds',10801),('absolute_cutoff_utc','2026-11-17T03:00:01+00:00'),('qualification',True),('section_index',1)]:
   bad=copy.deepcopy(v);bad[key]=value
   with self.assertRaises(ValueError):sections.verify_section(bad,0,core(),'a'*64)
  bad=copy.deepcopy(v);bad['command']['owned_job_final_member_pids']=[123]
  with self.assertRaises(ValueError):sections.verify_section(bad,0,core(),'a'*64)
 def test_first_custody_copies_exact_original_bytes(self):
  v=section();rows=[]
  for name in ['ninja_log','ninja_deps']:
   v[name+'_before']=row(name+'before');v[name+'_after']=row(name+'after','b')
   rows.append({'kind':name,'before_original':v[name+'_before'],'after_original':v[name+'_after'],'before_copy':{**v[name+'_before'],'path':'C:/copy/'+name+'before'},'after_copy':{**v[name+'_after'],'path':'C:/copy/'+name+'after'}})
  body={'schema':'airk.private.cuda-first-section-custody/1','phase_receipt_unchanged':True,'rows':rows};o=core();o.read_json=lambda p:body
  result=sections.normalized_custody(o,0,v,[v,{'predecessor_custody':row('bridge')}])
  self.assertEqual(set(result),{'before_log','before_deps','after_log','after_deps'})
  rows[0]['after_copy']['sha256']='c'*64
  with self.assertRaises(ValueError):sections.normalized_custody(o,0,v,[v,{'predecessor_custody':row('bridge')}])
 def test_section_order_and_successor_identity_refuse(self):
  with self.assertRaises(ValueError):sections.validate_sections(core(),sections,[], 'x','y','z')
 def test_changed_source_rows_preserve_original_and_single_target(self):
  before=[{'path':material.SOURCE_REL,'bytes':3,'sha256':'old'},{'path':'unrelated','bytes':2,'sha256':'same'}]
  self.assertEqual(material.changed_rows(before,'old','new',4),[{'path':material.SOURCE_REL,'bytes':4,'sha256':'new'},before[1]])
  self.assertEqual(before[0]['sha256'],'old')
  for rows in [[],[before[0],before[0]],[{**before[0],'sha256':'wrong'}]]:
   with self.assertRaises(ValueError):material.changed_rows(rows,'old','new',4)
 def test_duplicate_input_and_mirror_drift_refuse(self):
  o=core()
  with self.assertRaises(ValueError):material.verify_input_closure(o,[row('same'),row('same')])
  o.source_inventory=lambda p:[{'path':'unexpected'}]
  with self.assertRaises(ValueError):material.verify_mirror_closure(o,Path('C:/mirror'),[{'path':'expected'}])
 def test_command_request_exact_historical_shape(self):
  result=material.command_request(['C:/Windows/System32/cmd.exe','/d','/c','C:/a b/build.cmd'],Path('C:/cwd'),Path('C:/out'),'stage')
  self.assertEqual(result,{'schema':'airk.private.process-tree-command/1','executable':'C:/Windows/System32/cmd.exe','arguments':'/d /c "C:/a b/build.cmd"','cwd':str(Path('C:/cwd')),'stdout':str(Path('C:/out/logs/stage-tree.stdout.log')),'stderr':str(Path('C:/out/logs/stage-tree.stderr.log')),'result':str(Path('C:/out/stage-tree-result.json')),'started':str(Path('C:/out/stage-tree-started.json')),'started_event':None,'done_event':None})
  with self.assertRaises(ValueError):material.command_request(['relative.exe'],Path('C:/cwd'),Path('C:/out'),'stage')
 def test_checkpoint_path_has_no_ignored_dynamic_execution(self):
  text=(HERE/'scripts/cuda-static-checkpointed.py').read_text()
  self.assertNotIn('runtime.load(script',text);self.assertNotIn("runtime.load(inputs['successor_runner']",text)
  self.assertIn('tree.verify_tree_command(core,materialized,',text)
  for name in ['checkpoint_sections.py','materialized.py']:
   parsed=ast.parse((HERE/'scripts/cuda-static-runtime/verification'/name).read_text())
   for node in ast.walk(parsed):
    if isinstance(node,ast.Call):
     self.assertNotIn(getattr(node.func,'attr',''),['exec_module','run','Popen','write_text','write_bytes','mkdir'])
 def test_toolchain_metadata_projection_is_unchanged(self):
  names=['compiler','cmake','ninja','patch','python','vcvars'];tools={n:{**row(n),'probe':{'seconds':1.5}} for n in names}
  self.assertEqual(owner.toolchain_pins(tools),[row(n) for n in names])
if __name__=='__main__':unittest.main()
