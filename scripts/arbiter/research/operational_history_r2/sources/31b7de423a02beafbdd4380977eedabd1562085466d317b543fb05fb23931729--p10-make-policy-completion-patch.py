# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

root = Path('D:/code/Github/airk/artifacts/v190/p10-worktree')
out = Path('D:/code/Github/airk/artifacts/v190')
path = 'crates/airk-store/src/service_catalog_split.rs'
old = (root / path).read_text(encoding='utf-8')
marker = '''    pub fn catalog_complete_split_job(
        &mut self,
        operation: Incarnation,
        expected: Sha256Digest,
        job: Sha256Digest,
    ) -> Result<SplitSnapshot> {
'''
assert old.count(marker) == 1
addition = '''        // A worker can die after Succeeded but before its ordinary P9 policy
        // completion tail. Reconcile only this exact accepted result before the
        // split advertises RemainderPublished; never depend on another actor's
        // scan/timer ordering. The canonical P9 owner validates the real result.
        let policy_completion = {
            let tx = self.connection.unchecked_transaction()?;
            let old = exact(&tx, operation, expected)?;
            let (admitted, reference) =
                admitted_job(&tx, job)?.context("split job not admitted")?;
            ensure!(admitted.receipt_digest == expected, "split completion lifecycle changed");
            if reference.purpose == SplitJobPurpose::OriginalRemoval {
                ensure!(super::super::jobs::record(&tx, job)?.state == JobState::Succeeded,
                    "split job is not successfully complete");
                let work = super::super::configuration::work(&tx, job)?
                    .context("split original policy configuration missing")?;
                let ProjectConfiguration::ApplySourcePolicy { policy, operation_id } =
                    work.request.configuration else {
                    anyhow::bail!("split original removal is not accepted policy work");
                };
                let current = super::policy::snapshot(&tx, old.request.original_index)?
                    .context("split original policy missing")?;
                ensure!(current.job == job && current.operation_id == operation_id
                    && current.policy == *policy && current.policy == old.plan.remainder,
                    "split original policy completion lineage differs");
                Some((old.request.original_index, operation_id))
            } else {
                None
            }
        };
        if let Some((index, policy_operation)) = policy_completion {
            self.catalog_record_policy_job(index, policy_operation)?;
        }
        // Recheck the exact receipt, native work lineage, and Succeeded result
        // under the existing write transaction after the P9 owner's commit.
'''
new = old.replace(marker, marker + addition)
patch = ''.join(difflib.unified_diff(old.splitlines(keepends=True), new.splitlines(keepends=True), fromfile='a/' + path, tofile='b/' + path))
test_path = 'crates/airk-cli/src/project_supervisor_split_tests.rs'
test_old = (root / test_path).read_text(encoding='utf-8')
tail = '''            witnessed |= child.purpose == SplitJobPurpose::OriginalWitness;
'''
assert test_old.count(tail) == 1
test_new = test_old.replace(tail, '''            if child.purpose == SplitJobPurpose::OriginalRemoval {
                // Deliberately omit production's post-Succeeded P9 tail. This
                // is the real crash/interleaving boundary the metadata owner
                // must reconcile from the exact retained native job result.
                ensure!(fixture.registry.catalog_policy_status(current.request.original_index)?
                    .context("original policy absent after actual removal")?.application.is_none(),
                    "fixture unexpectedly ran the P9 completion tail");
            }
''' + tail)
loop_marker = '''        if let Some(child) = current.receipt.jobs.last()
'''
assert test_new.count(loop_marker) == 1
test_new = test_new.replace(loop_marker, '''        if current.receipt.stage == SplitStage::RemainderPublished {
            let policy = fixture.registry.catalog_policy_status(current.request.original_index)?
                .context("remainder policy absent")?;
            ensure!(policy.application.as_ref().is_some_and(|report|
                report.removal_publication == current.receipt.remainder_publication),
                "split advanced before canonical P9 completion acknowledgment");
        }
''' + loop_marker)
patch += ''.join(difflib.unified_diff(test_old.splitlines(keepends=True), test_new.splitlines(keepends=True), fromfile='a/' + test_path, tofile='b/' + test_path))
(out / 'p10-policy-completion-race.proposed.patch').write_text(patch, encoding='utf-8', newline='\n')
(out / 'p10-policy-completion-race.proposed.rs').write_text(new, encoding='utf-8', newline='\n')
print('Created inert targeted P9 policy completion patch; compiler inputs unchanged.')
