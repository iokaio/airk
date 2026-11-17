# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,hashlib,json,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[2];NODE=ROOT/'artifacts/tools/node-v24.15.0-win-x64/node.exe';OUT=ROOT/'artifacts/advanced-split'
files=[p for name in ('packages/admin/src','packages/admin/test','packages/ui/src','packages/ui/test','packages/vscode/src/workbenchWebview','packages/vscode/src/explorerWebview') for p in (ROOT/name).rglob('*') if p.is_file()]
def hashes():return {p.relative_to(ROOT).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(files)}
command=[str(NODE),'--test','packages/ui/test/answer.dom.test.mjs','packages/ui/test/help.dom.test.mjs','packages/ui/test/management.dom.test.mjs','packages/ui/test/outerManagement.dom.test.mjs','packages/admin/test/r22Picker.dom.test.mjs']
report={'argv':command,'cwd':str(ROOT),'before_sha256':hashes(),'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'Ten real browser DOM regressions, synthetic host responses except actual Rust Usage fixture; not native/installed parity.'}
start=time.perf_counter();r=subprocess.run(command,cwd=ROOT,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=60)
(OUT/'p6-final-browser.log').write_bytes(r.stdout);report.update(exit_code=r.returncode,elapsed_seconds=time.perf_counter()-start,after_sha256=hashes(),output=r.stdout.decode('utf-8','replace'))
report['passed']=r.returncode==0 and report['before_sha256']==report['after_sha256'] and 'tests 10' in report['output'] and 'pass 10' in report['output']
(OUT/'p6-final-browser.json').write_text(json.dumps(report,indent=2)+'\n');print('Browser receipt passed:',report['passed']);sys.exit(0 if report['passed'] else 1)
