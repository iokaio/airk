# SPDX-License-Identifier: Apache-2.0
#!/bin/sh
# Root-authorized isolated prerequisite provision only; no source/Cargo/product.
set -eu
umask 077
candidate=/Users/tylerjensen/airk-builds/advanced-split-local-delivery-prerequisites-r1
python=/Users/tylerjensen/Code/airk/artifacts/tools/neo-python-3.13.18/bin/python3.13
[ "$(/usr/bin/id -u)" = 501 ]
[ "$(/usr/bin/uname -m)" = arm64 ]
[ ! -e "$candidate" ] && [ ! -L "$candidate" ]
mkdir "$candidate"
mkdir "$candidate/downloads" "$candidate/tools" "$candidate/receipts"
"$python" -I -B - "$candidate" <<'PY'
import hashlib,json,sys,time
from pathlib import Path
root=Path(sys.argv[1]); index=Path('/Users/tylerjensen/Code/airk/.git/index')
(root/'receipts/before.json').write_text(json.dumps({'started_unix':time.time(),'original_git_index_sha256':hashlib.sha256(index.read_bytes()).hexdigest()}))
PY
finish() {
  code=$?
  trap - EXIT
  "$python" -I -B - "$candidate" "$code" <<'PY'
import hashlib,json,sys,time
from pathlib import Path
root=Path(sys.argv[1]); before=json.loads((root/'receipts/before.json').read_bytes())
index=Path('/Users/tylerjensen/Code/airk/.git/index')
receipt={'schema':'airk.private-neo-node-prerequisite/1','exit_code':int(sys.argv[2]),'elapsed_seconds':time.time()-before['started_unix'],'candidate':str(root),'original_git_index_before':before['original_git_index_sha256'],'original_git_index_after':hashlib.sha256(index.read_bytes()).hexdigest(),'no_cargo_nn_source_install_service_mutations':True}
receipt['original_git_index_unchanged']=receipt['original_git_index_before']==receipt['original_git_index_after']
detail=root/'receipts/node-verified.json'
if detail.is_file():receipt['node']=json.loads(detail.read_bytes())
receipt['result']='verified_prerequisite' if receipt['exit_code']==0 and detail.is_file() and receipt['original_git_index_unchanged'] else 'failed_or_unknown_prerequisite'
(root/'receipts/node-result.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
PY
  exit "$code"
}
trap finish EXIT
cd "$candidate/downloads"
/usr/bin/curl --fail --location --proto '=https' --tlsv1.2 --connect-timeout 20 --max-time 120 --output SHASUMS256.txt https://nodejs.org/dist/v24.15.0/SHASUMS256.txt
/usr/bin/curl --fail --location --proto '=https' --tlsv1.2 --connect-timeout 20 --max-time 120 --output node-v24.15.0-darwin-arm64.tar.gz https://nodejs.org/dist/v24.15.0/node-v24.15.0-darwin-arm64.tar.gz
"$python" -I -B - "$candidate" <<'PY'
import hashlib,json,posixpath,re,subprocess,sys,tarfile
from pathlib import Path,PurePosixPath
root=Path(sys.argv[1]); name='node-v24.15.0-darwin-arm64.tar.gz'; package='node-v24.15.0-darwin-arm64'
checksums=(root/'downloads/SHASUMS256.txt').read_bytes()
rows=[line.split() for line in checksums.decode('ascii').splitlines() if line.split()[-1:]==[name]]
assert len(rows)==1 and len(rows[0])==2 and re.fullmatch('[0-9a-f]{64}',rows[0][0])
archive=root/'downloads'/name; actual=hashlib.sha256(archive.read_bytes()).hexdigest()
assert actual==rows[0][0], 'Official Node archive SHA-256 mismatch'
with tarfile.open(archive,'r:gz') as stream:
 members=stream.getmembers()
 assert members and len({member.name for member in members})==len(members)
 for member in members:
  relative=PurePosixPath(member.name)
  assert not relative.is_absolute() and relative.parts[0]==package and '..' not in relative.parts
  assert member.isfile() or member.isdir() or member.issym() or member.islnk()
  if member.issym() or member.islnk():
   linked=posixpath.normpath(posixpath.join(str(relative.parent),member.linkname) if member.issym() else member.linkname)
   assert linked.startswith(package+'/') and not member.linkname.startswith('/')
 stream.extractall(root/'tools',filter='data')
tool=root/'tools'/package/'bin/node'; npm=root/'tools'/package/'lib/node_modules/npm/bin/npm-cli.js'
version=subprocess.check_output([str(tool),'--version'],text=True,timeout=30).strip()
assert version=='v24.15.0'
npm_version=subprocess.check_output([str(tool),str(npm),'--version'],text=True,timeout=30).strip()
receipt={'archive_url':'https://nodejs.org/dist/v24.15.0/'+name,'checksum_url':'https://nodejs.org/dist/v24.15.0/SHASUMS256.txt','checksum_file_sha256':hashlib.sha256(checksums).hexdigest(),'official_archive_sha256':rows[0][0],'actual_archive_sha256':actual,'archive_bytes':archive.stat().st_size,'node_path':str(tool),'node_sha256':hashlib.sha256(tool.read_bytes()).hexdigest(),'node_version':version,'npm_cli':str(npm),'npm_version':npm_version,'archive_members':len(members),'source_checkout_written':False}
(root/'receipts/node-verified.json').write_text(json.dumps(receipt,indent=2)+'\n')
PY
