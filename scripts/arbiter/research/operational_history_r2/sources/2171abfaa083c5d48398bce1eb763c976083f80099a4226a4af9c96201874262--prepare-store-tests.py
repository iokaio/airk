# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f08c2-draft/implementation/source/crates/airk-store/src/service_jobs.rs')
s=p.read_text(encoding='utf-8');pos=s.rfind('\n}')
s=s[:pos]+'''
    #[test]
    fn receipt_owner_healthy_precheck_is_read_only_and_authoritative_recheck_can_cancel_pressure() -> Result<()> {
        let root=tempfile::tempdir()?;let caller=Incarnation::from_bytes([61;16]);
        let mut registry=ServiceRegistry::open(root.path(),caller)?;
        let project=ProjectRegistration::new("/source".into(),"/state".into(),"receipt-maintenance".into())?;
        registry.register(&project)?;
        let (namespace,epoch)=registry.saved_request_epoch()?;
        let key=RequestKey {namespace,epoch,nonce:caller};
        let job=registry.enqueue(project.binding,caller,&key,JobKind::Index)?;
        registry.cancel_pending(job.id)?;
        let before=registry.connection.total_changes();
        registry.connection.pragma_update(None,"query_only",true)?;
        assert_eq!(registry.maintain_job_receipts()?,JobReceiptMaintenance::default());
        assert_eq!(registry.connection.total_changes(),before);
        assert_eq!(registry.saved_request_epoch()?,(namespace,epoch));
        registry.connection.pragma_update(None,"query_only",false)?;
        let tx=registry.connection.transaction()?;
        for n in 0..MAX_REQUESTS {
            let nonce=framed_digest(b"receipt-owner-alias",&[&n.to_le_bytes()]);
            tx.execute("INSERT INTO service_requests(namespace,epoch,nonce,caller,job) VALUES(?1,?2,?3,?4,?5)",params![namespace.to_string(),epoch,nonce.to_string(),caller.to_string(),job.id.to_string()])?;
        }
        tx.commit()?;
        assert!(registry.job_receipt_pressure()?);
        // A different metadata action removed the pressure between hint and
        // authoritative helper. The real write transaction must not retire a job.
        registry.connection.execute("DELETE FROM service_requests WHERE nonce<>?1",[caller.to_string()])?;
        assert_eq!(registry.prune_job_receipts(false,None)?,0);
        assert_eq!(registry.saved_request_epoch()?,(namespace,epoch));
        assert_eq!(registry.request_receipt(caller,&key)?.0.unwrap().id,job.id);
        Ok(())
    }
    #[test]
    fn bounded_owner_turns_retire_only_terminal_receipts_rotate_once_and_preserve_accepted_work() -> Result<()> {
        let root=tempfile::tempdir()?;let caller=Incarnation::from_bytes([62;16]);
        let mut registry=ServiceRegistry::open(root.path(),caller)?;
        let project=ProjectRegistration::new("/source".into(),"/state".into(),"receipt-maintenance".into())?;
        registry.register(&project)?;
        let (namespace,epoch)=registry.saved_request_epoch()?;
        let retired_key=RequestKey {namespace,epoch,nonce:Incarnation::from_bytes([63;16])};
        let retired=registry.enqueue(project.binding,caller,&retired_key,JobKind::Index)?;
        registry.cancel_pending(retired.id)?;
        let running_key=RequestKey {nonce:Incarnation::from_bytes([64;16]),..retired_key.clone()};
        let running=registry.enqueue(project.binding,caller,&running_key,JobKind::Index)?;
        let mut lease=ProjectLease::acquire(root.path(),project.binding)?;
        registry.admit_worker(&mut lease)?;registry.claim_job_id(&lease,running.id)?;
        let pending_key=RequestKey {nonce:Incarnation::from_bytes([65;16]),..retired_key.clone()};
        let pending=registry.enqueue(project.binding,caller,&pending_key,JobKind::Rebuild)?;
        let last=framed_digest(b"receipt-owner-terminal",&[&69i64.to_le_bytes()]);
        let tx=registry.connection.transaction()?;
        for n in 0..70i64 {
            let id=framed_digest(b"receipt-owner-terminal",&[&n.to_le_bytes()]);
            tx.execute("INSERT INTO service_jobs(id,project,kind,state,epoch,attempts) VALUES(?1,?2,'verify','canceled',0,0)",params![id.to_string(),project.binding.to_string()])?;
        }
        for n in 0..MAX_REQUESTS {
            let nonce=framed_digest(b"receipt-owner-many-aliases",&[&n.to_le_bytes()]);
            tx.execute("INSERT INTO service_requests(namespace,epoch,nonce,caller,job) VALUES(?1,?2,?3,?4,?5)",params![namespace.to_string(),epoch,nonce.to_string(),caller.to_string(),last.to_string()])?;
        }
        tx.commit()?;
        let first=registry.maintain_job_receipts()?;
        assert_eq!(first,JobReceiptMaintenance {retired:64,pending:true});
        assert_eq!(registry.saved_request_epoch()?.1,epoch+1);
        for (key,id) in [(&running_key,running.id),(&pending_key,pending.id)] {
            let (accepted,expired)=registry.request_receipt(caller,key)?;
            assert!(!expired);assert_eq!(accepted.unwrap().id,id);
        }
        assert_eq!(registry.enqueue(project.binding,caller,&running_key,JobKind::Index)?.id,running.id);
        assert_eq!(registry.job(running.id)?.state,JobState::Running);
        assert_eq!(registry.job(pending.id)?.state,JobState::Pending);
        let (missing,expired)=registry.request_receipt(caller,&retired_key)?;
        assert!(missing.is_none()&&expired);
        assert!(matches!(registry.enqueue(project.binding,caller,&retired_key,JobKind::Index).unwrap_err().downcast_ref(),Some(JobError::KeyExpired)));
        let second=registry.maintain_job_receipts()?;
        assert_eq!(second,JobReceiptMaintenance {retired:7,pending:false});
        assert_eq!(registry.saved_request_epoch()?.1,epoch+2);
        assert_eq!(registry.maintain_job_receipts()?,JobReceiptMaintenance::default());
        assert_eq!(registry.saved_request_epoch()?.1,epoch+2);
        Ok(())
    }
'''+s[pos:];p.write_text(s,encoding='utf-8',newline='\n')
