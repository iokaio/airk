# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
root = Path.cwd()
path = root / 'README.md'
text = path.read_text(encoding='utf-8')
start = text.index('**1.8.2 — more responsive installed startup and first status.**')
end = text.index('\n\n', start)
lead = text[start:end].replace('The owner approved the installed product and observed', 'Normal-editor acceptance testing observed')
text = text[:start] + text[end + 2:]
at = text.index('\n- **Find useful context:**')
workbench = '**Workbench improvements in 1.8.2:** readiness can proceed while saved history loads; New/Clear reuses the selected connection, Save avoids a duplicate session scan, and status/save updates send scalar state while preserving complete exports. New replies keep the question visible, sidebar switches retain reading position, and linked **Select all N / Copy selected** controls make returned evidence easier to use. Supported conversational requests retrieve code and document evidence through the shared native parser. The [performance report](docs/performance-improvements-1.8.2.md#workbench-responsiveness-and-everyday-use) separates measured message-byte reductions from usability improvements.'
text = text[:at] + '\n' + lead + '\n\n' + workbench + '\n' + text[at:]
path.write_text(text, encoding='utf-8')

path = root / 'packages/vscode/README.md'
text = path.read_text(encoding='utf-8')
start = text.index('**Version 1.8.2 improves installed responsiveness.**')
end = text.index('\n\n', start)
lead = text[start:end].replace('The owner approved the installed experience after observing', 'Normal-editor acceptance testing observed')
text = text[:start] + text[end + 2:]
at = text.index('## Meet AIRK Workbench')
text = text[:at] + lead + '\n\n**Workbench improvements:** readiness proceeds independently of saved-history loading; New/Clear reuses the selected connection, Save avoids a duplicate session scan, and scalar status/save updates preserve complete exports. Query-first reveal, retained reading position and linked Select all/Copy selected controls make evidence easier to inspect and copy. In the retained 200-turn fixture, scalar status messages are over 95% smaller than full-state messages; this measures transfer work, not an elapsed UI speedup.\n\n' + text[at:]
path.write_text(text, encoding='utf-8')

section = '''## Workbench responsiveness and everyday use

Workbench improvements are part of the release's main installed-experience story, alongside launcher/status behavior. They reduce avoidable preparation and transfer work and improve how returned evidence is read and copied.

| Change | User-visible benefit | Evidence / measurement boundary |
|---|---|---|
| Readiness independent of saved history | A slow history scan need not prevent preparing the selected project's connection and tools. | Implemented host lifetime/controller behavior; installed UI elapsed time was not instrumented. |
| Reuse the selected connection across New/Clear | Starting a transcript avoids discarding and rebuilding a healthy selected transport. | Lifecycle/identity tests; no universal reconnect speedup percentage. |
| Save avoids a duplicate session scan | Saving does less repeated local history work. | Implementation and session tests; storage latency not benchmarked. |
| Scalar status/save updates | Background updates avoid resending full result bodies while exports retain complete values. | Over 95% smaller status messages in the retained 200-turn fixture; byte counts, not elapsed UI speed. |
| Query-first reveal and retained reading position | New pending/completed/failed replies show the question; manual/nested reading and sidebar-tab position remain stable. | Compiled UI fixtures and owner-approved installed experience; no timed rendering claim. |
| Linked Select all N and Copy selected | Top/bottom controls select and copy the returned set, including unexpanded or filtered selections under the documented rules. | Functional keyboard/filter/copy/budget fixtures; no extra page is fetched. |
| Conversational evidence requests | Supported developer, writer, architect and QA phrasing prepares bounded native evidence plans while preserving identifiers. | 476-case inventory plus 48 known role regressions; unsupported/compound requests retain clarification and explicit review. |

See the [Workbench implementation](workbench/implementation.md), [release notes](releases/release-1.8.2.md) and [UI preparation/reuse record](optimizations.md#f16af17-ui-preparation-and-workbench-reuse). These usability and work-reduction improvements are separate from kernel throughput.

'''
path = root / 'docs/performance-improvements-1.8.2.md'
text = path.read_text(encoding='utf-8').replace('## Why responsiveness improved without a general native speedup', section + '## Why responsiveness improved without a general native speedup')
path.write_text(text, encoding='utf-8')
path = root / 'docs/releases/release-1.8.2.md'
text = path.read_text(encoding='utf-8').replace('## Conversational retrieval', '## Workbench responsiveness and everyday use\n\nWorkbench readiness now proceeds independently of saved-history loading. New/Clear retains a healthy selected connection, Save avoids a duplicate session scan, and scalar status/save messages avoid resending full result bodies while preserving complete exports. These changes reduce avoidable setup and UI transfer work. The retained 200-turn fixture measures over 95% smaller scalar status messages; it does not measure elapsed UI rendering speed.\n\nDaily use also improves: new replies reveal the question, sidebar switches retain manual/nested reading position, and linked top/bottom Select all N and Copy selected controls simplify collecting returned evidence. Supported conversational role requests use the shared bounded native parser. The detailed behavior and regression evidence follow below; the [Workbench section of the performance report](../performance-improvements-1.8.2.md#workbench-responsiveness-and-everyday-use) maps each change to its benefit and measurement limits.\n\n## Conversational retrieval', 1)
path.write_text(text, encoding='utf-8')
for name in ('docs/README.md', 'docs/vscode-extension.md'):
    path = root / name
    text = path.read_text(encoding='utf-8')
    marker = '**1.8.2 features:**'
    assert marker in text
    text = text.replace(marker, '**Workbench improvements:** independent readiness/history loading, selected-connection reuse across New/Clear, no duplicate Save scan, scalar status/save updates, query-first reveal, retained reading position and linked Select all/Copy selected controls. See the [Workbench performance and usability evidence](performance-improvements-1.8.2.md#workbench-responsiveness-and-everyday-use).\n\n' + marker, 1)
    path.write_text(text, encoding='utf-8')
