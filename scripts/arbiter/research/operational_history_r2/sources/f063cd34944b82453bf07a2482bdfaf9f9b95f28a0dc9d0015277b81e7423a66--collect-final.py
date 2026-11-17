# SPDX-License-Identifier: Apache-2.0
import pathlib,json,hashlib,re,subprocess,platform,datetime
run=pathlib.Path(__file__).resolve().parent;runtime=run/'runtime-ownership-final'
report=json.loads((run/'qualification-ownership-final.json').read_text());build=json.loads((runtime/'airk-build.json').read_text());pair=json.loads((runtime/'airk-runtime.json').read_text());install=json.loads((runtime/'airk-install.json').read_text())
assert build['profile']=='debug' and build['source']==report['native_source']
assert all(hashlib.file_digest((runtime/n).open('rb'),'sha256').hexdigest()==h for n,h in install['files'].items()),'sealed payload changed'
logs={p.name:{'sha256':hashlib.file_digest(p.open('rb'),'sha256').hexdigest(),'test_summaries':re.findall(r'test result: [^\r\n]+',p.read_text(errors='replace'))} for p in run.glob('r7-*.log')}
signatures={}
for name in ['airk','airk-embed']:
 r=subprocess.run(['/usr/bin/codesign','-dv','--verbose=2',str(runtime/'AIRK.app/Contents/MacOS'/name)],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 signatures[name]={'exit_code':r.returncode,'metadata':[line for line in r.stdout.splitlines() if line.startswith(('Signature=','TeamIdentifier=','CodeDirectory '))]}
processes=[]
for line in subprocess.check_output(['/bin/ps','-Ao','pid=,ppid=,command='],text=True).splitlines():
 parts=line.strip().split(None,2)
 if len(parts)==3 and str(run)+'/runtime-' in parts[2] and '/AIRK.app/Contents/MacOS/' in parts[2]:processes.append({'pid':int(parts[0]),'parent_pid':int(parts[1])})
summary={'schema':'airk.p7.neo-focused-correctness/1','collected_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'platform':platform.platform(),'arch':platform.machine(),'scope':'native correctness and process ownership only; no Mac performance or release qualification','profile':build['profile'],'native_source':build['source'],'pair':pair,'install_manifest_sha256':hashlib.file_digest((runtime/'airk-install.json').open('rb'),'sha256').hexdigest(),'sealed_payload_files':len(install['files']),'sealed_payload_verified':True,'development_signatures':signatures,'developer_id_notarization_release_qualification':False,'source_before':report['source_before'],'source_after':report['source_after'],'status':report['status'],'logs':logs,'remaining_owned_runtime_processes':processes,'source_97_prior_scoped_gate_results_retained':True}
(run/'final-summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))
