# SPDX-License-Identifier: Apache-2.0
from pathlib import Path

p=Path('docs/README.md'); text=p.read_text(encoding='utf-8')
start=text.index('**Earlier 1.8.4 development:**'); end=text.index('**1.7.6 stable release',start)
text=text[:start]+'''The [CLI/MCP/Workbench parity audit](read-interface-parity.md) maps every native read, complete response/input contracts, conditional/continuity options and full versus selected exports. [Installed MCP checks](mcp-improvements-test-results.md#installed-187-acceptance--november-1-2026) record the 78-check Windows campaign, its permitted retry and the remaining navigation limits. Earlier 20-scenario investigations remain explicitly historical.

[Release records](releases/README.md) retains the consolidated 1.8.7 notes and dated development candidates. The [release workflow](release-workflow.md) describes exact-main CI, native signing, Marketplace promotion and published-output verification.

[Performance improvements and causal analysis](performance-improvements-1.8.2.md) retains the earlier installed-responsiveness measurements and separate native regressions. Those unmatched samples are not a controlled 1.8.7 speedup claim. [Workbench implementation](workbench/implementation.md) describes retained session/tool choices, query reveal, reading position, complete exports and local evidence parsing.

'''+text[end:]; p.write_text(text,encoding='utf-8',newline='\n')
p=Path('docs/releases/README.md'); text=p.read_text(encoding='utf-8').replace('[1.8.7 consolidated candidate]','[1.8.7]').replace('Workbench/MCP parity and dated validation.','complete CLI/MCP/Workbench read results, configured concurrency and dated validation.');p.write_text(text,encoding='utf-8',newline='\n')
p=Path('README.md'); text=p.read_text(encoding='utf-8').replace('**1.8.4 Workbench session choices:**','**Workbench session choices:**');p.write_text(text,encoding='utf-8',newline='\n')
print('Current documentation index and release summaries agree with 1.8.7.')
