# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f10-consolidated/source/crates/airk-store/src/service_collection_tests.rs')
s=p.read_text(encoding='utf-8');start=s.index('fn native_metadata_enrollment_requires_exact_writer')
s=s[:start]+s[start:].replace('(_temporary, collection, registry, bound, lease, store, _scope)','(temporary, collection, registry, bound, lease, store, _scope)',1).replace('_temporary.path().join("foreign-writer")','temporary.path().join("foreign-writer")',1)
p.write_text(s,encoding='utf-8',newline='\n')
