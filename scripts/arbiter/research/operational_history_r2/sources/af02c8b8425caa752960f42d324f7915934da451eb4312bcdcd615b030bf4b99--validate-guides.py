# SPDX-License-Identifier: Apache-2.0
import json, re
from pathlib import Path
import jsonschema

root = Path.cwd()
cli = (root / 'docs/cli-developer-guide.md').read_text()
reference = (root / 'docs/cli-reference.md').read_text()
names = set(re.findall(r'^## (airk.*)$', reference, re.M))
documented = set(re.findall(r'^## (airk.*)$', cli, re.M))
assert documented == names | {'airk tools help'}, (names-documented, documented-names)
mcp = (root / 'docs/mcp-developer-guide.md').read_text()
inventory = json.loads((root / 'artifacts/performance-1.8.2-20261101/mcp-doc-inventory.json').read_text())
assert len(inventory) == 28
assert all(f'## {r["name"]}\n' in mcp for r in inventory)
native = json.loads((root / 'artifacts/performance-1.8.2-20261101/native-tool-schemas.json').read_text(encoding='utf-8-sig'))['tools']
schemas = {x['name'].removeprefix('airk_'): x['inputSchema'] for x in native}
legacy = json.loads((root / 'crates/airk-mcp/src/unified_explorer.json').read_text())
for x in legacy:
    schemas[x['name'].removeprefix('airk_explorer_')] = x['inputSchema']
for public, old in [('list_templates','templates_list'),('get_template','template_get'),('draft_search','suggest_search'),('draft_strategy','suggest_strategy'),('draft_plan','suggest_plan'),('get_handoff','handoff_get')]:
    schemas[public] = schemas[old]
schemas['projects'] = dict(type='object', additionalProperties=False, properties={})
schemas['suggest_view'] = dict(type='object', required=['kind','draft'], properties={'kind':{'enum':['navigation','review','management']},'draft':{'type':'object'}})
for row in inventory:
    schema = schemas[row['name']]
    example = dict(row['example'])
    # Placeholder IDs in the prose deliberately are not fabricated capabilities.
    # Substitute syntactic fixtures only in this offline schema check.
    if 'proposal_id' in example:
        example['proposal_id'] = 'ex-prop-' + '0'*16
    if row['name'] == 'get_handoff':
        example['token'] = 'ex-share-' + '0'*64
    jsonschema.validate(example, schema)
    if row['name'] == 'suggest_view':
        jsonschema.validate(row['example']['draft'], schemas['suggest_'+row['example']['kind']])
    if row['name'] == 'validate_draft':
        kind = row['example']['kind']
        jsonschema.validate(row['example']['draft'], schemas['draft_'+kind])
print(f'Coverage: {len(names)} reference sections plus tools help; 28 MCP examples conform to corresponding bundled schemas.')

for p in [root / 'docs/cli-developer-guide.md', root / 'docs/mcp-developer-guide.md']:
    for target in re.findall(r'\[[^\]]*\]\(([^)]+)\)',p.read_text()):
        if '://' in target: continue
        path, _, fragment = target.partition('#')
        q = (p.parent / path).resolve() if path else p
        assert q.exists(), (p,target)
        if fragment and q.suffix == '.md':
            heads = re.findall(r'^#+ (.*)$', q.read_text(), re.M)
            anchors = {re.sub(r'[^\w\- ]','',x.lower()).replace(' ','-') for x in heads}
            assert fragment in anchors, (p,target)
print('Local file links and heading anchors resolve.')
