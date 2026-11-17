# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,plistlib,subprocess,tarfile,time
from pathlib import Path
run=Path(__file__).resolve().parent;source=run/'source';evidence=run/'evidence'
assert run.parent==Path.home()/'airk-builds' and run.name.startswith('explorer-1812-')
assert (run/'help-ui-followup.json').exists() and not (run/'target').exists()
delta=json.loads((run/'ui2-delta.json').read_text());allowed={'scripts/check-installed-explorer.cjs','scripts/check-extension-update-editor.cjs'}
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
assert set(delta['files'])==allowed and sha(run/'ui2-delta.tar.gz')==delta['archive_sha256']
for n,row in delta['files'].items():assert sha(source/n)==row['before']
with tarfile.open(run/'ui2-delta.tar.gz') as archive:
 assert {m.name for m in archive.getmembers()}==allowed and all(m.isfile() for m in archive.getmembers());archive.extractall(source,filter='data')
for n,row in delta['files'].items():assert sha(source/n)==row['after']
tools=Path.home()/'airk-builds/timing-1.6.4.PutU1P/tools';node=tools/'node-v24.15.0-darwin-arm64/bin/node'
env=dict(os.environ,PATH=str(node.parent)+':'+str(tools)+':'+str(Path.home()/'.cargo/bin')+':'+os.environ.get('PATH',''),PYTHONDONTWRITEBYTECODE='1')
contents=Path('/Applications/Visual Studio Code.app/Contents');executable=contents/'MacOS'/plistlib.loads((contents/'Info.plist').read_bytes())['CFBundleExecutable'];cli=contents/'Resources/app/out/cli.js'
rows=[]
for name,args in [('installed-explorer-short-profile',['scripts/check-installed-explorer.cjs',str(executable),str(cli),'artifacts/neo/vscode-help-corrected/publish/airk-1.8.12-darwin-arm64.vsix']),('bootstrap-ui-short-profile',['scripts/check-extension-update-editor.cjs',str(executable)])]:
 (run/'ui2-phase.txt').write_text(name);start=time.monotonic()
 with (evidence/(name+'.log')).open('x') as out:result=subprocess.run([str(node),*args],cwd=source,env=env,stdout=out,stderr=subprocess.STDOUT,timeout=200)
 rows.append({'name':name,'command':[str(node),*args],'exit_code':result.returncode,'elapsed_seconds':round(time.monotonic()-start,3)});(run/'ui2-steps.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows[-1]),flush=True)
with (evidence/'cargo-clean-ui2.log').open('x') as out:clean=subprocess.run(['cargo','clean','--manifest-path',str(source/'Cargo.toml'),'--target-dir',str(run/'target')],cwd=source,env=env,stdout=out,stderr=subprocess.STDOUT)
report={'schema':'airk.neo-short-profile-ui/1','status':'passed' if all(row['exit_code']==0 for row in rows) and clean.returncode==0 else 'failed','reviewed_harness_delta':delta,'native_rebuilt':False,'production_vsix_repackaged':False,'steps':rows,'cargo_clean_exit_code':clean.returncode,'target_absent':not (run/'target').exists(),'scope':'Mac actual production installed-controller/UI (native execution disabled) and separate fictional bootstrap handoff fixture; no notarization/publication.'}
(run/'ui2-result.json').write_text(json.dumps(report,indent=2)+'\n');(run/'ui2-phase.txt').write_text('complete');print(json.dumps(report),flush=True)
