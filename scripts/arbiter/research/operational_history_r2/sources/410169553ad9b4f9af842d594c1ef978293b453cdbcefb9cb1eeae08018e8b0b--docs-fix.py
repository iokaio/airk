# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
root=Path(__file__).resolve().parents[2]
for name in ['docs/architecture.md','docs/research/implementation-v190.md','docs/releases/release-1.8.8.md','docs/releases/release-1.9.0.md']:
    p=root/name;s=p.read_text(encoding='utf-8')
    s=s.replace('Current native source `237f8969...`','Checkpoint native source `237f8969...`')
    if name.endswith('implementation-v190.md'):
        s=s.replace('or gain is claimed. Current source `237f8969...`','or gain is claimed. Checkpoint source `237f8969...`')
        s=s.replace('Deleted-source recovery and the local test installer remain unfinished. Version\n**1.8.8** is unpublished.','At this retained checkpoint, deleted-source recovery and the local test installer\nwere unfinished. Later selected recovery and package results appear at the end\nof this ledger. Version **1.8.8** remains unpublished.')
    elif name.endswith('release-1.9.0.md'):
        s=s.replace('P14 now evaluates source `237f8969...`','This retained P14 checkpoint evaluated source `237f8969...`')
        s=s.replace('the newer source. Deleted-source recovery and the local\ntest installer remain unfinished; no promotion or publication is implied.','the newer source. At this checkpoint, deleted-source recovery and the local\ntest installer were unfinished; later results are in the working-build record.\nNo promotion or publication is implied.')
    p.write_text(s,encoding='utf-8',newline='\n')
p=root/'docs/ci-and-builds.md';s=p.read_text(encoding='utf-8');assert 'exact unified 28-tool MCP catalog' in s
p.write_text(s.replace('exact unified 28-tool MCP catalog','exact unified 29-tool MCP catalog'),encoding='utf-8',newline='\n')
