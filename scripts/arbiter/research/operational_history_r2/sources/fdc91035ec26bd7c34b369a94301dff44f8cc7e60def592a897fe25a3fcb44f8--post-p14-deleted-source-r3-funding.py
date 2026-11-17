# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
root=Path(r'D:/code/Github/airk');out=root/'artifacts/v190/post-p14-deleted-source-r3-proposed'
rel='crates/airk-store/src/quota/funding.rs';raw=(root/rel).read_bytes();p=out/'base'/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(raw);s=raw.decode('utf-8')
needle='''pub(super) fn pinned_records(
    control: &crate::service::registry::catalog::setup::storage::SetupDirectory,
    root: &crate::service::registry::catalog::setup::storage::SetupDirectory,
    logical_root: &Path,
) -> Result<Vec<LiveRecord>> {
''';assert needle in s
new=needle+'''    pinned_records_checked(control, root, logical_root, None)
}
pub(super) fn pinned_retirement_records(
    control: &crate::service::registry::catalog::setup::storage::SetupDirectory,
    root: &crate::service::registry::catalog::setup::storage::SetupDirectory,
    logical_root: &Path,
    collection: &str,
) -> Result<Vec<LiveRecord>> {
    pinned_records_checked(control, root, logical_root, Some(collection))
}
fn pinned_records_checked(
    control: &crate::service::registry::catalog::setup::storage::SetupDirectory,
    root: &crate::service::registry::catalog::setup::storage::SetupDirectory,
    logical_root: &Path,
    retired_collection: Option<&str>,
) -> Result<Vec<LiveRecord>> {
''';s=s.replace(needle,new,1)
start=s.index('fn pinned_records_checked(');pos=s.index('        result.extend(accounting(&record)?);',start)
s=s[:pos]+'''        ensure!(retired_collection.is_none_or(|collection|
            !record.participants.iter().any(|part| part.collection == collection)),
            "funded participant requires exact split settlement and acknowledgment before generic retirement");
'''+s[pos:];p=out/'source'/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(s,encoding='utf-8',newline='\n')
p=out/'source/crates/airk-store/src/quota/retired.rs';s=p.read_text(encoding='utf-8');s=s.replace('|| Ok(())','|_| Ok(())',1).replace('before_unlink: impl FnOnce() -> Result<()>','mut checkpoint: impl FnMut(SettlementPoint) -> Result<()>');s=s.replace('funding::pinned_records(&proof.control, &proof.root, &proof.root_path)?','funding::pinned_retirement_records(&proof.control, &proof.root, &proof.root_path, &proof.collection)?');s=s.replace('        before_unlink()?;','        checkpoint(SettlementPoint::BeforeUnlink)?;');s=s.replace('for (name, row) in rows { directory.unlink_file_if_identity(name, row)?; }','for (name, row) in rows {\n                directory.unlink_file_if_identity(name, row)?;\n                checkpoint(SettlementPoint::AfterGrant)?;\n            }');s=s.replace('''            reservations.unlink_file_if_identity(name, row)?;
            reservations.unlink_file_if_identity(lease_name, held)?;''','''            reservations.unlink_file_if_identity(name, row)?;
            checkpoint(SettlementPoint::AfterReservationRecord)?;
            reservations.unlink_file_if_identity(lease_name, held)?;''');s+='''
#[derive(Clone, Copy, PartialEq, Eq)]
enum SettlementPoint { BeforeUnlink, AfterGrant, AfterReservationRecord }
#[cfg(test)]
#[path = "retired_tests.rs"]
mod tests;
''';p.write_text(s,encoding='utf-8',newline='\n')
print('funding scope guard + fault seam prepared')
