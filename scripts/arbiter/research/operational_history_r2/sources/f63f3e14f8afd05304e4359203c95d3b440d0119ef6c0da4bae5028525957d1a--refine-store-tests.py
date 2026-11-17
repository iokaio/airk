# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f08c2-draft/implementation/source/crates/airk-store/src/service_jobs.rs')
s=p.read_text(encoding='utf-8')
s=s.replace('let nonce=framed_digest(b"receipt-owner-alias",&[&n.to_le_bytes()]);','let mut bytes=[91u8;16];bytes[..8].copy_from_slice(&n.to_le_bytes());let nonce=Incarnation::from_bytes(bytes);')
s=s.replace('let nonce=framed_digest(b"receipt-owner-many-aliases",&[&n.to_le_bytes()]);','let mut bytes=[92u8;16];bytes[..8].copy_from_slice(&n.to_le_bytes());let nonce=Incarnation::from_bytes(bytes);')
line='        assert!(matches!(registry.enqueue(project.binding,caller,&retired_key,JobKind::Index).unwrap_err().downcast_ref(),Some(JobError::KeyExpired)));\n'
assert line in s;s=s.replace(line,'',1)
needle='        assert_eq!(registry.saved_request_epoch()?.1,epoch+2);\n        assert_eq!(registry.maintain_job_receipts()?,JobReceiptMaintenance::default());'
s=s.replace(needle,'        assert_eq!(registry.saved_request_epoch()?.1,epoch+2);\n'+line+'        assert_eq!(registry.maintain_job_receipts()?,JobReceiptMaintenance::default());')
s=s.replace("tx.execute(\"INSERT INTO service_jobs(id,project,kind,state,epoch,attempts) VALUES(?1,?2,'verify','canceled',0,0)\",params![id.to_string(),project.binding.to_string()])?;", "tx.execute(\"INSERT INTO service_jobs(id,project,kind,state,epoch,attempts,outcome) VALUES(?1,?2,'verify','canceled',0,0,?3)\",params![id.to_string(),project.binding.to_string(),serde_json::to_vec(&JobOutcome::Canceled)?])?;")
p.write_text(s,encoding='utf-8',newline='\n')
