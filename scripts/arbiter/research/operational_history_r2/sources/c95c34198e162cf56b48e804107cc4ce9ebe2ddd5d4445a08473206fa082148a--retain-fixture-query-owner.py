# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f11-consolidated/source/crates/airk-store/src/service_collection_tests.rs')
s=p.read_text(encoding='utf-8');start=s.index('fn real_retained_pages_release_writer');a=s[:start];b=s[start:]
b=b.replace('        let first = owner.verify_turn(', '        let first_result = owner.verify_turn(',1)
b=b.replace('''        })?;
        query
            .join()
            .map_err(|_| anyhow::anyhow!("actual query fixture panicked"))??;
        assert!(!first.progress.complete());''','''        });
        drop(go);
        let query_result=query.join().map_err(|_|anyhow::anyhow!("actual query fixture panicked"));
        let first=first_result?;
        query_result??;
        assert!(!first.progress.complete());''',1)
assert 'let first=first_result?' in b;s=a+b;p.write_text(s,encoding='utf-8',newline='\n')
p=Path('artifacts/optimizations/f11-consolidated/source/crates/airk-store/src/service_collection_verify.rs');s=p.read_text(encoding='utf-8').replace('/// verifies complete components, drains owned reads and persists progress.', '/// verifies the declared lexical/retained-input components, drains owned reads\n/// and persists dated progress. This is not a certificate for future F13\n/// projections, graph/vector bytes or every file in a private data root.')
p.write_text(s,encoding='utf-8',newline='\n')
