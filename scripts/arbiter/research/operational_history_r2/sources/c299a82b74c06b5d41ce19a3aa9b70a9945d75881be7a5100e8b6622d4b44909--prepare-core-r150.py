# SPDX-License-Identifier: Apache-2.0
import hashlib,json,shutil
from pathlib import Path
r=Path.cwd();w=r/'artifacts/advanced-split/core-contract-gate-r149';p=r/'fixtures/arbiter/profile-downloads.json';q=w/'fixtures/arbiter/profile-downloads.json'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
assert sha(p)=='d0064974be46e7cb56099a71c248587d81cd93eccd1405ec9e736664c199d593'
assert sha(q)=='886191b0610b4e69caab48b3eb39f493a1e4eac1b4c715bc708ade2e63441acc'
assert p.read_bytes().replace(b'\r\n',b'\n')==q.read_bytes().replace(b'\r\n',b'\n')
out=r/'artifacts/advanced-split/receipts/core-checkout-fixture-r150';out.mkdir();shutil.copyfile(q,out/'checkout-before.json');shutil.copyfile(p,q)
(out/'diagnosis.json').write_text(json.dumps({'cause':'Git isolated checkout changed this pinned retained fixture from LF to CRLF. Restore exact authentic retained bytes from current main checkout; unchanged Rust assertions. Main source/fixture never modified.','actual_fixture_sha256':sha(p),'corrected_checkout_sha256':sha(q)},indent=2)+'\n',encoding='utf-8')
s=(r/'artifacts/advanced-split/run-isolated-core-r149.py').read_text(encoding='utf-8').replace("receipts/core-contract-r149","receipts/core-contract-r150")
(r/'artifacts/advanced-split/run-isolated-core-r150.py').write_text(s,encoding='utf-8')
