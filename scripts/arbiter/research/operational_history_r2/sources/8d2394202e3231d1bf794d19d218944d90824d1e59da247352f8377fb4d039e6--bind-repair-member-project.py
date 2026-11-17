# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f11-consolidated/source/crates/airk-store/src')
f=p/'service_maintenance.rs';s=f.read_text(encoding='utf-8');start=s.index('pub(super) fn job_is_physical_verify(');end=s.index('pub(super) fn job_recovers_publication',start)
b=s[start:end].replace('    job: Sha256Digest,\n','    job: Sha256Digest,\n    expected_project:ProjectBinding,\n',1)
b=b.replace('    let member = item(connection, plan, project)?;','    ensure!(project==expected_project,"foreign physical Repair project");\n    let member = item(connection, plan, project)?;',1)
s=s[:start]+b+s[end:];f.write_text(s,encoding='utf-8',newline='\n')
f=p/'service_integrity_guard.rs';s=f.read_text(encoding='utf-8').replace('super::maintenance::job_is_physical_verify(connection, job.id)?','super::maintenance::job_is_physical_verify(connection, job.id,job.project)?');f.write_text(s,encoding='utf-8',newline='\n')
