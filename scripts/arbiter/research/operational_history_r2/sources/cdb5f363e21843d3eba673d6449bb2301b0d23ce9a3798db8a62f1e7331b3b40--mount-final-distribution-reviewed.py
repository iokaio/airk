# SPDX-License-Identifier: Apache-2.0
import argparse,hashlib,json,shutil
from pathlib import Path
r=Path.cwd().resolve();p=argparse.ArgumentParser();p.add_argument('--manifest',type=Path,required=True);p.add_argument('--sha256',required=True);p.add_argument('--receipt',type=Path,required=True);a=p.parse_args()
sha=lambda raw:hashlib.sha256(raw).hexdigest()
assert sha(a.manifest.read_bytes())==a.sha256
m=json.loads(a.manifest.read_bytes());assert m['schema']=='airk.private-final-distribution-coherent-mount/1'
allowed={
'crates/airk-service/src/service_runtime/arbiter.rs',
'crates/airk-retrieval/src/arbiter/publication.rs',
'crates/airk-service/src/service_runtime/evaluation_fixture_tests.rs',
'crates/airk-service/src/service_runtime/arbiter/final_distribution.rs',
'crates/airk-service/src/service_runtime/arbiter/final_distribution/build_target.rs',
'crates/airk-service/src/service_runtime/arbiter/final_distribution/calibration_equivalence.rs',
'crates/airk-service/src/service_runtime/arbiter/final_distribution/composed_validation.rs',
'crates/airk-service/src/service_runtime/arbiter/final_distribution/CONTRACT.md'}
assert {str(Path(x['target'])).replace('\\','/') for x in m['files']}==allowed
rows=[]
for x in m['files']:
 target=(r/x['target']).resolve();source=(r/x['proposed']).resolve()
 assert target.is_relative_to(r/'crates') and source.is_relative_to(r/'artifacts')
 assert not target.is_symlink() and not source.is_symlink()
 raw=source.read_bytes();assert sha(raw)==x['proposed_sha256'] and len(raw)==x['bytes']
 assert (sha(target.read_bytes()) if target.exists() else None)==x['base_sha256']
 rows.append((x,target,raw))
out=a.receipt.resolve();assert out.is_relative_to(r/'artifacts');out.mkdir(parents=True,exist_ok=False)
for x,target,raw in rows:
 if target.exists():
  saved=out/target.relative_to(r);saved.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(target,saved)
for x,target,raw in sorted(rows,key=lambda row:row[1].suffix!='.md'):
 target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(raw)
(out/'mount.json').write_text(json.dumps({'manifest':str(a.manifest),'manifest_sha256':a.sha256,'root_review':'Root self-review plus independent Focus review: canonical policy/publication hooks, exact original evidence/target/rights/final-calibration proof and rational A6 refusal controls; test-only private orchestration, no new product wire/state/schema. Contracts mounted first. Actual model qualification remains absent.','files':m['files']},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'mounted':len(rows),'receipt':str(out/'mount.json')}))
