# SPDX-License-Identifier: Apache-2.0
import base64,json,shlex,subprocess,time
from pathlib import Path
cfg=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());assert cfg['host']=='192.168.68.61'
out=Path('artifacts/local/one-airk-1814')/('neo-nextest-preparation-'+str(int(time.time())));out.mkdir()
code='''
import hashlib,json,os,stat,subprocess
from pathlib import Path
path=Path('/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/tools/cargo-nextest')
metadata=path.lstat();assert stat.S_ISREG(metadata.st_mode) and metadata.st_uid==501 and not path.is_symlink()
result={'path':str(path),'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'bytes':metadata.st_size,'readonly_version_help_only':True,'cargo_builds':False}
version=subprocess.run([str(path),'--version'],capture_output=True,text=True,timeout=20)
result['version']={'exit_code':version.returncode,'stdout':version.stdout.strip(),'stderr':version.stderr[-1024:]}
env={**os.environ,'PATH':str(Path.home()/'.cargo/bin')+':'+str(path.parent)+':'+os.environ.get('PATH','')}
help=subprocess.run(['cargo','nextest','run','--help'],capture_output=True,text=True,timeout=20,env=env)
result['help_exit']=help.returncode;result['help_relevant_lines']=[line for line in help.stdout.splitlines() if any(token in line for token in ['--timings','--lib','--test ','--profile','--test-threads','--no-fail-fast','--no-run'])];result['help_stderr']=help.stderr[-1024:]
print(json.dumps(result,indent=2))
'''
python='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13'
command=shlex.quote(python)+' -c '+shlex.quote('import base64;exec(base64.b64decode('+repr(base64.b64encode(code.encode()).decode())+'))')
p=subprocess.run(['ssh','-i',cfg['identity_path'],*cfg['ssh_options'],cfg['user']+'@'+cfg['host'],command],capture_output=True,timeout=50)
(out/'receipt.json').write_bytes(p.stdout);(out/'stderr.txt').write_bytes(p.stderr)
print('EVIDENCE '+str(out));print(p.stdout.decode());print(p.stderr.decode()[-1000:]);raise SystemExit(p.returncode)
