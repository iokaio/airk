# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shutil
from pathlib import Path
r=Path.cwd();sha=lambda b:hashlib.sha256(b).hexdigest()
manifest=r/'artifacts/advanced-split/local-delivery-preparation-r1/script-fixture-repair-combined.json';assert sha(manifest.read_bytes())=='70f79272050b329c038a90acb4620e15bfeb5c29a889475af9b10bb9a6c05fb5'
rows=[]
for target,item in json.loads(manifest.read_bytes())['files'].items():
 source=Path(item['private_candidate']);data=source.read_bytes();assert sha(data)==item['candidate_sha256'] and sha((r/target).read_bytes())==item['original_sha256'];rows.append((r/target,data))
p=r/'artifacts/advanced-split/webpki-roots-source-review-r1';review_manifest=p/'manifest.json';assert sha(review_manifest.read_bytes())=='a00999acd4979f2f1887dc9af1b487fbd2b19f309879b7013ecaccccc793fced'
for name,digest in json.loads(review_manifest.read_bytes())['files'].items():assert sha((p/name).read_bytes())==digest
policy=r/'supply-chain/source-policy.json';old=json.loads(policy.read_bytes());newraw=(p/'source-policy.json.proposed').read_bytes();new=json.loads(newraw)
added=[x for x in new['packages'] if x not in old['packages']];assert added==[{'name':'webpki-roots','version':'1.0.9','source':'registry+https://github.com/rust-lang/crates.io-index','checksum':'7dcd9d09a39985f5344844e66b0c530a33843579125f23e21e9f0f220850f22a'}]
rest=dict(new);rest['packages']=[x for x in new['packages'] if x not in added];assert rest==old;rows.append((policy,newraw))
text=(p/'REVIEW-PROPOSAL.md').read_text(encoding='utf-8').replace('source review proposal','source review').replace('Status: agent source/security review completed; Root acceptance and actual cargo-vet import selection/check pending.','Status: agent source/security review independently accepted by Root for the exact source identity on15November2026; actual cargo-vet import selection/check remains pending.')
public=r/'supply-chain/reviews/webpki-roots-1.0.9-2026-11-15.md';assert not public.exists();rows.append((public,text.encode('utf-8')))
out=r/'artifacts/advanced-split/receipts/script-source-review-mount-r105';out.mkdir(parents=True,exist_ok=False);proof=[]
for target,data in rows:
 before=None
 if target.exists():
  saved=out/target.relative_to(r);saved.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(target,saved);before=sha(saved.read_bytes())
 proof.append({'target':str(target.relative_to(r)),'before':before,'after':sha(data)})
for target,data in rows:target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
(out/'mount.json').write_text(json.dumps({'root_review':'Root reviewed test-only short-path/historical-duty/current-legal fixtures without weaker bounds or package checks; exact dependency source/static TLS-root review accepted with independently verified existing Mozilla publisher/date trust, no new audit or exemption. Actual unfiltered locked cargo-vet is still required.','files':proof},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'mounted':len(rows),'receipt':str(out/'mount.json')}))
