# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
def edit(path, old, new):
    p=Path(path); s=p.read_text(encoding='utf-8'); assert old in s, (path,old[:80]); p.write_text(s.replace(old,new),encoding='utf-8')
edit('crates/airk-mcp/src/lib.rs','pub mod remote;', 'pub mod remote;\npub mod query_capacity;')
edit('crates/airk-mcp/src/remote.rs','calls: Arc<tokio::sync::Semaphore>,','calls: Arc<crate::query_capacity::QueryCapacity>,')
edit('crates/airk-mcp/src/remote.rs','calls: Arc::new(tokio::sync::Semaphore::new(3)),','calls: crate::query_capacity::QueryCapacity::new(airk_core::service_resources::DEFAULT_QUERY_CONCURRENCY),')
edit('crates/airk-mcp/src/remote.rs','waiting: Arc::new(tokio::sync::Semaphore::new(8)),','waiting: Arc::new(tokio::sync::Semaphore::new(128)),')
edit('crates/airk-mcp/src/remote.rs','Result<tokio::sync::OwnedSemaphorePermit, AdmissionFailure>', 'Result<BridgePermit, AdmissionFailure>')
edit('crates/airk-mcp/src/remote.rs','Ok(permit) => return Ok(permit),','Ok(permit) => return Ok(BridgePermit::Status(permit)),')
# The generic lane is a separately typed owned adjustable permit.
edit('crates/airk-mcp/src/remote.rs','match lane.clone().try_acquire_owned() {\n                Ok(permit) => return Ok(BridgePermit::Status(permit)),','match lane.clone().try_acquire_owned() {\n                Ok(permit) => return Ok(BridgePermit::Evidence(permit)),')
edit('crates/airk-mcp/src/remote.rs','=> Ok(permit),\n                Ok(Ok(_))', '=> Ok(BridgePermit::Evidence(permit)),\n                Ok(Ok(_))')
edit('crates/airk-mcp/src/remote.rs','#[derive(Debug, PartialEq, Eq)]\nenum AdmissionFailure', '#[derive(Debug)]\nenum BridgePermit {\n    Evidence(crate::query_capacity::QueryPermit),\n    Status(tokio::sync::OwnedSemaphorePermit),\n}\n\n#[derive(Debug, PartialEq, Eq)]\nenum AdmissionFailure')
edit('crates/airk-mcp/src/remote.rs','let _permit = match self\n            .admission', 'self.admission.calls.configure(self.backend.query_concurrency());\n        let _permit = match self\n            .admission')
# Old small-capacity boundary tests keep their explicit fixture capacity.
edit('crates/airk-mcp/src/remote.rs','let admission = Admission::default();','let admission = Admission { calls: crate::query_capacity::QueryCapacity::new(3), status: Arc::new(tokio::sync::Semaphore::new(1)), waiting: Arc::new(tokio::sync::Semaphore::new(8)) };')
edit('crates/airk-mcp/src/remote.rs','enum BridgePermit {','#[allow(dead_code)] // Variants retain their owned permits until handler completion.\nenum BridgePermit {')
