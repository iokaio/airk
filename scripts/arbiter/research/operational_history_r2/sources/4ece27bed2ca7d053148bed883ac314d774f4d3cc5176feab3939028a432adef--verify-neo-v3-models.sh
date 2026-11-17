# SPDX-License-Identifier: Apache-2.0
#!/bin/sh
set -eu
umask 077
[ "$(/usr/bin/id -u)" = 501 ]
/Users/tylerjensen/Code/airk/artifacts/tools/neo-python-3.13.18/bin/python3.13 -I -B - <<'PY'
import base64,hashlib,json,stat,tarfile,time
from pathlib import Path
root=Path('/Users/tylerjensen/airk-builds/advanced-split-local-delivery-prerequisites-r1')
assert root.is_dir() and not root.is_symlink()
packet=json.loads(base64.b64decode('eyJmaWxlcyI6IHsibWluaWxtLWw2LXYyLXU4LXYzL0xJQ0VOU0UiOiB7ImJ5dGVzIjogMTEzNTgsICJzaGEyNTYiOiAiY2ZjNzc0OWI5NmY2M2JkMzFjM2M0MmI1YzQ3MWJmNzU2ODE0MDUzZTg0N2MxMGYzZWIwMDM0MTdiYzUyM2QzMCJ9LCAibWluaWxtLWw2LXYyLXU4LXYzL21vZGVsLmpzb24iOiB7ImJ5dGVzIjogNzE0LCAic2hhMjU2IjogIjIxZGE0ZGIzMThmYTA2MzQ1MGQ1NWE0MGYzZTJkYzljNjA5N2IyOTUwZDM1MTBlN2ZmNjA1NWYwZTVlYjg5MjQifSwgIm1pbmlsbS1sNi12Mi11OC12My9tb2RlbC5vbm54IjogeyJieXRlcyI6IDIzMDQ2Nzg5LCAic2hhMjU2IjogImI5NDFiZjE5ZjFmMTI4MzY4MGY0NDlmYTZhNzMzNmJiNTYwMGJkY2Q1Zjg0ZDEwZGRjNWNkNzIyMThhMGZkMjEifSwgIm1pbmlsbS1sNi12Mi11OC12My90b2tlbml6ZXIuanNvbiI6IHsiYnl0ZXMiOiA0NjYyNDcsICJzaGEyNTYiOiAiYmU1MGMzNjI4ZjJiZjViYjVlM2E3ZjE3YjFmNzQ2MTFiMjU2MWEzYTI3ZWVhYjA1ZTVhYTMwZjQxMTU3MjAzNyJ9LCAibWluaWxtLWw2LXYyLXM4LXYzL0xJQ0VOU0UiOiB7ImJ5dGVzIjogMTEzNTgsICJzaGEyNTYiOiAiY2ZjNzc0OWI5NmY2M2JkMzFjM2M0MmI1YzQ3MWJmNzU2ODE0MDUzZTg0N2MxMGYzZWIwMDM0MTdiYzUyM2QzMCJ9LCAibWluaWxtLWw2LXYyLXM4LXYzL21vZGVsLmpzb24iOiB7ImJ5dGVzIjogNzE0LCAic2hhMjU2IjogIjMzM2I2YmFiYWFhZThiZGEyMjllOTljNTMzYTlhZDU3NTI0NzA0NTY2ZWFkYjkwMmZlNTJmMzdmMzhjNzhjZTUifSwgIm1pbmlsbS1sNi12Mi1zOC12My9tb2RlbC5vbm54IjogeyJieXRlcyI6IDIzMDI2MDUzLCAic2hhMjU2IjogIjQyNzgzMzdmZDBmZjNjNjhiZmI2MjkxMDQyY2FkOGFiMzYzZTFkOWZiYzQzZGNiNDk5ZmU5MWM4NzE5MDI0NzQifSwgIm1pbmlsbS1sNi12Mi1zOC12My90b2tlbml6ZXIuanNvbiI6IHsiYnl0ZXMiOiA0NjYyNDcsICJzaGEyNTYiOiAiYmU1MGMzNjI4ZjJiZjViYjVlM2E3ZjE3YjFmNzQ2MTFiMjU2MWEzYTI3ZWVhYjA1ZTVhYTMwZjQxMTU3MjAzNyJ9fSwgImFyY2hpdmVfc2hhMjU2IjogIjI1ZjAyNGQ5ODYzZTg2ZDcxY2U3YWJkNjEyZjU5ZjE2NmM0MDA2YTM4NzVlOTk1ZDE1YjE2MmM1OTk4OWNkZjIiLCAiYXJjaGl2ZV9ieXRlcyI6IDQ3MDQyNTYwLCAiY2F0YWxvZ19zaGEyNTYiOiAiODIwMzM1MzY4ZGMwYTQyNzU0MWQwMTJmYzQyZmVjZWU4Mzg3ZjllYmExODlkMzViMDg2OGY5YTkyNjU3YTE4MyIsICJzb3VyY2UiOiAicm9vdC1hdXRob3JpemVkIGV4aXN0aW5nIGJ1bmRsZWQtbW9kZWwtdjM7IG5vdCBwYXJlbnQtYmFnIGV4cG9ydDsgY3VycmVudCBvcmlnaW5hbCB2MyBjYXRhbG9nIn0=')); expected=packet['files']
started=time.monotonic(); index=Path('/Users/tylerjensen/Code/airk/.git/index')
before=hashlib.sha256(index.read_bytes()).hexdigest()
node_receipt=json.loads((root/'receipts/node-result.json').read_bytes())
assert node_receipt['result']=='verified_prerequisite'
archive=root/'model-transfer-v3/original-minilm-v3-eight-files.tar'
assert archive.is_file() and not archive.is_symlink() and archive.stat().st_size==packet['archive_bytes']
with archive.open('rb') as stream:assert hashlib.file_digest(stream,'sha256').hexdigest()==packet['archive_sha256']
inputs=root/'inputs'
assert inputs.is_dir() and not inputs.is_symlink() and inputs.resolve()==inputs
destination=inputs/'bundled-model-v3'
assert not destination.exists() and not destination.is_symlink()
payload={}
with tarfile.open(archive,'r:') as stream:
 members=stream.getmembers()
 assert len(members)==8 and len({member.name for member in members})==8 and {member.name for member in members}==set(expected)
 for member in members:
  assert member.isfile() and not member.issym() and not member.islnk()
  data=stream.extractfile(member).read(); row=expected[member.name]
  assert len(data)==row['bytes'] and hashlib.sha256(data).hexdigest()==row['sha256']
  payload[member.name]=data
destination.mkdir(parents=True)
for name,data in payload.items():
 path=destination/name; assert path.is_relative_to(destination)
 path.parent.mkdir(exist_ok=True)
 with path.open('xb') as stream:stream.write(data)
actual={}
for path in destination.rglob('*'):
 if path.is_file():
  assert not path.is_symlink() and stat.S_ISREG(path.stat().st_mode) and path.stat().st_nlink==1
  actual[path.relative_to(destination).as_posix()]={'bytes':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
assert actual==expected
after=hashlib.sha256(index.read_bytes()).hexdigest(); assert before==after
receipt={'schema':'airk.private-neo-v3-model-prerequisite/1','result':'exact_original_eight_files_verified','elapsed_seconds':time.monotonic()-started,'directory':str(destination),'files':actual,'archive_sha256':packet['archive_sha256'],'source_catalog_sha256':packet['catalog_sha256'],'source':packet['source'],'original_git_index_before':before,'original_git_index_after':after,'original_git_index_unchanged':True,'no_cargo_nn_source_install_service_mutations':True,'model_quality_or_new_model_promotion_claimed':False}
(root/'receipts/models-v3-result.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
PY
