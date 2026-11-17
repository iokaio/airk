# SPDX-License-Identifier: Apache-2.0
from pathlib import Path

def edit(path, replacements):
    p=Path(path); text=p.read_text(encoding='utf-8')
    for old,new in replacements:
        assert old in text, (path,old)
        text=text.replace(old,new)
    p.write_text(text,encoding='utf-8',newline='\n')

for path in ['README.md','packages/vscode/README.md','docs/README.md']:
    edit(path,[('**1.8.7 release candidate:**','**1.8.7:**')])
for path in ['docs/mcp.md','docs/unified-mcp.md','docs/mcp-developer-guide.md']:
    replacements=[('## Request capacity in status and diagnostics (1.8.6)','## Request capacity in status, diagnostics and plans'),('In the 1.8.4 local candidate, a completed status query','A completed status query'),('Cached reuse still has its five-second limit, and 1.8.5 uses the configured evidence capacity plus one reserved status slot.','Cached reuse has its five-second limit; configured evidence capacity has one additional reserved status slot.')]
    if path!='docs/mcp-developer-guide.md': replacements.append(('## Concurrent evidence requests (1.8.5)','## Concurrent evidence requests'))
    if path=='docs/mcp.md': replacements.extend([('The **1.8.7 release candidate**','**AIRK 1.8.7**'),('AIRK 1.8.2 carries forward the 1.8.0 work that reduces','AIRK 1.8.7 includes the response preparation work that reduces')])
    if path=='docs/unified-mcp.md': replacements.extend([('The AIRK 1.8.6 candidate retains','AIRK 1.8.7 retains'),('The 1.8.5 bridge admits','The bridge admits')])
    if path!='docs/mcp.md': replacements.append(('CLI status/diagnostics and Workbench `/status` and `/diagnostics` also include `query_concurrency` inside completed step results. The shared frontend/CLI response adapter reuses its policy observation, including for outer `parse` requests; raw payload accounting and output limits still apply. This follow-up requires a rebuilt runtime; existing saved replies retain their original fields.','CLI status/diagnostics include top-level `query_concurrency`. Workbench `/status` and `/diagnostics`, CLI-executed plans and MCP `parse` retain it inside each completed step result. The shared frontend/CLI response adapter reuses the outer request\'s admission observation; payload accounting and output limits still apply. Previously saved replies retain their original fields; rerun them with 1.8.7 to obtain the new data.'))
    edit(path,replacements)
edit('docs/cli-developer-guide.md',[('In the 1.8.4 local candidate, a completed status query','A completed status query'),('Cached reuse still has its five-second limit, and 1.8.5 uses the configured evidence capacity plus one reserved status slot.','Cached reuse has its five-second limit; configured evidence capacity has one additional reserved status slot.')])
edit('docs/release-workflow.md',[('Enter the exact checked-in version (`1.8.0`)','Enter the exact checked-in version (`1.8.7`)'),('version=1.8.0','version=1.8.7')])
edit('docs/vscode-extension.md',[('the current release line is **1.8.2**','the current release line is **1.8.7**')])

# Keep current product guidance together. Candidate narratives remain in the
# consolidated, dated release history rather than competing with the new version.
for path in ['README.md','packages/vscode/README.md']:
    p=Path(path); text=p.read_text(encoding='utf-8')
    start=text.index('**1.8.5 local candidate')
    end=text.index('- **Find useful context:**',start)
    url='docs/releases/release-1.8.7.md' if path=='README.md' else 'https://github.com/iokaio/airk/blob/main/docs/releases/release-1.8.7.md'
    text=text[:start]+f'''**Configurable concurrent evidence requests.** Management → Service → **Concurrent requests…** selects **4, 8, 16, 32 or 64**, default **16**, with one additional status/control slot. MCP admission, independent native channels and project workers use the same setting. Changes apply to subsequent requests without restarting agents; accepted work drains when capacity decreases. Finite queues, source scope, revision identity and typed retry receipts remain authoritative.

**Complete evidence across interfaces.** CLI and Workbench retain the MCP handlers' full results: provenance, readiness, coverage, continuations and retry details. Workbench exposes filters, exact/suffix/contains matching, count/file summaries and conditional/continuity inputs. **Complete response** and full JSON/Markdown exports retain the result even when cited cards are shown; selected-evidence exports identify their subset. Status and diagnostics report configured capacity, including inside completed plan steps. See the [1.8.7 behavior and validation]({url}).

'''+text[end:]
    if path=='packages/vscode/README.md':
        start=text.index('**Why 1.8.4:**'); end=text.index('## Meet AIRK Workbench',start)
        text=text[:start]+'''**Retained session choices:** each session keeps its selected tool, per-tool inputs and shared Scope/Recipe. Saved history restores those choices. **Reset this tool form** clears the active fields; New, Clear and Fork start fresh. Editing these choices submits no evidence request.

'''+text[end:]
    p.write_text(text,encoding='utf-8',newline='\n')

print('Updated current MCP/CLI guides, release examples and product descriptions.')
