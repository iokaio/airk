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
packet=json.loads(base64.b64decode('eyJmaWxlcyI6IHsibWluaWxtLWw2LXYyLXU4LXYyL0xJQ0VOU0UiOiB7ImJ5dGVzIjogMzQ4ODksICJzaGEyNTYiOiAiOWNjN2ZiZDY5Zjc4ZmYxOWExODgwMjU5NzZkZjIwZDVjMTdiMGNlYzVlZDc5YWQ1ZDdlMTZkMjA5NGU0NmFiZCJ9LCAibWluaWxtLWw2LXYyLXU4LXYyL21vZGVsLmpzb24iOiB7ImJ5dGVzIjogNzE0LCAic2hhMjU2IjogIjYzNGNjNTk1YWRlNWFlMmJlOTU2OTljNmY0NTFlMDExNDdmZGIzOGQ1ODA5NmQxYjNhMDM5YTM0NzM2Nzg4ZDAifSwgIm1pbmlsbS1sNi12Mi11OC12Mi9tb2RlbC5vbm54IjogeyJieXRlcyI6IDY5MzI1OTI2LCAic2hhMjU2IjogImU3Mzk5ZTgxNzM3ZDNmOTcwN2EzMDRiZTExNmEyMjk5ODNiYzlmZmYwOTc0ZDFiMDg3MjkzNzg5MDIxZDJkN2IifSwgIm1pbmlsbS1sNi12Mi11OC12Mi90b2tlbml6ZXIuanNvbiI6IHsiYnl0ZXMiOiA0NjYyNDcsICJzaGEyNTYiOiAiYmU1MGMzNjI4ZjJiZjViYjVlM2E3ZjE3YjFmNzQ2MTFiMjU2MWEzYTI3ZWVhYjA1ZTVhYTMwZjQxMTU3MjAzNyJ9LCAibWluaWxtLWw2LXYyLXM4LXYyL0xJQ0VOU0UiOiB7ImJ5dGVzIjogMzQ4ODksICJzaGEyNTYiOiAiOWNjN2ZiZDY5Zjc4ZmYxOWExODgwMjU5NzZkZjIwZDVjMTdiMGNlYzVlZDc5YWQ1ZDdlMTZkMjA5NGU0NmFiZCJ9LCAibWluaWxtLWw2LXYyLXM4LXYyL21vZGVsLmpzb24iOiB7ImJ5dGVzIjogNzE0LCAic2hhMjU2IjogImFlOWUwZjVhOTg2YjQzMmVlOTM3Mzg2NjA1ZWUxNjk5MTY5NDIxMjBlYmUzMzM0MmZmYjIxOWM4NzdkMTIwZDQifSwgIm1pbmlsbS1sNi12Mi1zOC12Mi9tb2RlbC5vbm54IjogeyJieXRlcyI6IDY5MzU1NDQ0LCAic2hhMjU2IjogIjQ3ODg1OTdiNmQyNjA2MWI3NzhhYzYyNTU3MjNkNGJiMDA0ZTQ5Mjc0NDZjNDgwZWNhZWJlMTllNGVhN2Y0N2MifSwgIm1pbmlsbS1sNi12Mi1zOC12Mi90b2tlbml6ZXIuanNvbiI6IHsiYnl0ZXMiOiA0NjYyNDcsICJzaGEyNTYiOiAiYmU1MGMzNjI4ZjJiZjViYjVlM2E3ZjE3YjFmNzQ2MTFiMjU2MWEzYTI3ZWVhYjA1ZTVhYTMwZjQxMTU3MjAzNyJ9fSwgImFyY2hpdmVfc2hhMjU2IjogIjIwMWJjMTVlZTJiMDRlZjRkYTUzMGNmOWQ5ODI1NGM1MmM1OTdhMjNlODIzOGFiYWFlN2M3ODA5Yzg4YjYzYjAiLCAiYXJjaGl2ZV9ieXRlcyI6IDEzOTY5NDA4MCwgImNhdGFsb2dfc2hhMjU2IjogIjdmZDIzN2UyMjgyZDU1Njc2NGQ3OWUyNDNkOWVlMDZlMDg4NTZlZjgwZWE1MzJmYjMwMGZjNDhjZGY0NTg3MmUiLCAic291cmNlIjogInJvb3QtYXV0aG9yaXplZCBleGlzdGluZyBidW5kbGVkLW1vZGVsLXYyOyBub3QgcGFyZW50LWJhZyBleHBvcnQifQ==')); expected=packet['files']
started=time.monotonic(); index=Path('/Users/tylerjensen/Code/airk/.git/index')
before=hashlib.sha256(index.read_bytes()).hexdigest()
node_receipt=json.loads((root/'receipts/node-result.json').read_bytes())
assert node_receipt['result']=='verified_prerequisite'
archive=root/'model-transfer/original-minilm-v2-eight-files.tar'
assert archive.is_file() and not archive.is_symlink() and archive.stat().st_size==packet['archive_bytes']
with archive.open('rb') as stream:assert hashlib.file_digest(stream,'sha256').hexdigest()==packet['archive_sha256']
destination=root/'inputs/bundled-model-v2'
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
receipt={'schema':'airk.private-neo-model-prerequisite/1','result':'exact_original_eight_files_verified','elapsed_seconds':time.monotonic()-started,'directory':str(destination),'files':actual,'archive_sha256':packet['archive_sha256'],'source_catalog_sha256':packet['catalog_sha256'],'source':packet['source'],'original_git_index_before':before,'original_git_index_after':after,'original_git_index_unchanged':True,'no_cargo_nn_source_install_service_mutations':True,'model_quality_or_new_model_promotion_claimed':False}
(root/'receipts/models-result.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
PY
