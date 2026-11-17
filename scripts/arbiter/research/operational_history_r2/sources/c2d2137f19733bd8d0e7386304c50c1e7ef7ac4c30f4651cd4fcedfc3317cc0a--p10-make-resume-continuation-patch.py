# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

root = Path('D:/code/Github/airk/artifacts/v190/p10-worktree')
out = Path('D:/code/Github/airk/artifacts/v190')
patches = []
path = 'crates/airk-store/src/service_catalog_split.rs'
old = (root / path).read_text(encoding='utf-8')
marker = '    pub fn catalog_resume_split_continuation(\n'
assert old.count(marker) == 1
helper = '''    /// Read-only routing observation, never retry authority. Both native worker
    /// recovery owners recheck it before accepting an exact metadata continuation.
    pub fn catalog_split_resume_requires_continuation(
        &self,
        request: &SplitRecoveryRequest,
    ) -> Result<bool> {
        let tx = self.connection.unchecked_transaction()?;
        let old = exact(&tx, request.operation_id, request.expected_receipt_digest)?;
        request.validate_for(&old.request, &old.receipt)?;
        ensure!(request.action == SplitRecoveryAction::Resume, "split Resume action required");
        let purpose = match old.receipt.stage {
            SplitStage::OriginalFenced => SplitJobPurpose::OriginalRemoval,
            SplitStage::DestinationValidated => SplitJobPurpose::OriginalWitness,
            _ => return Ok(false),
        };
        let Some(reference) = old.receipt.jobs.iter().rev().find(|job| job.purpose == purpose)
        else { return Ok(false); };
        if super::super::jobs::record(&tx, reference.job)?.state != JobState::Succeeded {
            return Ok(false);
        }
        let (admitted, admitted_reference) = admitted_job(&tx, reference.job)?
            .context("split successful child authority not retained; outcome remains unknown")?;
        ensure!(admitted.receipt_digest == old.receipt_digest && admitted_reference == *reference,
            "split successful child lineage differs");
        let raw = super::super::configuration::result(&tx, reference.job)?
            .context("split successful child result proof is not retained; outcome remains unknown")?;
        if purpose == SplitJobPurpose::OriginalRemoval {
            super::policy::validate_result(&tx, reference.job, &raw)?;
            let result: airk_core::catalog_policy::SourcePolicyApplyReport = serde_json::from_value(raw)?;
            ensure!(result.policy_digest == old.plan.remainder.identity()?
                && result.removal_publication.is_some() && !result.no_index,
                "split successful original removal result differs");
            // Physical reclamation is a later OriginalCleanup purpose. Never
            // rerun a successful restrictive publication to repair bookkeeping.
            return Ok(true);
        }
        let result: SplitJobResult = serde_json::from_value(raw)?;
        result.validate_for(&old.request, &old.plan)?;
        let witness = result.remainder_witness.as_ref()
            .context("successful split witness result lacks its proof")?;
        ensure!(witness.job == Some(reference.job), "split successful witness job differs");
        if old.receipt.remainder_witness.as_ref().is_some_and(|saved| saved.job == Some(reference.job)) {
            ensure!(old.receipt.remainder_witness == result.remainder_witness,
                "split retained witness differs from successful result");
            // The exact witness was already consumed. Explicit Resume may
            // refresh it when cutover detects a changed original publication.
            Ok(false)
        } else {
            Ok(true)
        }
    }

'''
new = old.replace(marker, helper + marker)
for method in ['catalog_resume_split_job_action', 'catalog_resume_split_policy_action']:
    start = new.index('    pub fn ' + method + '(')
    end = new.index('        let tx = self\n', start)
    new = new[:end] + '''        if self.catalog_split_resume_requires_continuation(request)? {
            return self.catalog_resume_split_continuation(request);
        }
''' + new[end:]
patches.append((path, old, new))

path = 'crates/airk-cli/src/catalog_split_control.rs'
old = (root / path).read_text(encoding='utf-8')
marker = '    if snapshot.receipt.stage == SplitStage::OriginalFenced {\n'
assert old.count(marker) == 1
new = old.replace(marker, '''    // Successful native work may outlive a lost metadata acknowledgment. Route
    // that exact saved result before any new worker/settings preparation.
    if registry.catalog_split_resume_requires_continuation(request)? {
        return registry.catalog_resume_split_continuation(request);
    }
''' + marker)
patches.append((path, old, new))

path = 'crates/airk-cli/src/project_supervisor_split_tests.rs'
old = (root / path).read_text(encoding='utf-8')
new = old.replace('    let mut complete = None;\n', '    let mut complete = None;\n    let mut resumed_metadata = 0;\n    let mut refreshed_witness = false;\n', 1)
marker = '            drop(project_lease);\n'
assert new.count(marker) == 1
addition = '''            if child.purpose == SplitJobPurpose::OriginalRemoval
                || child.purpose == SplitJobPurpose::OriginalWitness && !refreshed_witness {
                let observed = fixture.registry.catalog_split_status(operation)?.context("split absent")?;
                let attention = fixture.registry.catalog_note_split_failure(operation, observed.receipt_digest,
                    "native_fixture_metadata_ack_interrupted")?;
                let request = airk_core::catalog_split::SplitRecoveryRequest {
                    caller: attention.request.caller,
                    key: airk_core::service::RequestKey {
                        nonce: Incarnation::from_bytes([if child.purpose == SplitJobPurpose::OriginalWitness { 121 } else { 122 }; 16]),
                        ..attention.request.key.clone()
                    },
                    operation_id: operation, request_digest: attention.receipt.request_digest,
                    expected_receipt_digest: attention.receipt_digest,
                    expected_finalization_revision: None,
                    action: airk_core::catalog_split::SplitRecoveryAction::Resume,
                };
                let sql = rusqlite::Connection::open(fixture.control.join("service-1.4.sqlite3"))?;
                let count: i64 = sql.query_row("SELECT count(*) FROM service_jobs", [], |row| row.get(0))?;
                // Omit retained proof temporarily to exercise unknown/pruned
                // result refusal. Restore only the exact genuine native bytes.
                let bytes: Vec<u8> = sql.query_row("SELECT result FROM service_configuration WHERE job=?1",
                    [child.job.to_string()], |row| row.get(0))?;
                sql.execute("UPDATE service_configuration SET result=zeroblob(16392) WHERE job=?1", [child.job.to_string()])?;
                let method = ControlMethod::CatalogSplitRecover { request: Box::new(request.clone()) };
                ensure!(crate::catalog_split_control::execute(&mut fixture.registry, &fixture.control, &method).is_err(),
                    "missing successful result became retry authority");
                ensure!(fixture.registry.catalog_split_recovery_receipt(request.caller, &request.key)?.is_none()
                    && fixture.registry.catalog_split_status(operation)? == Some(attention.clone()),
                    "unknown result mutated acknowledgment or current split");
                sql.execute("UPDATE service_configuration SET result=?1 WHERE job=?2", rusqlite::params![bytes, child.job.to_string()])?;
                let Some(ControlResponse::CatalogSplitRecovered { receipt, snapshot }) =
                    crate::catalog_split_control::execute(&mut fixture.registry, &fixture.control, &method)?
                else { anyhow::bail!("metadata Resume response absent"); };
                ensure!(matches!(receipt.outcome, airk_core::catalog_split::SplitRecoveryOutcome::ContinuationAccepted { .. })
                    && snapshot.receipt.jobs == attention.receipt.jobs
                    && snapshot.receipt.stage == attention.receipt.stage
                    && snapshot.receipt.state == SplitRunState::Pending,
                    "successful child Resume reran work instead of exact metadata continuation");
                // The canonical owning methods must replay this historical ack
                // before observing the now-newer current receipt or config.
                let replay = if child.purpose == SplitJobPurpose::OriginalRemoval {
                    fixture.registry.catalog_resume_split_policy_action(&request)?
                } else {
                    fixture.registry.catalog_resume_split_job_action(&request, work.engine, &work.request)?
                };
                ensure!(replay == *receipt, "exact native Resume key changed its accepted acknowledgment");
                ensure!(sql.query_row("SELECT count(*) FROM service_jobs", [], |row| row.get::<_, i64>(0))? == count,
                    "metadata-only Resume grew the native work queue");
                resumed_metadata += 1;
            }
'''
new = new.replace(marker, marker + addition)
marker = '''                    crate::catalog_coordinator::refresh_catalog_clock(&gate, &fixture.registry)?;
'''
assert new.count(marker) == 1
new = new.replace(marker, '''                    if !refreshed_witness {
                        // The successful proof is now retained. Preserve the
                        // distinct explicit fresh-witness branch used after a
                        // stale cutover observation; this test requests fresh
                        // requalification without claiming the source changed.
                        let attention = fixture.registry.catalog_note_split_failure(operation,
                            snapshot.receipt_digest, "native_fixture_retained_witness_refresh")?;
                        let request = airk_core::catalog_split::SplitRecoveryRequest {
                            caller: attention.request.caller,
                            key: airk_core::service::RequestKey { nonce: Incarnation::from_bytes([123; 16]), ..attention.request.key.clone() },
                            operation_id: operation, request_digest: attention.receipt.request_digest,
                            expected_receipt_digest: attention.receipt_digest,
                            expected_finalization_revision: None,
                            action: airk_core::catalog_split::SplitRecoveryAction::Resume,
                        };
                        let Some(ControlResponse::CatalogSplitRecovered { receipt, snapshot: next }) =
                            crate::catalog_split_control::execute(&mut fixture.registry, &fixture.control,
                                &ControlMethod::CatalogSplitRecover { request: Box::new(request) })?
                        else { anyhow::bail!("stale witness recovery absent"); };
                        ensure!(matches!(receipt.outcome, airk_core::catalog_split::SplitRecoveryOutcome::ResumeAccepted { .. })
                            && next.receipt.jobs.len() == attention.receipt.jobs.len() + 1
                            && next.receipt.jobs.last().is_some_and(|job| job.purpose == SplitJobPurpose::OriginalWitness),
                            "explicit stale retained witness Resume did not admit one fresh witness job");
                        refreshed_witness = true;
                        continue;
                    }
''' + marker)
marker = '    let complete = complete.context("bounded native split did not complete")?;\n'
assert new.count(marker) == 1
new = new.replace(marker, marker + '    ensure!(resumed_metadata == 2 && refreshed_witness, "native happy path omitted metadata recovery or stale witness refresh");\n')
patches.append((path, old, new))
patch = ''.join(''.join(difflib.unified_diff(old.splitlines(keepends=True), new.splitlines(keepends=True), fromfile='a/' + path, tofile='b/' + path)) for path,old,new in patches)
(out / 'p10-resume-continuation.proposed.patch').write_text(patch, encoding='utf-8', newline='\n')
for path,old,new in patches:
    (out / ('p10-resume-' + Path(path).name)).write_text(new, encoding='utf-8', newline='\n')
print('Created inert Resume continuation proposal; compiler inputs unchanged.')
