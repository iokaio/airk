# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f08c2-draft/implementation/source/crates/airk-cli/src/service_management.rs')
s=p.read_text(encoding='utf-8');pos=s.rfind('\n}')
s=s[:pos]+'''
    #[test]
    fn owned_receipt_turn_and_canceled_management_waiter_keep_service_lease_until_drain() -> Result<()> {
        use std::future::Future;
        use airk_core::service::{JobKind,RequestKey};
        let owner=tempfile::tempdir()?;let root=owner.path().join("control");
        crate::local_ipc::prepare_root(&root)?;
        let ownership=airk_store::service::ServiceLease::acquire(&root)?;
        let caller=Incarnation::from_bytes([82;16]);
        let mut registry=ServiceRegistry::open(&root,caller)?;
        let project=ProjectRegistration::new("/never-read-source".into(),"/never-open-index".into(),"receipt-owner".into())?;
        registry.register(&project)?;
        let (namespace,epoch)=registry.saved_request_epoch()?;
        let key=RequestKey {namespace,epoch,nonce:caller};
        let job=registry.enqueue(project.binding,caller,&key,JobKind::Index)?;registry.cancel_pending(job.id)?;
        let mut connection=rusqlite::Connection::open(root.join("service-1.4.sqlite3"))?;
        let tx=connection.transaction()?;
        for n in 0..4096i64 {
            let mut nonce=[83u8;16];nonce[..8].copy_from_slice(&n.to_le_bytes());
            tx.execute("INSERT INTO service_requests(namespace,epoch,nonce,caller,job) VALUES(?1,?2,?3,?4,?5)",rusqlite::params![namespace.to_string(),epoch,Incarnation::from_bytes(nonce).to_string(),caller.to_string(),job.id.to_string()])?;
        }
        tx.commit()?;drop(connection);
        let (sender,receiver)=sync_channel::<Envelope>(2);
        let (release,gate)=sync_channel(1);let (ready,turn)=sync_channel(1);let (done,drained)=sync_channel(1);
        let pending=Arc::new(AtomicUsize::new(0));let active=pending.clone();
        let thread=std::thread::spawn(move || {
            let _ownership=ownership;
            let report=registry.maintain_job_receipts().unwrap();
            ready.send(report).unwrap();
            gate.recv_timeout(Duration::from_secs(15)).unwrap();
            for (method,reply) in receiver {
                let response=execute_request(&mut registry,method);
                active.fetch_sub(1,Ordering::AcqRel);let _=reply.send(response);let _=done.send(());
            }
        });
        let management=Management {sender:Some(sender),thread:Some(thread),pending,plans:Arc::new(AtomicBool::new(false))};
        let retired=turn.recv_timeout(Duration::from_secs(15))?;
        assert_eq!(retired.retired,1);assert!(!retired.pending);
        let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        {
            let _entered=runtime.enter();
            let mut request=Box::pin(management.request(ControlMethod::RequestReceipt {caller,key,project:Some(project.binding)}));
            let mut context=std::task::Context::from_waker(std::task::Waker::noop());
            assert!(request.as_mut().poll(&mut context).is_pending());
            drop(request); // actual accepted queue charge survives the caller
        }
        assert!(!management.can_idle_exit());
        assert!(airk_store::service::ServiceLease::acquire(&root).is_err());
        release.send(())?;drained.recv_timeout(Duration::from_secs(15))?;
        assert!(management.can_idle_exit());
        assert!(airk_store::service::ServiceLease::acquire(&root).is_err());
        drop(management); // actual Drop closes queue and joins its native owner
        assert!(airk_store::service::ServiceLease::acquire(&root).is_ok());
        Ok(())
    }
'''+s[pos:];p.write_text(s,encoding='utf-8',newline='\n')
