# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib
root=Path('D:/code/Github/airk/artifacts/v190/p10-worktree')
out=Path('D:/code/Github/airk/artifacts/v190')
patch=''
path='crates/airk-cli/tests/combined_reads.rs'
old=(root/path).read_text(encoding='utf-8')
new=old.replace('mod permanent_trim;\n','mod permanent_trim;\n\n#[path = "combined_reads/split.rs"]\nmod split;\n',1)
assert new!=old
patch+=''.join(difflib.unified_diff(old.splitlines(keepends=True),new.splitlines(keepends=True),fromfile='a/'+path,tofile='b/'+path))
(out/'p10-process-combined_reads.proposed.rs').write_text(new,encoding='utf-8',newline='\n')
path='crates/airk-cli/tests/combined_reads/permanent_trim.rs'
old=(root/path).read_text(encoding='utf-8');new=old
for name in ['policy_key','policy_revision','policy_rpc_cli']:
    if 'async fn '+name+'(' in new:
        new=new.replace('async fn '+name+'(', 'pub(super) async fn '+name+'(',1)
    else:
        new=new.replace('fn '+name+'(', 'pub(super) fn '+name+'(',1)
patch+=''.join(difflib.unified_diff(old.splitlines(keepends=True),new.splitlines(keepends=True),fromfile='a/'+path,tofile='b/'+path))
(out/'p10-process-permanent_trim.proposed.rs').write_text(new,encoding='utf-8',newline='\n')
path='crates/airk-cli/tests/combined_reads/split.rs'
source=(out/'p10-process-split.proposed.rs').read_text(encoding='utf-8')
patch+=''.join(difflib.unified_diff([],source.splitlines(keepends=True),fromfile='/dev/null',tofile='b/'+path))
(out/'p10-process-split.proposed.patch').write_text(patch,encoding='utf-8',newline='\n')
print('Created inert process/RPC integration proposal; no compiler inputs changed.')
