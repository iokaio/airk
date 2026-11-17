# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f08c2-draft/implementation/IMPLEMENTATION.txt')
s=p.read_text(encoding='utf-8').replace('a failed reset reports secondary diagnosticFailure and throws the original error.', 'a failed reset reports secondary diagnosticFailure and attempts the original namespace fallback; failed confirmation retains the original error except for the existing shared-timeout cleanup classification.')
s=s.replace('at most64','at most 64').replace('one second, independently of unchanged250ms','one second, independently of unchanged 250ms').replace('Store tests use actual SQLite/ServiceRegistry and real enqueue/claim/cancel/receipt methods:', 'Store tests use actual SQLite/ServiceRegistry and real enqueue/claim/cancel/receipt methods:').replace(';71 terminal jobs and4096','; 71 terminal jobs and 4096').replace('retire64 then7','retire 64 then 7')
p.write_text(s,encoding='utf-8',newline='\n')
