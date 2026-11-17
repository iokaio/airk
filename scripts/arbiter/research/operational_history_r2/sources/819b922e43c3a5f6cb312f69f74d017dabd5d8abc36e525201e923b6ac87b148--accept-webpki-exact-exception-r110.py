# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shutil,tomllib
from pathlib import Path
r=Path.cwd();p=r/'artifacts/advanced-split/webpki-roots-source-review-r1/import-semantics-correction-r2';sha=lambda data:hashlib.sha256(data).hexdigest()
manifest=p/'manifest.json';assert sha(manifest.read_bytes())=='aab7ee696d64060a1ece3af467ea326d75cd25a0bd5488730a4bc972787ef3c4'
m=json.loads(manifest.read_bytes());out=r/'artifacts/advanced-split/receipts/webpki-exception-acceptance-r110';out.mkdir(parents=True,exist_ok=False)
for name,digest in m['files'].items():assert sha((p/name).read_bytes())==digest
config=r/'supply-chain/config.toml';old=config.read_bytes();new=(p/'config.toml.proposed').read_bytes();assert sha(old)==m['bases']['supply-chain\\config.toml']
a=tomllib.loads(old.decode('utf-8'));b=tomllib.loads(new.decode('utf-8'));assert 'webpki-roots' not in a['exemptions'];extra=b['exemptions'].pop('webpki-roots');assert len(extra)==1 and extra[0]['version']=='1.0.9' and extra[0]['criteria']=='safe-to-deploy' and a==b
review=r/'supply-chain/reviews/webpki-roots-1.0.9-2026-11-15.md';assert sha(review.read_bytes())==m['bases'][str(review.relative_to(r)).replace('\\','/')]
shutil.copyfile(config,out/'config.before.toml');shutil.copyfile(review,out/'review.before.md');shutil.copyfile(r/'supply-chain/imports.lock',out/'imports.generated-unselected.lock')
original=r/'artifacts/advanced-split/receipts/cargo-vet-refresh-r108/imports.lock';assert sha(original.read_bytes())=='7f841bb4ee4d19e06027dfe1dc2ab8f8d4104f856c5c727fce2c2db6630caab5'
shutil.copyfile(original,r/'supply-chain/imports.lock');config.write_bytes(new)
text=(p/'review.md.proposed').read_text(encoding='utf-8')
text+='\n\n## Root acceptance,15November2026\n\nRoot independently reviewed the exact archive/source identity, static trust-anchor implementation, sole bounded hash-pinned download consumer and stated limitations. The one exact1.0.9 limited reviewed exception is accepted under the standing supply-chain policy. This is not a human/full security audit or publisher trust. The prior imported-trust coverage assertion is retracted; actual R109 correctly failed. The unnecessary generated import selection and automatic exemption cleanup were not accepted; original imports and all prior exceptions are restored. A fresh unfiltered locked cargo-vet check remains required.\n'
review.write_text(text,encoding='utf-8',newline='\n')
(out/'decision.json').write_text(json.dumps({'decision':'approved_exact_version_limited_reviewed_exception','package':'webpki-roots','version':'1.0.9','checksum':'7dcd9d09a39985f5344844e66b0c530a33843579125f23e21e9f0f220850f22a','review_manifest_sha256':sha(manifest.read_bytes()),'new_audit_or_publisher_trust':False,'actual_locked_check_pending':True,'prior_exemptions_preserved':True,'original_imports_restored':True},indent=2)+'\n')
print('exact exception mounted; original imports restored')
