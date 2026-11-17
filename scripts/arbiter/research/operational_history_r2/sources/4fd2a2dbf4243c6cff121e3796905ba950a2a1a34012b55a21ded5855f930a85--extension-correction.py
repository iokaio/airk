# SPDX-License-Identifier: Apache-2.0
import hashlib,importlib.util,json,os,subprocess,tarfile
from pathlib import Path
run=Path(__file__).resolve().parent;record=json.loads((run/'snapshot.json').read_bytes());patch=json.loads((run/'extension-correction.json').read_bytes());assert hashlib.sha256((run/'snapshot.json').read_bytes()).hexdigest()==patch['base_snapshot_sha256'];root=run/'extension-correction-source';root.mkdir(exist_ok=False)
assert hashlib.sha256((run/'source.tar.gz').read_bytes()).hexdigest()==record['archive_sha256']
with tarfile.open(run/'source.tar.gz') as packed:packed.extractall(root,filter='data')
subprocess.run(['git','read-tree','HEAD'],cwd=root,check=True)
body=(run/'cliInstall.test.ts.corrected').read_bytes();assert hashlib.sha256(body).hexdigest()==patch['changed_source_sha256'];(root/patch['changed_source_file']).write_bytes(body)
def verify():
 for name,entry in record['public_files'].items():
  expected=patch['changed_source_sha256'] if name==patch['changed_source_file'] else entry['sha256'];assert hashlib.sha256((root/name).read_bytes()).hexdigest()==expected,name
 spec=importlib.util.spec_from_file_location('builder',root/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder);assert builder.source_identity()==record['native_source']
verify();environment=dict(os.environ,PATH='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/node-v24.15.0-darwin-arm64/bin:'+os.environ['PATH']);environment.pop('AIRK_TEST_BINARY',None);environment.pop('AIRK_EDITOR_RUN',None)
for name,args in [('npm-inputs',['npm','ci','--offline','--ignore-scripts','--no-audit','--no-fund','--prefix','packages/vscode']),('tests',['npm','test','--prefix','packages/vscode'])]:
 with (run/('extension-correction-'+name+'.log')).open('w') as output:result=subprocess.run(args,cwd=root,env=environment,stdout=output,stderr=subprocess.STDOUT)
 assert result.returncode==0,(name,result.returncode)
verify();(run/'extension-correction-result.json').write_text(json.dumps({'passed':True,'source_unchanged_except_declared_test':True,'native_inputs_unchanged':record['native_source'],'test_umask':'077 (owned shell)'}));print('Corrected full extension suite passed with source correspondence verified',flush=True)
