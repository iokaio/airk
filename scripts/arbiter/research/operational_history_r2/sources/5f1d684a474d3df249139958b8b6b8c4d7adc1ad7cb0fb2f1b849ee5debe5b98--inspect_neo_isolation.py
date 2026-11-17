# SPDX-License-Identifier: Apache-2.0
"""Read-only source-isolation/static-input inventory. Never prints SSH secrets."""
import hashlib
import json
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
packet = ROOT / "artifacts/v190/neo-connection.local.json"
connection = json.loads(packet.read_bytes())
remote = r'''import hashlib,json,os,pwd,subprocess
from pathlib import Path
home=Path(pwd.getpwuid(os.geteuid()).pw_dir); repo=home/'code/airk'
env=dict(os.environ,GIT_OPTIONAL_LOCKS='0')
def digest(path):
 h=hashlib.sha256()
 with path.open('rb') as stream:
  for block in iter(lambda:stream.read(1048576),b''):h.update(block)
 return h.hexdigest()
def run(argv,cwd=None):
 x=subprocess.run(argv,cwd=cwd,env=env,capture_output=True,text=True,timeout=30)
 return {'exit':x.returncode,'stdout':x.stdout[:30000],'stderr':x.stderr[:1000]}
def file(path):
 result={'exists':path.exists(),'symlink':path.is_symlink(),'path':str(path)}
 if path.is_file():result.update(bytes=path.stat().st_size,sha256=digest(path))
 return result
index=repo/'.git/index'; before=file(index)
report={'schema':'airk.private-neo-isolated-delivery-preparation/1','read_only':True,
 'head':run(['/usr/bin/git','rev-parse','HEAD'],repo),
 'worktrees':run(['/usr/bin/git','worktree','list','--porcelain'],repo),
 'common_git_dir':run(['/usr/bin/git','rev-parse','--git-common-dir'],repo),
 'status':run(['/usr/bin/git','status','--porcelain=v1','--untracked-files=no'],repo),
 'disk':run(['/bin/df','-Pk',str(home)]),'index_before':before}
paths=[repo/'artifacts/tools/neo-python-3.13.18/bin/python3.13',Path('/opt/homebrew/bin/node'),
 Path('/opt/homebrew/bin/npm'),home/'airk-builds',repo/'artifacts/advanced-split/ort130-arm64-env.json',
 repo/'artifacts/advanced-split/ort130-arm64/owner.json',repo/'artifacts/advanced-split/ort130-arm64/build-receipt.json',
 repo/'artifacts/advanced-split/ort130-arm64/static-aggregate-receipt.json',
 repo/'artifacts/advanced-split/ort130-arm64/cargo-aggregate-env.json',
 repo/'artifacts/advanced-split/bundled-model-v2/minilm-l6-v2-u8-v2/model.json',
 repo/'artifacts/advanced-split/bundled-model-v2/minilm-l6-v2-s8-v2/model.json']
report['paths']={str(path):file(path) for path in paths}
report['static_environments']={}
for path in paths:
 if path.is_file() and path.name in ('ort130-arm64-env.json','cargo-aggregate-env.json'):
  value=json.loads(path.read_bytes());report['static_environments'][str(path)]={key:item for key,item in value.get('environment',{}).items() if key in ('LIBONNXRUNTIME_NO_PKG_CONFIG','ORT_LIB_LOCATION','ORT_LIB_PROFILE','ORT_PREFER_DYNAMIC_LINK','ORT_SKIP_DOWNLOAD')}
for tool in (repo/'artifacts/tools/neo-python-3.13.18/bin/python3.13',Path('/opt/homebrew/bin/node')):
 if tool.is_file():report[str(tool)+'_version']=run([str(tool),'--version'])
report['index_after']=file(index);report['existing_index_unchanged']=before==report['index_after']
report['candidate_isolated_path']=str(home/'airk-builds/advanced-split-local-delivery-r1')
report['candidate_isolated_path_exists']=(home/'airk-builds/advanced-split-local-delivery-r1').exists()
print(json.dumps(report))
'''
argv = ['ssh', '-p', str(connection['port']), '-i', str(connection['identity_path']),
        *connection['ssh_options'], connection['user'] + '@' + connection['host'], '/usr/bin/python3', '-']
start = time.monotonic()
result = subprocess.run(argv, input=remote, capture_output=True, text=True, timeout=60)
receipt = {'ssh_exit': result.returncode, 'elapsed_seconds': time.monotonic() - start,
           'credentials_printed': False, 'connection_file_sha256': hashlib.sha256(packet.read_bytes()).hexdigest()}
if result.returncode == 0:
    receipt['observation'] = json.loads(result.stdout)
else:
    receipt['error'] = 'SSH observation failed; no source or product mutation attempted'
out = Path(__file__).with_name('neo-isolation-readonly.json')
assert not out.exists()
out.write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
if result.returncode == 0:
    value = receipt['observation']
    print(json.dumps({'ssh_exit': 0, 'existing_index_unchanged': value['existing_index_unchanged'],
                      'candidate_isolated_path_exists': value['candidate_isolated_path_exists'],
                      'static_environments': value['static_environments']}))
else:
    print(json.dumps(receipt))
raise SystemExit(result.returncode)
