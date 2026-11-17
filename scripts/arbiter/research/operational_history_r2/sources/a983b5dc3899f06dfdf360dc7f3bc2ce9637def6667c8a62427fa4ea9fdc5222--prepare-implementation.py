# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json
p=Path('artifacts/optimizations/f08c2-draft/implementation')
names=['crates/airk-store/src/service_jobs.rs','crates/airk-store/src/service_registry.rs','crates/airk-cli/src/service_management.rs','packages/vscode/src/serviceConsoleController.ts','packages/vscode/src/test/serviceConsole.test.ts']
record={}
for name in names:
 f=Path(name);record[name]={'sha256':hashlib.sha256(f.read_bytes()).hexdigest()}
 for layer in ['base','source']:
  target=p/layer/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(f.read_bytes())
(p/'BASE.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
f=p/'source/crates/airk-store/src/service_registry.rs';s=f.read_text(encoding='utf-8');pos=s.index('/// Only callers that have established')
s=s[:pos]+'''/// One pressure-triggered receipt turn in the existing service metadata owner.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct JobReceiptMaintenance {
    pub retired: u64,
    pub pending: bool,
}

'''+s[pos:];f.write_text(s,encoding='utf-8',newline='\n')
f=p/'source/crates/airk-store/src/service_jobs.rs';s=f.read_text(encoding='utf-8').replace('use super::ServiceRegistry;','use super::{ServiceRegistry, JobReceiptMaintenance};')
# Existing request_epoch/enqueue call sites ignore the count and preserve their exact pruning admission.
start=s.index('    fn prune_job_receipts(');end=s.index('    pub fn request_epoch',start)
block=s[start:end].replace('    ) -> Result<()> {','    ) -> Result<u64> {',1)
block=block.replace('        let count_full =','        let mut retired = 0;\n        let count_full =',1).replace('            if deleted > 0 {','            retired = deleted as u64;\n            if deleted > 0 {',1)
block=block.replace('        Ok(())\n    }','        Ok(retired)\n    }',1)
s=s[:start]+block+'''    // Hint only: no IMMEDIATE transaction or durable change on a healthy idle tick.
    fn job_receipt_pressure(&self) -> Result<bool> {
        let requests:i64=self.connection.query_row("SELECT count(*) FROM service_requests",[],|row|row.get(0))?;
        Ok(requests >= MAX_REQUESTS || metadata_pressure(&self.connection)?)
    }
    /// Same pressure and retirement policy as RequestEpoch(false), independent of UI.
    /// At most64 terminal jobs; pending/running jobs and their accepted keys remain.
    pub fn maintain_job_receipts(&mut self) -> Result<JobReceiptMaintenance> {
        if !self.job_receipt_pressure()? { return Ok(JobReceiptMaintenance::default()); }
        // Authoritative thresholds/protected states are rechecked inside IMMEDIATE.
        let retired=self.prune_job_receipts(false,None)?;
        let pending=self.job_receipt_pressure()? && self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM service_jobs WHERE state IN('succeeded','failed','canceled'))",[],|row|row.get::<_,bool>(0))?;
        Ok(JobReceiptMaintenance {retired,pending})
    }

'''+s[end:];f.write_text(s,encoding='utf-8',newline='\n')
f=p/'source/crates/airk-cli/src/service_management.rs';s=f.read_text(encoding='utf-8')
s=s.replace('                let mut metadata_failed = false;', '                let mut metadata_failed = false;\n                let mut next_receipts=std::time::Instant::now();\n                let mut receipt_pending=false;\n                let mut receipt_failed=false;')
needle='                        if running.is_none() && !metadata_failed {'
s=s.replace(needle,'''                        if std::time::Instant::now() >= next_receipts {
                            let retired=registry.as_mut().ok().and_then(|registry|registry.maintain_job_receipts().ok());
                            receipt_pending=retired.as_ref().is_some_and(|turn|turn.pending);
                            receipt_failed=retired.is_none();
                            // Existing owner only: one <=64 retirement turn per second,
                            // regardless of UI requests; queued work still gets a turn.
                            next_receipts=std::time::Instant::now()+Duration::from_secs(1);
                        }
                        let failed=running.is_none()||receipt_failed;
                        if failed && !metadata_failed {''',1)
s=s.replace('                        metadata_failed = running.is_none();','                        metadata_failed = failed;',1)
s=s.replace('runnable_plans.store(running.unwrap_or(true), Ordering::Release);','runnable_plans.store(running.unwrap_or(true)||receipt_pending||receipt_failed, Ordering::Release);',1)
s=s.replace('Duration::from_millis(if running.is_some() { 250 } else { 1000 })','Duration::from_millis(if failed { 1000 } else { 250 })',1)
f.write_text(s,encoding='utf-8',newline='\n')
