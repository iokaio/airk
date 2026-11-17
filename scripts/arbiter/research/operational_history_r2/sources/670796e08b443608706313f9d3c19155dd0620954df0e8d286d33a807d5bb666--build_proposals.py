# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,difflib
P=Path('artifacts/advanced-split/arbiter-help-preparation-r1');C={}
def change(name,fn):
 old=Path(name).read_text(encoding='utf-8');C[name]=(old,fn(old))
name='crates/airk-mcp/src/help/topics.rs';old=Path(name).read_text(encoding='utf-8');new=(P/'files'/name).read_text(encoding='utf-8');new=new.replace('omissions and stop reasons."#\n    },','omissions and stop reasons."#,\n    },');C[name]=(old,new)
def cases(s):
 n='                "MiniLM",\n                "excluded indexes",'
 r='''                "MiniLM",
                "Arbiter Focus",
                "Arbiter Context — Compact",
                "Arbiter Context — Large",
                "Qwen3-Reranker-0.6B",
                "Qwen3-1.7B",
                "Qwen3-4B",
                "selected versus active",
                "offline import",
                "no universal RAM minimum",
                "excluded indexes",'''
 assert n in s;return s.replace(n,r)
change('crates/airk-mcp/src/help/content_cases.rs',cases)
service='''
## Optional Arbiter stages

The approved source design separates **Arbiter Focus** (Qwen3-Reranker-0.6B) from **Arbiter Context — Compact** (Qwen3-1.7B) and **Arbiter Context — Large** (Qwen3-4B). Focus ranks MiniLM's retrieved candidates and can run with Context off. Context assesses evidence and guides bounded additional reads. Only one Context model is active. The stage controls and profiles remain subject to implementation and qualification; inspect the running service rather than assuming this guide enables them.

Both stages are off by default and need their own explicit index enrollment and verified supported assets. Model selection is intended configuration; active readiness additionally needs task/platform support and current resources. Status, ordinary queries and Help never authorize model downloads. Use native Arbiter status and current generated command/schema guidance for supported controls.

Focus still has a memory and latency cost; candidate count and passage length matter. There is no universal RAM floor or assumed GPU support. An unavailable stage retains ordinary retrieval with an explicit reason, while normal vector/completeness refusals still apply. Disablement drains native work; remove models through guarded service controls instead of deleting live assets. Native Help's Arbiter guide explains requirements, offline use, stage consent, fallback and recovery.
'''
change('packages/vscode/src/explorerHelp/manage-service.md',lambda s:s.replace('aliases: start, stop, restart, upgrade, rollback, embedding, resources','aliases: start, stop, restart, upgrade, rollback, embedding, resources, Arbiter Focus, Arbiter Context, Qwen, MiniLM').replace('related_airk_help: service, upgrades, resources','related_airk_help: service, upgrades, resources, models, arbiter')+service)
search='''
## MiniLM, Focus and Context

MiniLM supplies reusable vectors for candidate retrieval. The approved **Arbiter Focus** stage uses Qwen3-Reranker-0.6B to rerank a bounded retrieved set; it cannot find evidence omitted by candidate retrieval. Focus is designed to run independently with **Arbiter Context** off. Context Compact (Qwen3-1.7B) and Large (Qwen3-4B) add evidence assessment and bounded follow-up reads. Relevance scores do not prove evidence sufficiency.

Check the actual service's supported and qualified stages before enabling them; new stage controls are still being introduced in this source candidate. They require explicit stage/index consent and resource admission. Ordinary search remains available with explicit not-applied reasons when optional work is unavailable. Full provenance, publication identity, errors and any disclosed reordering or omissions remain part of the native result.
'''
change('packages/vscode/src/explorerHelp/search.md',lambda s:s.replace('related_airk_help: search, tool-selection','related_airk_help: search, tool-selection, models, arbiter')+search)
agent='''
## Questions and contextual evidence

The `answer` tool gathers a bounded cited evidence packet for a question; `context` expands neighboring indexed lines at a specific location. The assistant can use the packet's organized source-backed evidence, assessments, gaps and stop reason when composing its response. These are read-only operations within the connection's native scope.

The approved optional design has independent Arbiter Focus ranking plus Arbiter Context Compact/Large gathering. Focus can assist ordinary retrieval with Context off; neither stage grants an agent extra index permissions. Availability depends on actual implemented controls, verified profiles, explicit stage/index enrollment and current resources. Inspect native Arbiter Help and status; a selected or downloaded model does not establish active readiness or completed model qualification.
'''
change('packages/vscode/src/explorerHelp/assistance-tools.md',lambda s:s.replace('related_airk_help: explorer, roles, agent-setup, mcp, workflows','related_airk_help: explorer, roles, agent-setup, mcp, workflows, arbiter')+agent)
section='''## Retrieval, ranking and contextual questions

MiniLM remains the bundled embedding encoder for native candidate retrieval.
The approved source design adds independently enabled **Arbiter Focus**
(Qwen3-Reranker-0.6B), followed when enabled by **Arbiter Context — Compact**
(Qwen3-1.7B) or **Arbiter Context — Large** (Qwen3-4B). Focus can assist ordinary
retrieval while Context is off; Context guides bounded evidence gathering.
Controls and new profiles remain under implementation/qualification.

The `answer` MCP tool gathers contextual evidence for a question and retains
source receipts, gaps, work and stop reasons. It differs from `context`, which
expands lines around an indexed coordinate. Neither model advice nor stage
enrollment expands the client's authorized index scope. Inspect the running
catalog and native status for supported operations and active profiles.

The [Arbiter operator guide](arbiter/user-guide.md) explains stage/index consent,
explicit model provisioning, hardware admission, offline use, selected versus
active state and fallback. A reranker cannot recover evidence absent from the
candidate set, and a relevance score is not a sufficiency guarantee.

'''
change('docs/agent-integration.md',lambda s:s.replace('## Select scope and provision the actual client',section+'## Select scope and provision the actual client',1))
section='''## Optional Arbiter stages

The approved source design separates **Arbiter Focus** (independent
Qwen3-Reranker-0.6B ranking) from **Arbiter Context — Compact** (Qwen3-1.7B) and
**Arbiter Context — Large** (Qwen3-4B) evidence gathering. The new stage controls
and profiles remain under implementation and qualification; inspect actual
native support rather than inferring availability from a visible model name.

Review configuration and consent per stage and immutable index. Selection is
not active readiness: authenticated assets, supported tasks/platform/backend and
current resource admission are separate requirements. Ordinary search, status
and Help never approve downloads. Focus can run with Context off; turning both
off preserves ordinary retrieval. Missing or unavailable stages report their
reason and retain normal retrieval, including its existing completeness refusals.

Use the [Arbiter operator guide](arbiter/user-guide.md) for model roles, measured
hardware requirements, offline provisioning, drain/removal and original-request
recovery. Actual CLI/schema definitions remain authoritative for control inputs.

'''
change('docs/admin-pages.md',lambda s:s.replace('## What works in this version',section+'## What works in this version',1))
C['docs/arbiter/user-guide.md']=('',(P/'user-guide.md').read_text(encoding='utf-8'))
rows=[];diffs=[]
for name,(old,new) in C.items():
 dest=P/'files'/name;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_text(new,encoding='utf-8',newline='\n')
 rows.append({'path':name,'source_sha256':hashlib.sha256(Path(name).read_bytes()).hexdigest() if Path(name).exists() else None,'proposed_sha256':hashlib.sha256(dest.read_bytes()).hexdigest()})
 diffs.append(''.join(difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/'+name,tofile='b/'+name)))
(P/'manifest.json').write_text(json.dumps(rows,indent=2)+'\n',encoding='utf-8');(P/'combined.diff').write_text(''.join(diffs),encoding='utf-8')
print('Prepared',len(rows),'source/doc changes; no public mutation')
