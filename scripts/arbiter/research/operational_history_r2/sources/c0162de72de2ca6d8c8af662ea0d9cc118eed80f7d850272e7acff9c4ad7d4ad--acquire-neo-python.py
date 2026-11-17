# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess,tarfile
from pathlib import Path
root=Path('artifacts/advanced-split/tools/neo-python-source');root.mkdir(parents=True,exist_ok=True)
p=root/'cpython-3.13.18+20261110-aarch64-apple-darwin-install_only_stripped.tar.gz'
if not p.exists():
 with p.open('xb') as out:
  r=subprocess.run(['gh','api','repos/astral-sh/python-build-standalone/releases/assets/733712223','-H','Accept: application/octet-stream'],stdout=out,stderr=subprocess.PIPE,timeout=120)
 if r.returncode:raise RuntimeError(r.stderr.decode())
assert p.stat().st_size==25479437
assert hashlib.sha256(p.read_bytes()).hexdigest()=='7a2384641584ab9984221fcb0fc9265df465922af71be71410529e2a9c3f0540'
with tarfile.open(p,'r:gz') as t:
 members=t.getmembers();print(json.dumps({'members':len(members),'bytes':sum(m.size for m in members),'links':[(m.name,m.linkname) for m in members if m.issym() or m.islnk()],'licenses':[m.name for m in members if any(s in m.name.lower() for s in ('license','copyright','notice'))]},indent=2))
