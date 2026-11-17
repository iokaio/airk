# SPDX-License-Identifier: Apache-2.0
import hashlib,json
from pathlib import Path
p=Path('crates/airk-service/src/service_runtime/arbiter/lifecycle.rs')
raw=p.read_bytes();text=raw.decode('utf-8')
marker='\n#[cfg(all(test, feature = "test-support"))]\npub(super) fn observe_warm('
start=text.index(marker)
test=text.index('\n#[cfg(test)]\nmod tests {')
assert start>test and text.count(marker)==1 and text.count('\nmod tests {')==1
helper=text[start:]
assert helper.rstrip().endswith('}')
updated=text[:test]+helper+'\n'+text[test:start]
assert updated.count(helper)==1 and updated.count('\nmod tests {')==1
output=Path('artifacts/advanced-split/receipts/native-recovery-helper-move-r120');output.mkdir(exist_ok=False)
(output/'before.rs').write_bytes(raw)
p.write_text(updated,encoding='utf-8',newline='')
(output/'move.json').write_text(json.dumps({'source':str(p),'before_sha256':hashlib.sha256(raw).hexdigest(),'after_sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'helper_bytes_sha256':hashlib.sha256(helper.encode()).hexdigest(),'exact_helper_bytes_preserved':True,'reason':'R119 items_after_test_module; move unchanged private warm helpers before existing test module, no warning suppression or assertion change.'},indent=2)+'\n',encoding='utf-8')
