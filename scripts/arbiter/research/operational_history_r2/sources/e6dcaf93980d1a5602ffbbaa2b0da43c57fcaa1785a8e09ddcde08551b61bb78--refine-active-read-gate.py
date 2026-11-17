# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f11-consolidated/source/crates/airk-store/src/service_collection_tests.rs')
s=p.read_text(encoding='utf-8');start=s.index('fn real_retained_pages_release_writer')
s=s[:start]+s[start:].replace('if calls.get() == 1 {','if calls.get() == 2 {\n                // The audit already owns its actual CurrentRead and writer.',1)
p.write_text(s,encoding='utf-8',newline='\n')
