# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('artifacts/optimizations/f16b-draft/source/crates/airk-cli/src/unified_mcp/explorer_adapter.rs')
s=p.read_text(encoding='utf-8')
s=s.replace('error.downcast_ref::<std::io::Error>().unwrap().kind(),','error.chain().find_map(|cause| cause.downcast_ref::<std::io::Error>()).unwrap().kind(),')
p.write_text(s,encoding='utf-8',newline='\n')
p=Path('artifacts/optimizations/f16b-draft/IMPLEMENTATION.txt')
s=p.read_text(encoding='utf-8')
s=s.replace('Absolute 15 seconds now includes queue/setup/verification/reply; old one-shot timing began around child work. This tighter admission behavior requires review and actual workload qualification.', 'Review correction: queue/verification remain cancellation-aware preparation without consuming the historical child-work allowance. Cold execution starts its one 15-second deadline immediately after spawn; initialize and the user request share that deadline without renewal. A warm call starts its 15-second exchange allowance only after the final binding recheck. Queued abandoned work stays owned and must drain; there is no claimed hard setup/queue wall-clock bound.')
s=s.replace('A source refusal remains its original typed result', 'A source refusal remains its original typed result')
s += '\nReview corrections 2026-10-31\nThe resolver is actually invoked again immediately before tools/call, after bundle checks/drain/initialize. Refusal/error drops the child and preserves the typed refusal; incompatible binding refuses without replay. An ignored real-JS fixture holds this post-initialize resolver, removes the source, then verifies exact refusal and disposed adapter. I/O joins preserve the first observed reply/timeout rather than substituting teardown BrokenPipe; a write-first event preserves its actual writer cause with secondary failure retained. Pure error-order and test-owned clock-instant budget cases are drafted. These corrections passed rustfmt and patch apply-check only, not compilation/tests.\n'
p.write_text(s,encoding='utf-8',newline='\n')
