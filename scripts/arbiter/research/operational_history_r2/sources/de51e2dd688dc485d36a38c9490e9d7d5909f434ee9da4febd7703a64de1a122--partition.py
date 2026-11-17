# SPDX-License-Identifier: Apache-2.0
import json, subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[2]; out=Path(__file__).resolve().parent
paths=subprocess.check_output(['git','diff','--name-only','origin/main...HEAD'],cwd=root,text=True).splitlines()
groups={name:[] for name in ['cli_catalog','cli_runtime','store','kernel','contracts_ingest','extension','tooling','docs']}
for p in paths:
    if p.startswith('crates/airk-cli/'):
        name='cli_catalog' if any(s in p for s in ['catalog','combined_frontend','combined_reads','reference_refresh','project_worker_scope','project_worker_split','project_worker_source']) else 'cli_runtime'
    elif p.startswith('crates/airk-store/'):name='store'
    elif p.startswith(('crates/airk-kernel/','crates/airk-retrieval/')):name='kernel'
    elif p.startswith('crates/'):name='contracts_ingest'
    elif p.startswith('packages/vscode/'):name='extension'
    elif p.startswith('docs/') or p in ['README.md','CONTRIBUTING.md','AGENTS.md','CLAUDE.md']:name='docs'
    else:name='tooling'
    groups[name].append(p)
for name,files in groups.items():
    (out/(name+'.files')).write_text('\n'.join(files)+'\n',encoding='utf-8')
    subprocess.run(['git','diff','--binary','origin/main...HEAD','--output='+str(out/(name+'.diff')),'--',*files],cwd=root,check=True)
print(json.dumps({k:len(v) for k,v in groups.items()}))
