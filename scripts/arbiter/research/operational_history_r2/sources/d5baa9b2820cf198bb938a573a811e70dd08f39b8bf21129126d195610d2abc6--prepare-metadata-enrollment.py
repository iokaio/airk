# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib
p=Path('artifacts/optimizations/f10-consolidated')
f=p/'BASE.json';record=json.loads(f.read_text(encoding='utf-8'));patch=Path('artifacts/optimizations/f10-f11-draft/restart-metadata-after-foundation.patch');record['settled_metadata_input']={'path':patch.as_posix(),'sha256':hashlib.sha256(patch.read_bytes()).hexdigest()};f.write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
f=p/'source/crates/airk-store/src/service_collection_integrity.rs';s=f.read_text(encoding='utf-8');needle='    /// Durable sticky detection, never a claim that other components are clean.'
method='''    /// Bounded initial native metadata enrollment under the actual logical writer.
    /// Opens its own fresh read; accepts no retained pre-mutation snapshot. This
    /// owning operation may use existing collection lock creation. It creates no
    /// restart/query authority, producer certificate or physical audit coverage.
    pub fn enroll_native_metadata(&self, writer: &crate::WriterLock) -> Result<Option<IntegrityObservation>> {
        self.check()?;
        let selected=self.collection.select()?;
        crate::current::gate::check_writer(&selected.store,writer)?;
        let Some(scope)=selected.store.current_owned_scope()? else { return Ok(None); };
        let read=selected.store.current_read(&scope)?;
        if read.settled_native_head()?.is_none() { return Ok(None); }
        Ok(Some(self.anchor(&read)?))
    }
'''
assert needle in s;s=s.replace(needle,method+needle,1);f.write_text(s,encoding='utf-8',newline='\n')
f=p/'source/crates/airk-store/src/service_collection_tests.rs';s=f.read_text(encoding='utf-8');needle='    drop(writer);\n    // Actual interrupted Applying WAL, using the existing oversized-sidecar'
assert needle in s;s=s.replace(needle,'''    assert!(owner.enroll_native_metadata(&writer)?.is_none(),
        "fresh owning metadata enrollment cannot certify Applying as settled");
    assert_eq!(fs::read(&receipt_path)?,receipt);
'''+needle,1)
s+='''
#[test]
fn native_metadata_enrollment_requires_exact_writer_and_never_promotes_unknown_audit() -> Result<()> {
    let (_temporary,collection,registry,bound,lease,store,_scope)=integrity_fixture()?;
    let owner=collection.integrity_owner(&registry,&lease,&bound)?;
    let writer=store.lock()?;
    let state=owner.enroll_native_metadata(&writer)?.context("settled native head missing")?;
    assert_eq!(state.physical_audit,super::integrity::PhysicalAudit::Unknown);
    assert_eq!(state.privacy,super::integrity::PrivacyEligibility::PlatformEvidenceUnavailable);
    assert_eq!(state.basis,super::integrity::PublicationBasis::RetainedHeadMetadataAnchor);
    let before=fs::read(collection.directory.join("integrity-observation.json"))?;
    let foreign=FileStore::new(_temporary.path().join("foreign-writer"));
    let foreign_writer=foreign.lock()?;
    assert!(owner.enroll_native_metadata(&foreign_writer).is_err());
    assert_eq!(fs::read(collection.directory.join("integrity-observation.json"))?,before);
    Ok(())
}
''';f.write_text(s,encoding='utf-8',newline='\n')
