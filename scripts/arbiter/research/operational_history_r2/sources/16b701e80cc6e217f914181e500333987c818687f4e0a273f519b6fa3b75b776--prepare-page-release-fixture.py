# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f11-consolidated/source/crates/airk-store/src/service_collection_tests.rs')
s=p.read_text(encoding='utf-8');s+='''

fn audit_retained_input(store:&FileStore,writer:&crate::WriterLock,quota:&Quota,scope:&CurrentScope,path:&str,text:&str)->Result<crate::current::CapturedInput>{
    use airk_core::extraction::{ExtractionPayload,ExtractionProfile,LocalChunk,LocalSpan};
    use sha2::{Digest,Sha256};
    let profile=ExtractionProfile{contract_version:1,adapter_id:"text".into(),adapter_version:"audit-fixture/1".into(),language:"text".into(),dialect:None,grammar_identity:None,mapping_version:1,chunk_version:3,options:Default::default(),configuration_inputs:Default::default()};
    let payload=ExtractionPayload{semantic_projection:Default::default(),continuity_coverage:Default::default(),contract_version:1,extraction_method:"utf8".into(),parse_status:"plain".into(),provenance:vec![],text:text.into(),syntax_identities:vec![],facts:vec![],chunks:vec![LocalChunk{span:LocalSpan{start:0,end:text.len()},start_line:1,end_line:1,location:airk_core::DocumentLocation::Lines{start:1,end:1},region:None}],regions:vec![],relationships:vec![],continuity:vec![],diagnostics:vec![],partial:false};
    let artifact=airk_core::binding::bind_payload(&payload,&profile,airk_core::binding::ArtifactBinding{path,source_hash:Sha256Digest::from_bytes(Sha256::digest(text.as_bytes()).into()),source_bytes:text.len()})?;
    let reservation=quota.reserve(&scope.collection,1<<20)?;
    let artifact=store.retain_payload_with_budget_owned(writer,&artifact,&payload,&profile,Some(text.as_bytes()),Some(&reservation))?;
    Ok(crate::current::CapturedInput::new(artifact))
}
#[test]
fn real_retained_pages_release_writer_admit_query_and_reclaim_same_job_with_original_dates() -> Result<()> {
    use crate::service::{ProjectLease,data_scope,registry::{ServiceRegistry,AuditEvidence}};
    use airk_core::service::{JobKind,JobOutcome,RequestKey};
    let (temporary,collection,mut registry,bound,lease,store,scope)=integrity_fixture()?;
    let quota=Quota::open_service(Path::new(&collection.owner.data_root))?;
    {
        let writer=store.lock()?;
        for(path,text) in [("a.txt","first retained body"),("b.txt","second retained body")]{
            let input=audit_retained_input(&store,&writer,&quota,&scope,path,text)?;
            let mut current=store.current_write(&writer,&quota,&scope,8<<20)?;let token=current.head()?.token()?;
            current.commit_artifact(&token,path,Some(&input))?;
        }
    }
    assert_eq!(store.current_read(&scope)?.lookup_paths(crate::current::postings::Lookup::Files,"",None,3)?.paths.len(),2);
    let project=lease.identity().0;let(namespace,epoch)=registry.saved_request_epoch()?;
    let job=registry.enqueue(project,namespace,&RequestKey{namespace,epoch,nonce:Incarnation::from_bytes([97;16])},JobKind::Verify)?;
    registry.claim_job_id(&lease,job.id)?.context("Verify not claimed")?;
    let before={
        let owner=collection.integrity_owner(&registry,&lease,&bound)?;let read=store.current_read(&scope)?;owner.anchor(&read)?;drop(read);
        let mut writer=store.lock()?;
        let(go,queued)=std::sync::mpsc::sync_channel(1);let(done,admitted)=std::sync::mpsc::sync_channel(1);
        let query_store=store.clone();let query_scope=scope.clone();
        let query=std::thread::spawn(move||->Result<()>{queued.recv_timeout(Duration::from_secs(10))?;let read=query_store.current_read(&query_scope)?;
            assert!(read.artifact("b.txt")?.is_some());done.send(read.token()?)?;Ok(())});
        let calls=std::cell::Cell::new(0);
        let first=owner.verify_turn(&mut writer,job.id,1,1<<20,100,||{
            calls.set(calls.get()+1);if calls.get()==1{go.send(())?;admitted.recv_timeout(Duration::from_secs(10))?;}Ok(())})?;
        query.join().map_err(|_|anyhow::anyhow!("actual query fixture panicked"))??;
        assert!(!first.progress.complete());assert!(first.completed.is_none());
        let progress=serde_json::to_value(&first.progress)?;assert_eq!(progress["checked_files"],1);assert_eq!(progress["after"],"a.txt");
        drop(writer);
        let next_store=store.clone();std::thread::spawn(move||->Result<()>{let _writer=next_store.lock()?;Ok(())}).join().map_err(|_|anyhow::anyhow!("writer release fixture panicked"))??;
        progress
    };
    drop(lease);drop(registry);
    let control=temporary.path().canonicalize()?.join("control");
    let mut registry=ServiceRegistry::open_saved(&control)?;let mut lease=ProjectLease::acquire(&control,project)?;registry.admit_worker(&mut lease)?;
    registry.recover_jobs(&lease)?;registry.claim_job_id(&lease,job.id)?.context("same durable job not reclaimed")?;
    let bound=data_scope::bind(Path::new(&collection.owner.data_root),&control,namespace)?;
    let complete={let owner=collection.integrity_owner(&registry,&lease,&bound)?;let mut writer=store.lock()?;
        let last=owner.verify_turn(&mut writer,job.id,1,1<<20,200,||Ok(()))?;let after=serde_json::to_value(&last.progress)?;
        assert!(last.progress.complete());assert_eq!(after["checked_files"],2);assert_eq!(after["started_unix_ms"],before["started_unix_ms"]);
        assert_eq!(after["lexical_observed_unix_ms"],before["lexical_observed_unix_ms"]);assert!(after["checked_bytes"].as_u64()>before["checked_bytes"].as_u64());
        last.completed.context("complete physical proof missing")?};
    registry.complete_job_with_maintenance_and_audit_at(&lease,job.id,JobOutcome::Succeeded{revision:Some(complete.revision()?)},201,None,Some(AuditEvidence::Complete(&complete)))?;
    assert!(registry.integrity_audit_veto(&lease)?.is_none());
    Ok(())
}
''';p.write_text(s,encoding='utf-8',newline='\n')
