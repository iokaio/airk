# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f06-draft/rebased-f05/source/crates/airk-cli/src')
u=p/'unified_mcp.rs';s=u.read_text().replace('''            lookup: Arc::new(SourceLookupOwner::new()),
            backend:''','''            lookup: Arc::new(SourceLookupOwner::new()),
            status: Arc::new(crate::project_client::StatusOwner::default()),
            backend:''');u.write_text(s,newline='\n')
# Keep a separate untouched F05 regression inventory: adaptations must preserve
# assertions under changed status ownership, not silently delete coverage.
import re,json
live=Path('crates/airk-cli/src/project_client.rs').read_text()
(out:=p.parent.parent.parent/'F05-REGRESSION-INVENTORY.json').write_text(json.dumps({'project_client_tests':re.findall(r'(?:async )?fn ([a-zA-Z0-9_]+)\(',live[live.index('#[cfg(test)]'):]),'notes':'All live source_brokerage_ipc_tests retained in rebased source; owner-specific assertion adaptations still required before review/compile.'},indent=2)+'\n')
# The status-owner fixture constructs the actual Connection, which now requires
# the authenticated grant identity retained by F05.
f=p/'status_observation.rs';s=f.read_text().replace('''let mut connection=Connection{stream,authorized_until:''','''let mut connection=Connection{stream,identity:airk_core::service::WorkerIdentity{
            service:airk_core::identity::Incarnation::from_bytes([1;16]),
            worker:airk_core::identity::Incarnation::from_bytes([2;16]),
            project, fencing_generation:1},authorized_until:''');f.write_text(s,newline='\n')
