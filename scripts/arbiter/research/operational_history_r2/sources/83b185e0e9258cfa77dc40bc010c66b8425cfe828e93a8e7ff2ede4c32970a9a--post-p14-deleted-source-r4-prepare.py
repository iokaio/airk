# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import shutil
root=Path(r'D:/code/Github/airk');old=root/'artifacts/v190/post-p14-deleted-source-r3-proposed';out=root/'artifacts/v190/post-p14-deleted-source-r4-proposed';shutil.copytree(old,out)
p=out/'source/crates/airk-store/src/quota/funding.rs';s=p.read_text(encoding='utf-8')
start=s.index('pub(super) fn pinned_retirement_records(');end=s.index('fn pinned_records_checked(',start);s=s[:start]+s[end:]
s=s.replace('retired_collection: Option<&str>,','retired_registration: Option<&ProjectRegistration>,',1)
old='''        ensure!(
            retired_collection.is_none_or(|collection| !record
                .participants
                .iter()
                .any(|part| part.collection == collection)),
            "funded participant requires exact split settlement and acknowledgment before generic retirement"
        );'''
if old not in s:
 start=s.index('        ensure!(\n            retired_collection.');end=s.index('        result.extend(accounting(&record)?);',start);s=s[:start]+'''        if let Some(registration) = retired_registration {
            retired::participant(&record, registration)?;
        }
'''+s[end:]
else:s=s.replace(old,'''        if let Some(registration) = retired_registration {
            retired::participant(&record, registration)?;
        }''',1)
s+='''
#[path = "funding_retired.rs"]
mod retired;
pub(super) use retired::RetiredCurrentInventory;
''';p.write_text(s,encoding='utf-8',newline='\n')
p=out/'source/crates/airk-store/src/quota/funding_retired.rs'
p.write_text('''// SPDX-License-Identifier: Apache-2.0
//! Disposal of fully acknowledged allocations already handed to ordinary Current.
//! Active/unacknowledged operation funding never gains retirement authority here.
use super::*;
use crate::service::registry::catalog::setup::storage::{SetupDirectory, SetupFileMode};
use std::ffi::{OsStr, OsString};

pub(super) fn participant(record: &FundingReceipt, registration: &ProjectRegistration) -> Result<Option<IndexId>> {
    let Some(part) = record.participants.iter().find(|part| part.collection == registration.collection) else {
        return Ok(None);
    };
    ensure!(record.request.registration(part.index_id) == Some(registration)
        && record.complete && record.release_acknowledged && part.remaining_growth == 0,
        "retirement requires exact fully acknowledged ordinary Current ownership");
    ensure!(record.children.iter().filter(|child| child.index_id == part.index_id)
        .all(|child| child.current_owned && !child.closed && matches!(child.kind, FundingChildKind::Current { .. })),
        "retirement cannot adopt operation-owned or ordinary reservation children");
    Ok(Some(part.index_id))
}
struct Row {
    name: OsString,
    owned: File,
    scratch: OsString,
    next: Option<Vec<u8>>,
}
/// Caller has exact native retired participant, positive whole-logical absence,
/// actual global writer exclusion and the existing lifecycle/ledger handles.
pub(in crate::quota) struct RetiredCurrentInventory {
    directory: SetupDirectory,
    root: PathBuf,
    rows: Vec<Row>,
}
impl RetiredCurrentInventory {
    pub(in crate::quota) fn capture(
        control: &SetupDirectory, root: &SetupDirectory, logical_root: &Path,
        registration: &ProjectRegistration,
    ) -> Result<Option<Self>> {
        // Validate ALL known funding metadata before any ordinary/funded effect.
        // Zero-credit live or unacknowledged participants still refuse.
        pinned_records_checked(control, root, logical_root, Some(registration))?;
        let directory = match control.open_dir(OsStr::new(DIRECTORY)) {
            Ok(directory) => directory,
            Err(error) if absent(&error) => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut rows = Vec::new();
        for (visited, entry) in directory.entries()?.enumerate() {
            ensure!(visited < MAX_OPERATIONS * 2 + 1, "excessive retired Current funding inventory");
            let name = entry?;
            let text = name.to_str().context("funding name UTF-8")?;
            if text == "leases" || text.starts_with("funding-") { continue; }
            let owned = directory.open_file(&name, SetupFileMode::Read)?;
            let mut record: FundingReceipt = serde_json::from_slice(&directory.read_file(&name, MAX_RECORD_BYTES)?)?;
            record.validate()?;
            ensure!(text == record.filename() && record.data_root == logical_root
                && record.root_identity == pinned_identity(root, logical_root, record.request.namespace)?,
                "retired Current funding identity changed");
            let Some(index) = participant(&record, registration)? else { continue; };
            if !record.children.iter().any(|child| child.index_id == index) { continue; }
            let scratch = OsString::from(format!("funding-retire-{}-{}-{}",
                record.request.operation_id, record.revision, registration.binding));
            record.children.retain(|child| child.index_id != index);
            advance(&mut record)?;
            record.validate()?;
            let next = if record.children.is_empty() { None } else { Some(serde_json::to_vec(&record)?) };
            if let Some(bytes) = &next {
                ensure!(bytes.len() as u64 <= MAX_RECORD_BYTES, "retired Current metadata exceeds bound");
                if let Ok(prior) = directory.read_file(&scratch, MAX_RECORD_BYTES) {
                    ensure!(bytes.starts_with(&prior), "retired Current scratch belongs to another write");
                } else {
                    match directory.open_file(&scratch, SetupFileMode::Read) {
                        Err(error) if absent(&error) => (),
                        Err(error) => return Err(error),
                        Ok(_) => anyhow::bail!("retired Current scratch cannot be validated"),
                    }
                }
            }
            rows.push(Row { name, owned, scratch, next });
        }
        Ok(Some(Self { directory, root: logical_root.to_path_buf(), rows }))
    }
    pub(in crate::quota) fn apply(&self) -> Result<()> {
        for row in &self.rows {
            if let Some(bytes) = &row.next {
                // Same bounded ledger metadata capacity rule as ordinary
                // release_current; no source, model or new operation allocation.
                crate::capacity::require_growth(&self.root, MAX_RECORD_BYTES)?;
                let mut pending = match self.directory.open_file(&row.scratch, SetupFileMode::ReadWrite) {
                    Ok(file) => file,
                    Err(error) if absent(&error) => self.directory.open_file(&row.scratch, SetupFileMode::CreateNew)?,
                    Err(error) => return Err(error),
                };
                pending.set_len(0)?;
                pending.write_all(bytes)?;
                pending.sync_all()?;
                self.directory.replace_file(&row.scratch, &pending, &row.name, &row.owned)?;
            } else {
                // Native catalog retains operation/release history, exactly as
                // ordinary release_current forgetting its final acknowledged child.
                self.directory.unlink_file_if_identity(&row.name, &row.owned)?;
            }
        }
        self.directory.sync()
    }
}
fn absent(error: &anyhow::Error) -> bool {
    error.downcast_ref::<io::Error>().is_some_and(|error| error.kind() == io::ErrorKind::NotFound)
}
''',encoding='utf-8',newline='\n')
p=out/'source/crates/airk-store/src/quota/retired.rs';s=p.read_text(encoding='utf-8');start=s.index('        // Funding has separate');end=s.index('        let reservations =',start)
s=s[:start]+'''        // Operation funding refuses; fully acknowledged Current-owned children
        // are ordinary growth credit and have a separate exact disposal plan.
        let terminal_current = funding::RetiredCurrentInventory::capture(
            &proof.control, &proof.root, &proof.root_path, registration,
        )?;
'''+s[end:]
needle='''        if let Some((directory, rows)) = &grants {''';s=s.replace(needle,'''        if let Some(plan) = &terminal_current { plan.apply()?; }
'''+needle,1);p.write_text(s,encoding='utf-8',newline='\n')
p=out/'source/crates/airk-cli/tests/combined_reads/deleted_source_recovery.rs';s=p.read_text(encoding='utf-8');needle='''    let b_rows = inventory(&f.registrations[1].collection)?;''';assert needle in s;s=s.replace(needle,needle+'''\n    ensure!(!b_rows.is_empty(), "healthy native Current allowance fixture is vacuous");''',1);p.write_text(s,encoding='utf-8',newline='\n')
print('r4 terminal-owned production copies prepared')
