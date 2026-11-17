# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f08c2-draft/implementation/source/crates/airk-store/src/service_jobs.rs')
s=p.read_text(encoding='utf-8');needle='            .pragma_update(None, "query_only", false)?;'
insert='''
        // A separate actual writer owns admission; a healthy hint must still
        // succeed without attempting its own IMMEDIATE transaction.
        let mut competing=Connection::open(root.path().join("service-1.4.sqlite3"))?;
        let held=competing.transaction_with_behavior(TransactionBehavior::Immediate)?;
        registry.connection.busy_timeout(std::time::Duration::ZERO)?;
        assert_eq!(registry.maintain_job_receipts()?,JobReceiptMaintenance::default());
        drop(held);drop(competing);
'''
assert needle in s;s=s.replace(needle,needle+insert,1);p.write_text(s,encoding='utf-8',newline='\n')
