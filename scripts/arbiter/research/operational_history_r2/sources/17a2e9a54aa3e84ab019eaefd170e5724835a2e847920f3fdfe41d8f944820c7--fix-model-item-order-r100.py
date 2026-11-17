# SPDX-License-Identifier: Apache-2.0
import hashlib,json
from pathlib import Path
r=Path.cwd();p=r/'crates/airk-retrieval/src/model.rs';old=p.read_bytes()
marker=b'/// Metadata-only active-catalog fence.';needle=b'#[cfg(test)]\nmod tests {'
assert old.count(marker)==1 and old.count(needle)==1
at=old.index(marker);block=old[at:];assert block.rstrip().endswith(b'}')
prefix=old[:at];insert=prefix.index(needle);new=prefix[:insert]+block+b'\n'+prefix[insert:]
assert new.count(block)==1
out=r/'artifacts/advanced-split/receipts/model-item-order-r100';out.mkdir(parents=True,exist_ok=False)
(out/'before.rs').write_bytes(old);p.write_bytes(new)
(out/'move.json').write_text(json.dumps({'before':hashlib.sha256(old).hexdigest(),'after':hashlib.sha256(new).hexdigest(),'exact_moved_block_sha256':hashlib.sha256(block).hexdigest(),'behavior_changed':False,'reason':'Place unchanged production metadata helpers before test module for Clippy items_after_test_module.'},indent=2)+'\n')
