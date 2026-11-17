# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f11-consolidated/source/crates/airk-cli/src/project_worker.rs')
s=p.read_text(encoding='utf-8').replace('if turn.blocked_large {','if turn.blocked_large || turn.progress.requires_repair() {',1)
p.write_text(s,encoding='utf-8',newline='\n')
