# SPDX-License-Identifier: Apache-2.0
"""Explicit scoped P14 final queue. Adopts the sole release producer; never builds/retries/edits."""
import argparse, atexit, hashlib, importlib.util, json, os, re, subprocess, time
from pathlib import Path
import xml.etree.ElementTree as ET
from datetime import datetime
BASE=Path(__file__).resolve().parent
MAP=BASE/'p14-final-qualification-command-map-r3.json'
def read(p):return json.loads(p.read_text(encoding='utf-8'))
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def load(name,path):
 s=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--fmt-receipt',type=Path,required=True);p.add_argument('--debug-junit',type=Path,required=True);p.add_argument('--debug-junit-sha256',required=True);p.add_argument('--release-receipt',type=Path,default=BASE/'p14-release-runtime-r1.receipt.json');args=p.parse_args()
plan=read(MAP);WORK=Path(plan['cwd']);SOURCE=plan['source_expected'];runtime=Path(plan['runtime']);output=BASE/'p14-final-qualification-r3.json';assert not output.exists(),'Fresh campaign label required after correction'
prepared=read(BASE/'p14-final-prepared-r3.json');assert prepared['ready'] is True and sha(MAP)==prepared['map_sha256'] and sha(Path(__file__))==prepared['runner_sha256'],'Reviewed queue/map drift'
assert args.release_receipt.resolve()==Path(plan['release_adoption']['receipt']).resolve(),'Only exact reviewed sole release producer may be adopted'
lock=BASE/'p14-final-owner-r1.lock';fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.write(fd,str(os.getpid()).encode())
def unlock():os.close(fd);lock.unlink()
atexit.register(unlock)
os.environ['GIT_CONFIG_GLOBAL']=str(BASE/'p10-owned-ignore-controls/global.gitconfig')
builder=load('p14_final_builder',WORK/'scripts/build-service-runtime.py');sealer=load('p14_final_sealer',BASE/'p9-integrated-checks.py')
record={'status':'running','map_sha256':sha(MAP),'source':SOURCE,'steps':[],'runtime_basis':'FRESH P14 current production release; P12pair baseline only, no test-only equivalence adoption'};seal=None

def save():
 pending=output.with_suffix('.pending');pending.write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8');pending.replace(output)
def pins():
 assert builder.source_identity()==SOURCE,'Native source drift'
 for name,digest in plan['pinned_inputs'].items():assert sha(Path(name))==digest,'Pinned evidence/helper drift: '+name
 for row in plan['driver_parity']:
  assert sha(BASE.parents[1]/row['path'])==row['root_sha256'] and sha(WORK/row['path'])==row['worktree_sha256'],'Driver drift'
 if seal is not None:assert sealer.seal_runtime(runtime,SOURCE)==seal,'Runtime seal drift'
def private(stage):
 proof=plan['private_base_proof'];assert sha(Path(proof['receipt']))==proof['receipt_sha256']
 result=subprocess.run(['pwsh','-NoProfile','-File',proof['verifier'],'-ExpectedReceiptSha256',proof['receipt_sha256']],capture_output=True,text=True)
 assert result.returncode==0,'Private owned-root proof refused: '+result.stdout+result.stderr
 value=json.loads(result.stdout.lstrip('\ufeff'));assert all(value[k]==proof[k] for k in ['path','owner_sid','sddl']) and value['acl_mutated'] is False
 record.setdefault('private_base_observations',[]).append({'stage':stage,**value})
def receipt(path,expected):
 value=read(path);assert value['actual_exit_code']==value['exit_code']==0
 assert value['source_before']==value['source_after']==expected and value['source_unchanged']
 assert sha(path.with_name(path.name.replace('.receipt.json','.log')))==value['log_sha256']
 return value
def junit(path,count):
 cases=ET.parse(path).getroot().findall('.//testcase');assert len(cases)==count and not any(c.find(x) is not None for c in cases for x in ['failure','error','skipped']);return cases
save()
try:
 pins();private('prerequisites')
 fmt=receipt(args.fmt_receipt,SOURCE);assert Path(fmt['command'][0]).stem=='cargo' and fmt['command'][1:]==['fmt','--all','--','--check'], 'Separate actualcurrent formatting receipt required'
 record['adopted_format']={'receipt':str(args.fmt_receipt),'receipt_sha256':sha(args.fmt_receipt),'executed_in_this_queue':False}
 receipt(Path(plan['adopted_current_fmt_clippy']),SOURCE)
 debug=receipt(Path(plan['adopted_debug_semantic']),SOURCE);assert debug['passing_tests']==1
 full=receipt(Path(plan['adopted_full_debug']['receipt']),SOURCE)
 command=full['command'];assert Path(command[0]).stem=='cargo' and command[1:3]==['nextest','run']
 assert all(flag in command for flag in ['--workspace','--all-features','--locked','--offline','--ignore-default-filter','--no-fail-fast'])
 assert command[command.index('--profile')+1]=='ci' and command[command.index('--test-threads')+1]=='4' and command[command.index('--retries')+1]=='0'
 assert not any(flag in command for flag in ['-E','--filter-expr','--test','--lib','--package','-p','--exclude','--no-run']), 'Full workspace receipt contains a narrowing selector'
 assert sha(args.debug_junit)==args.debug_junit_sha256
 source_junit=WORK/'target/nextest/ci/junit.xml'
 assert source_junit.is_file() and sha(source_junit)==args.debug_junit_sha256, 'Live sourceJUnit no longer matches immutable collected copy; do not run another Nextest before this adoption'
 full_receipt_path=Path(plan['adopted_full_debug']['receipt'])
 full_log=full_receipt_path.with_name(full_receipt_path.name.replace('.receipt.json','.log'))
 assert os.name=='nt', 'This sourceJUnit birthtime proof is Windows-only'
 birth=full_log.stat().st_birthtime_ns
 js=source_junit.stat();completed=full_receipt_path.stat().st_mtime_ns
 assert js.st_birthtime_ns<=js.st_mtime_ns and birth<=js.st_mtime_ns<=completed, 'SourceJUnit is outside completed fullNextest log/receipt window'
 debug_junit=BASE/'p14-final-adopted-full-debug-r3.junit.xml';assert not debug_junit.exists()
 with debug_junit.open('xb') as handle:handle.write(args.debug_junit.read_bytes())
 assert sha(debug_junit)==args.debug_junit_sha256
 xml=ET.parse(debug_junit).getroot();cases=xml.findall('.//testcase')
 assert cases and not any(c.find(x) is not None for c in cases for x in ['failure','error'])
 xml_failed=sum(c.find('failure') is not None for c in cases);xml_errors=sum(c.find('error') is not None for c in cases);xml_skipped=sum(c.find('skipped') is not None for c in cases);xml_passed=len(cases)-xml_failed-xml_errors-xml_skipped
 assert int(xml.attrib['tests'])==len(cases) and int(xml.attrib['failures'])==xml_failed and int(xml.attrib['errors'])==xml_errors and int(xml.attrib['skipped'])==xml_skipped
 timestamp=datetime.fromisoformat(xml.attrib['timestamp']);assert timestamp.tzinfo is not None
 assert birth<=int(timestamp.timestamp()*1e9)<=completed, 'JUnit run timestamp outside actual fullNextest execution'
 text=re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]','',full_log.read_text(encoding='utf-8',errors='strict'))
 summaries=re.findall(r'^\s*Summary\s+\[[^\]]+\]\s+(\d+) tests run:\s*(.*)$',text,re.M);assert len(summaries)==1,'One completed fullNextest Summary required'
 total=int(summaries[0][0]);tail=summaries[0][1]
 counts={key:int(value) for value,key in re.findall(r'(\d+) (passed|failed|skipped)',tail)}
 assert counts.get('passed',0)==xml_passed and counts.get('failed',0)==xml_failed+xml_errors and total==len(cases) and xml_skipped==0
 assert not re.search(r'\b(leaked|timed out|canceled|cancelled|not run)\b',tail), 'Incomplete or abnormal Nextest summary'
 starts=re.findall(r'^\s*Starting (\d+) tests across (\d+) binaries(?: \((\d+) tests skipped[^)]*\))?\s*$',text,re.M);assert len(starts)==1
 assert int(starts[0][0])==total and int(starts[0][2] or '0')==counts.get('skipped',0)
 # Nextest Summary excluded/ignored tests are absent from selected-case JUnit, not XML skipped cases.
 required=['two_hundred_current_edits_reach_object_and_database_plateau','compact_measurements_compare_small_reads_from_large_catalog','manual_backlog_larger_than_one_page_advances_without_unprotected_deletion','measure_small_and_larger_sparse_catalog_edits','split_semantic_ready_destination_has_real_paid_vectors_and_complete_transports']
 required_log=[]
 for name in required:
  matched=[c for c in cases if c.attrib['name'].split('::')[-1]==name]
  assert len(matched)==1 and matched[0].find('skipped') is None,'Required full-workspace test absent/skipped: '+name
  lines=[line for line in text.splitlines() if re.match(r'^\s*PASS\s+\[[^\]]+\]',line) and line.split()[-1].split('::')[-1]==name]
  assert len(lines)==1,'Required full-workspace exactPASS log evidence absent/duplicated: '+name
  required_log.append(lines[0])
 record['adopted_full_debug']={'receipt':str(full_receipt_path),'receipt_sha256':sha(full_receipt_path),'log_sha256':sha(full_log),'junit':str(debug_junit),'junit_sha256':sha(debug_junit),'source_junit':str(source_junit),'source_junit_birth_ns':js.st_birthtime_ns,'source_junit_mtime_ns':js.st_mtime_ns,'full_log_birth_ns':birth,'completed_receipt_mtime_ns':completed,'cases':len(cases),'passed':xml_passed,'failed':xml_failed,'errors':xml_errors,'xml_skipped':xml_skipped,'selection_excluded_skipped':counts.get('skipped',0),'required_pass_lines':required_log,'executed_in_this_queue':False}
 record['adopted_debug_semantic']={'receipt':plan['adopted_debug_semantic'],'passed':1,'executed_in_this_queue':False}
 ui=Path(plan['separate_ui']['log']).read_text(encoding='utf-8');assert re.search(r'(?:\u2139|#) tests 795\b',ui) and re.search(r'(?:\u2139|#) pass 776\b',ui) and re.search(r'(?:\u2139|#) fail 0\b',ui) and re.search(r'(?:\u2139|#) skipped 19\b',ui)
 record['adopted_current_fmt_clippy']=plan['adopted_current_fmt_clippy'];record['separate_ui']=plan['separate_ui'];save()
 release=Path(plan['release_adoption']['receipt']);record['waiting_for_release_receipt']=str(release);save();started=time.monotonic()
 while not release.exists():
  assert time.monotonic()-started<7200,'Sole release producer receipt unavailable after bounded wait'
  time.sleep(3)
 r=receipt(release,SOURCE);assert r['cargo_incremental']=='0'
 cmd=r['command'];assert len(cmd)==6 and cmd[2:5]==['--profile','release','--destination']
 assert (WORK/cmd[1]).resolve()==(WORK/'scripts/build-service-runtime.py').resolve() and Path(cmd[5]).resolve()==runtime.resolve()
 pins();seal=sealer.seal_runtime(runtime,SOURCE);sealpath=BASE/'p14-runtime-final-r1-seal.json'
 if sealpath.exists():assert read(sealpath)==seal
 else:
  with sealpath.open('x',encoding='utf-8') as f:f.write(json.dumps(seal,indent=2)+'\n')
 record['adopted_release']={'receipt':str(release),'receipt_sha256':sha(release),'source':SOURCE,'seal_sha256':sha(sealpath),'executed_in_this_queue':False};save()
 for row in plan['commands']:
  pins();private(row['name']);save();env=os.environ.copy();env['CARGO_NET_OFFLINE']='true'
  for key in ['AIRK_P10_RESOURCE_OUTPUT_DIR','AIRK_P11B_RESOURCE_OUTPUT_DIR','AIRK_CATALOG_CANDIDATE_BINARY','AIRK_TEST_BINARY']:env.pop(key,None)
  env.update(row.get('environment',{}))
  if row.get('path_prepend'):env['PATH']=os.pathsep.join([*row['path_prepend'],env.get('PATH','')])
  assert env['AIRK_PRIVATE_CENSUS_BASE']==plan['private_base_proof']['path'];assert 'AIRK_P11B_RESOURCE_OUTPUT_DIR' not in env
  assert not Path(row['log']).exists() and not Path(row['log']).with_suffix('.receipt.json').exists(), 'Fresh immutable stage logs required'
  print('START',row['name'],flush=True);result=subprocess.run(row['command'],cwd=row['cwd'],env=env)
  rp=Path(row['log']).with_suffix('.receipt.json');value=read(rp) if rp.exists() else None
  error=None
  try:pins()
  except BaseException as exc:error=str(exc)
  record['steps'].append({'name':row['name'],'exit_code':result.returncode,'receipt':str(rp),'seal_or_source_error':error});save()
  assert error is None,error
  assert result.returncode==0 and value is not None,row['name']
  value=receipt(rp,SOURCE);assert value['command']==row['command'][3:],'Command receipt differs'
  if row.get('expected_passing_tests'):assert value['passing_tests']==row['expected_passing_tests'],'Test selection changed/zero'
  if row['name']=='latency-gates':
   report=read(BASE/'p14-final-gates-r3.json');assert report['passed'] is True and report['limits_changed'] is False and len(report['gates'])==84
  if row['name']=='dependency-audit':
   report=read(Path(row['expected_report']['path']));assert report['status']=='passed' and report['all_requested_checks_passed'] is True and report['fresh_full_advisory_check'] is True and report['network_mode']=='live advisories, locked vet imports'
   expected={'source_review','cargo_audit','npm_audit','cargo_deny','cargo_vet','advisory_dispositions','input_integrity'};assert set(report['checks'])==expected and all(c['status']=='passed' for c in report['checks'].values())
  save()
 pins();record['status']='passed';save()
except BaseException as exc:
 record['status']='failed';record['failure']=str(exc);save();raise
