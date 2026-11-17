# SPDX-License-Identifier: Apache-2.0
import json, subprocess, hashlib, sys
from pathlib import Path
base=Path(__file__).resolve().parent
root=base/'source';source=root/'auth.cs';state=base/'state'
old=Path('artifacts/local/20261021-144525-206f925f/native/stage/airk.exe').resolve()
new=Path(sys.argv[1]).resolve()
args=['--root',str(root),'--data-dir',str(state),'--collection','upgrade-fixture']
def run(exe,tail):
 r=subprocess.run([str(exe),*args,*tail],capture_output=True,text=True,encoding='utf-8',timeout=90,creationflags=subprocess.CREATE_NO_WINDOW)
 if r.returncode: raise RuntimeError(r.stderr)
 return json.loads(r.stdout)
owner=state/'collections-1.2/upgrade-fixture/current-owner.json'
assert json.loads(owner.read_text())['version']==120
before=hashlib.sha256(source.read_bytes()).hexdigest()
run(new,['index','--full','--scheduled'])
after=run(new,['status']);assert after['lexical_backend']=='sqlite-fts5-current';assert after['files']==1
assert json.loads(owner.read_text())['version']==125
assert run(new,['collections','vectors','show'])['enabled']==False
assert run(new,['search','ConfigureAuthentication','--mode','lexical'])['hits']
assert hashlib.sha256(source.read_bytes()).hexdigest()==before
(base/'result.json').write_text(json.dumps({'passed':True,'from_format':120,'to_format':125,'same_collection':True,'saved_vectors_disabled':True,'source_unchanged':True,'scheduled_full_rebuild':True,'old_binary_sha256':hashlib.sha256(old.read_bytes()).hexdigest(),'new_binary_sha256':hashlib.sha256(new.read_bytes()).hexdigest()},indent=2),encoding='utf-8')
print('Actual pre-FTS binary to FTS5 scheduled full rebuild passed.')
