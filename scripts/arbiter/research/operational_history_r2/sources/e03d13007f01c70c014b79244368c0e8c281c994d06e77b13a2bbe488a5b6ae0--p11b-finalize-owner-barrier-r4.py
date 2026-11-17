# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib,difflib
root=Path('D:/code/Github/airk');base=root/'artifacts/v190/p10-worktree';out=root/'artifacts/v190/p11b-owner-barrier-r4-proposed'
baseline=json.loads((out/'baseline.json').read_text(encoding='utf-8'));chunks=[]
for path,digest in baseline.items():
    raw=(base/path).read_bytes() if (base/path).exists() else None
    assert (hashlib.sha256(raw).hexdigest() if raw is not None else None)==digest,path
    chunks.append('diff --git a/'+path+' b/'+path+'\n')
    if raw is None:chunks.append('new file mode 100644\n')
    chunks.extend(difflib.unified_diff((raw.decode('utf-8') if raw is not None else '').splitlines(True),(out/path).read_text(encoding='utf-8').splitlines(True),fromfile='a/'+path if raw is not None else '/dev/null',tofile='b/'+path))
patch=''.join(chunks);(out/'owner-barrier-r4.patch').write_text(patch,encoding='utf-8',newline='\n')
notes=json.loads((root/'artifacts/v190/p11b-owner-barrier-r3-proposed/notes.json').read_text(encoding='utf-8'))
notes.update(status='INERT r4 private owning argument context +named-slot bracket; all predecessor revisions preserved, no tracked edits/Cargo/native execution/SSH.',patch_sha256=hashlib.sha256(patch.encode()).hexdigest(),files=list(baseline))
notes['r4_changes']=[
'Bundle actual native owning inputs in a private nonserializable borrowed RetentionOwnerContext, avoiding nine-argument helper Clippy failure. No actual ownership/phase/funding checks removed or waived. Public Collection API and peer scope accessor remain unchanged.',
'Prepared lower marker brackets its exact named slot identity before/after anchored publication, while captured directory remains publication operand. No reopened pathname capture or inode-CAS claim.'
]
notes['validation']='All11 ignored Rust copies parsed/formatted in r3; modified r4 context/journal copies parsed/formatted. Live baseline hashes verified, proper new-file Git metadata. No typecheck, Clippy, tests or subprocess/SSH execution. Read-only applicability check next.'
(out/'notes.json').write_text(json.dumps(notes,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'files':len(baseline),'patch_sha256':notes['patch_sha256']}))
