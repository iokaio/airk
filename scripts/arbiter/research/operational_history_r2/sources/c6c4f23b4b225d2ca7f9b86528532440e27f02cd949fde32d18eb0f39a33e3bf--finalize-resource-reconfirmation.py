# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib,hashlib,json
base=Path(__file__).resolve().parent
tree=base/'p10-worktree'
cases=[('crates/airk-cli/tests/combined_reads/split_metrics.rs','p10-resource-reconfirmation.proposed.rs'),
       ('crates/airk-cli/tests/combined_reads.rs','p10-resource-fixture-reconfirmation.proposed.rs')]
patch=''
identities={}
for relative,proposal in cases:
    live=tree/relative
    identities[relative]=hashlib.sha256(live.read_bytes()).hexdigest()
    before=live.read_text(encoding='utf-8')
    after=(base/proposal).read_text(encoding='utf-8')
    patch+=''.join(difflib.unified_diff(before.splitlines(True),after.splitlines(True),fromfile='a/'+relative,tofile='b/'+relative))
(base/'p10-resource-reconfirmation.proposed.patch').write_text(patch,encoding='utf-8',newline='\n')
handoff={'status':'inert test-observer proposal, not applied/typechecked/native run',
 'patch':'artifacts/v190/p10-resource-reconfirmation.proposed.patch',
 'base_sha256':identities,'patch_sha256':hashlib.sha256(patch.encode()).hexdigest(),
 'behavior':'Every completed query reply is retained full <=64KiB plus framed SHA256, bytes and measured timing before validation. Exact nonretryable scope expiry/unavailability becomes failed sample, never replayed. Before a later planned wave at changed observed revision, compare complete native Grant/Project/Index/Membership items, verify all source hashes, canonical read-only resolve and reopen four sessions with same selection except expected catalog revision. Refused reconfirmation retains old sessions and error; later failed query remains observable. Settled waves force read-only confirmation. Complete same-child Running successful overlap remains required; cancel stays metadata-only N/A.',
 'identity':'No new grant, mutation, context preference or expanded scope. Original full independent persisted grant baseline includes revocation and provenance; native resolve authenticates host/context/caller. Every successful query compares exact original publication/generation/collection/citation with baseline, and exact content hash.',
 'limits':'Reconfirmation cost separately timed; setup/session reopening included in overall case elapsed. Completed queries are never resubmitted; only next predetermined wave runs. Reconfirmation can itself be refused during native fence; no false successes.',
 'checks':'Ignored proposals rustfmt parse only; primary must typecheck/review and run actual measurements. No tracked edits or Cargo.',
 'plan':'artifacts/v190/p10-resource-campaign-plan-r5.json'}
(base/'p10-resource-reconfirmation-handoff.json').write_text(json.dumps(handoff,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'patch_sha256':handoff['patch_sha256'],'bytes':len(patch.encode()),'base':identities}))
