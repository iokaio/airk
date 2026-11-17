# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f11-consolidated/source')
f=p/'crates/airk-store/src/service_collection_verify.rs';s=f.read_text(encoding='utf-8').replace('pub struct VerifyTurn {','pub struct VerifyTurn {\n    pub verified_files:u64,',1)
s=s.replace('        let mut blocked_large = false;', '        let files_before=progress.checked_files;\n        let mut blocked_large = false;',1)
s=s.replace('        Ok(VerifyTurn {\n            progress,','        Ok(VerifyTurn {\n            verified_files:progress.checked_files.saturating_sub(files_before),\n            progress,',1)
f.write_text(s,encoding='utf-8',newline='\n')
f=p/'crates/airk-kernel/src/diagnostics.rs';s=f.read_text(encoding='utf-8');start=s.index('    pub fn verify_service_turn(')
s=s[:start]+s[start:].replace('        owner.verify_turn(&mut writer, job, 32, 64 << 20, now,\n            crate::service_execution::checkpoint)', '        let turn=owner.verify_turn(&mut writer, job, 32, 64 << 20, now,\n            crate::service_execution::checkpoint)?;\n        crate::service_execution::progress_verified(turn.verified_files);\n        Ok(turn)',1)
# It may already be formatted: replace exact tail if multiline.
if 'progress_verified(turn.verified_files)' not in s:
 tail='''        owner.verify_turn(
            &mut writer,
            job,
            32,
            64 << 20,
            now,
            crate::service_execution::checkpoint,
        )'''
 assert tail in s;s=s.replace(tail,tail.replace('owner.verify_turn(', 'let turn=owner.verify_turn(',1)+'?;\n        crate::service_execution::progress_verified(turn.verified_files);\n        Ok(turn)',1)
s=s.replace('''        let collection = self
            .service_collection''','''        crate::service_execution::progress_phase(airk_core::service_progress::WorkPhase::Verifying);
        let collection = self
            .service_collection''',1) if False else s
f.write_text(s,encoding='utf-8',newline='\n')
