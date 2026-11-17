# SPDX-License-Identifier: Apache-2.0
import hashlib,io,json,subprocess,tarfile,time
from pathlib import Path

root=Path('D:/code/Github/airk');out=root/'artifacts/pr60-fixes-r1'
connection=json.loads((root/'artifacts/v190/neo-connection.local.json').read_text())
ssh=['ssh','-i',connection['identity_path'],*connection['ssh_options'],connection['user']+'@'+connection['host']]
scp=['scp','-i',connection['identity_path'],*connection['ssh_options']]
def remote(command):
    return subprocess.run([*ssh,command],capture_output=True,text=True,timeout=180)
archive=out/'neo-source.tar.gz';assert not archive.exists()
names=subprocess.check_output(['git','-c','safe.directory='+root.as_posix(),'-C',str(root),'ls-files','--cached','--others','--exclude-standard','-z']).split(b'\0')
excluded={'.git','.codex','.agents','.aws','.mcp.json','artifacts','target','temp'}
inputs={}
with tarfile.open(archive,'x:gz') as tar:
    selected={root/n.decode() for n in names if n and Path(n.decode()).parts[0] not in excluded}
    for directory in ['packages/vscode/out','packages/vscode/node_modules/jsonc-parser','packages/vscode/node_modules/preact']:
        selected.update(p for p in (root/directory).rglob('*') if p.is_file())
    for path in sorted(selected):
        assert not path.is_symlink() and not path.is_junction() and path.resolve().is_relative_to(root)
        if not path.is_file():continue
        name=path.relative_to(root).as_posix();data=path.read_bytes();inputs[name]=hashlib.sha256(data).hexdigest()
        item=tarfile.TarInfo(name);item.size=len(data);item.mode=0o600;tar.addfile(item,io.BytesIO(data))
    script=b'''#!/bin/sh
set -u
case "$PWD" in /Users/tylerjensen/airk-builds/pr60-ui.*) ;; *) exit 97;; esac
test ! -L target || exit 98
node_rc=125
finish() {
 /Users/tylerjensen/.cargo/bin/cargo clean --offline --locked --manifest-path "$PWD/Cargo.toml" --target-dir "$PWD/target" >cargo-clean.log 2>&1
 clean_rc=$?
 removed=true; test ! -e "$PWD/target" || removed=false
 printf '{"node_exit":%s,"cargo_clean_exit":%s,"target_removed":%s}\n' "$node_rc" "$clean_rc" "$removed" >result.json
}
trap finish EXIT
trap 'exit 129' HUP INT TERM
/opt/homebrew/bin/node --version >node-version.txt
/opt/homebrew/bin/node --test --test-reporter=tap --test-concurrency=2 packages/vscode/out/test/cliRemove.test.js packages/vscode/out/test/serviceBootstrap.test.js packages/vscode/out/test/serviceConsole.test.js packages/vscode/out/test/serviceOnboarding.test.js >node.log 2>&1
node_rc=$?
exit "$node_rc"
'''
    item=tarfile.TarInfo('run.sh');item.size=len(script);item.mode=0o700;tar.addfile(item,io.BytesIO(script))
created=remote('umask 077; mktemp -d /Users/tylerjensen/airk-builds/pr60-ui.XXXXXX');assert created.returncode==0,created.stderr
directory=created.stdout.strip();assert directory.startswith('/Users/tylerjensen/airk-builds/pr60-ui.') and '\n' not in directory and ' ' not in directory
digest=hashlib.sha256(archive.read_bytes()).hexdigest();start=time.monotonic()
subprocess.run([*scp,str(archive),connection['user']+'@'+connection['host']+':'+directory+'/source.tar.gz'],check=True,capture_output=True)
result=remote("cd '"+directory+"' && test \"$(shasum -a 256 source.tar.gz | cut -d ' ' -f 1)\" = '"+digest+"' && tar -xzf source.tar.gz && /bin/sh run.sh")
reports={}
for name in ['result.json','node.log','node-version.txt','cargo-clean.log']:
    path=out/('neo-'+name)
    copied=subprocess.run([*scp,connection['user']+'@'+connection['host']+':'+directory+'/'+name,str(path)],capture_output=True,text=True)
    assert copied.returncode==0,copied.stderr
    reports[name]=hashlib.sha256(path.read_bytes()).hexdigest()
summary=json.loads((out/'neo-result.json').read_text())
value={'remote_directory':directory,'archive_sha256':digest,'inputs':inputs,'seconds':time.monotonic()-start,'ssh_exit':result.returncode,'stderr':result.stderr,'result':summary,'reports':reports,'node_version':(out/'neo-node-version.txt').read_text().strip(),'scope':'Four owning extension suites only; no native runtime or Cargo build; cargo clean still performed after tests'}
with (out/'neo-batch.json').open('x',encoding='utf-8') as stream:json.dump(value,stream,indent=2)
print(json.dumps({k:v for k,v in value.items() if k!='inputs'}))
assert result.returncode==0 and summary=={'node_exit':0,'cargo_clean_exit':0,'target_removed':True}
