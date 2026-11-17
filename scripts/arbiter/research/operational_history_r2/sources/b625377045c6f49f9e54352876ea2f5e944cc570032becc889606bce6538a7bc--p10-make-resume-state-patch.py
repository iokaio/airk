# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

root = Path('D:/code/Github/airk/artifacts/v190/p10-worktree')
path = 'crates/airk-store/src/service_catalog_split.rs'
old = (root / path).read_text(encoding='utf-8')
start = old.index('fn enqueue_split_job_in(')
end = old.index('\nfn cancel_split_in(', start)
section = old[start:end]
marker = '''    require_unused_action_key(db, caller, key, None)?;
'''
assert section.count(marker) == 1
section = section.replace(marker, marker + '''    ensure!(
        purpose == SplitJobPurpose::DestinationCleanup
            || !matches!(old.receipt.state,
                SplitRunState::CanceledBeforeFence { .. } | SplitRunState::Complete),
        "only destination cleanup may continue a canceled split",
    );
''')
marker = '''    if purpose == SplitJobPurpose::DestinationBuild {
        next.stage = SplitStage::Building;
        next.state = SplitRunState::Pending;
    }
'''
assert section.count(marker) == 1
section = section.replace(marker, '''    if purpose == SplitJobPurpose::DestinationBuild {
        next.stage = SplitStage::Building;
    }
    // Exact accepted work clears the attention latch for every runnable purpose,
    // including a fresh retained-witness job and an original cleanup retry.
    // Cancellation remains restrictive throughout destination-only cleanup.
    if purpose != SplitJobPurpose::DestinationCleanup {
        next.state = SplitRunState::Pending;
    }
''')
new = old[:start] + section + old[end:]
out = Path('D:/code/Github/airk/artifacts/v190')
(out / 'p10-resume-state.proposed.patch').write_text(''.join(difflib.unified_diff(old.splitlines(keepends=True), new.splitlines(keepends=True), fromfile='a/' + path, tofile='b/' + path)), encoding='utf-8', newline='\n')
(out / 'p10-resume-state.proposed.rs').write_text(new, encoding='utf-8', newline='\n')
print('Created inert enqueue-state correction; compiler inputs unchanged.')
