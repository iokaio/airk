# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f11-consolidated/source')
f=p/'crates/airk-cli/src/project_worker.rs';s=f.read_text(encoding='utf-8').replace('                        if kind == JobKind::Verify {\n                            loop {','                        if kind == JobKind::Verify {\n                            airk_kernel::service_execution::progress_phase(airk_core::service_progress::WorkPhase::Verifying);\n                            loop {',1);f.write_text(s,encoding='utf-8',newline='\n')
f=p/'crates/airk-store/src/service_collection_tests.rs';s=f.read_text(encoding='utf-8').replace('        assert!(!first.progress.complete());\n        assert!(first.completed.is_none());', '        assert!(!first.progress.complete());\n        assert_eq!(first.verified_files,1);\n        assert!(first.completed.is_none());',1)
s=s.replace('        assert!(last.progress.complete());\n        assert_eq!(after["checked_files"], 2);', '        assert!(last.progress.complete());\n        assert_eq!(last.verified_files,1);\n        assert_eq!(after["checked_files"], 2);',1)
f.write_text(s,encoding='utf-8',newline='\n')
