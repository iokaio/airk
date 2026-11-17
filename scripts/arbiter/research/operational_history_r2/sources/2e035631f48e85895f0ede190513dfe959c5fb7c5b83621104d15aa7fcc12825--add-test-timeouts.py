# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f09-draft/lazy-discovery')
f=p/'source/packages/vscode/src/test/serviceBootstrap.test.ts'
s=f.read_text(encoding='utf-8')
for title in ['lone healthy connection runs zero diagnose and later full discovery obtains fresh inspect exactly once','mandatory health and Stop failures retire connection-only attempts without diagnostics or poisoned reuse','connection waiters share mandatory work until the last health completes even after full settles']:
 old="test('"+title+"',async()=>{";new="test('"+title+"',{timeout:30000},async()=>{"
 assert s.count(old)==1,title
 s=s.replace(old,new)
f.write_text(s,encoding='utf-8',newline='\n')
f=p/'IMPLEMENTATION.txt';s=f.read_text(encoding='utf-8');s+='\nFinal test-only correction: all three new tests have explicit timeout:30000, matching existing connection gates. Missing held-entry signals therefore fail the bounded test gate rather than leaving E waiting indefinitely. Exact BASE input bytes remain unchanged; only draft test output/patch hashes change. No tests executed.\n';f.write_text(s,encoding='utf-8',newline='\n')
