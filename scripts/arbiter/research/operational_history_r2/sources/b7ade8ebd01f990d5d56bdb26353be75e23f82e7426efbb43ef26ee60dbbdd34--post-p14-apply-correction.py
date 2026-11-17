# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import argparse, hashlib, json

B=Path(__file__).resolve().parent
W=B/'p10-worktree'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
p=argparse.ArgumentParser()
p.add_argument('--proposal',required=True)
p.add_argument('--manifest-sha256',required=True)
p.add_argument('--receipt',required=True)
p.add_argument('--content-dir',choices=['source','files'],default='source')
a=p.parse_args()
d=(B/a.proposal).resolve(strict=True)
assert d.is_relative_to(B) and d!=B
m=d/'manifest.json'
assert sha(m)==a.manifest_sha256
v=json.loads(m.read_text(encoding='utf-8'))
out=B/a.receipt
assert not out.exists()
rows=[]
for row in v['files']:
    rel=Path(row['path'])
    assert not rel.is_absolute() and '..' not in rel.parts
    src=d/a.content_dir/rel
    dst=W/rel
    assert dst.parent.resolve().is_relative_to(W.resolve())
    if row['base_sha256'] is None:
        assert not dst.exists() and not dst.is_symlink()
    else:
        assert dst.is_file() and sha(dst)==row['base_sha256']
    assert src.is_file() and sha(src)==row['proposed_sha256']
    rows.append((row,src,dst))
backup=out.with_suffix('')
backup.mkdir(exist_ok=False)
for row,src,dst in rows:
    if row['base_sha256'] is None:continue
    saved=backup/row['path']
    saved.parent.mkdir(parents=True,exist_ok=True)
    saved.write_bytes(dst.read_bytes())
for row,src,dst in rows:
    if row['base_sha256'] is None:
        assert not dst.exists() and not dst.is_symlink()
    else:
        assert sha(dst)==row['base_sha256']
    dst.write_bytes(src.read_bytes())
    assert sha(dst)==row['proposed_sha256']
with out.open('x',encoding='utf-8') as f:
    json.dump({'status':'applied','proposal':str(d),'manifest_sha256':sha(m),'files':v['files']},f,indent=2)
print(json.dumps({'status':'applied','files':len(rows),'receipt':str(out)}))
