# SPDX-License-Identifier: Apache-2.0
"""Read-only source-content review aids; evidence is not an authorship verdict."""
import ast
from collections import Counter,defaultdict
import hashlib
import json
from pathlib import Path
import re

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]

def main():
    rows=json.loads((HERE/'source-map.json').read_bytes())['files']; texts={};facts={};siblings=defaultdict(list)
    for row in rows:
        path=row['source'];raw=(ROOT/path).read_bytes()
        if hashlib.sha256(raw).hexdigest()!=row['original_sha256']:raise ValueError('source changed')
        text=raw.decode('utf-8-sig',errors='strict');texts[path]=text;siblings[str(Path(path).parent)].append(path)
        refs=[]
        for number,line in enumerate(text.splitlines(),1):
            if re.search(r'(?i)(\bairk\b|artifacts[/\\]|(?:scripts|packages|crates|tools)/[a-z])',line):refs.append({'line':number,'excerpt':line[:240]})
        doc=None;imports=[]
        if path.endswith('.py'):
            tree=ast.parse(raw);doc=ast.get_docstring(tree)
            for node in ast.walk(tree):
                if isinstance(node,ast.Import):imports.extend(n.name for n in node.names)
                if isinstance(node,ast.ImportFrom) and node.module:imports.append(node.module)
        facts[path]={'source':path,'source_sha256':row['original_sha256'],'role':row['role'],'direct_repository_references':refs[:6],'module_docstring':doc[:400] if doc else None,'imports':sorted(set(imports)),'sibling_callers':[]}
    for path,fact in facts.items():
        if fact['direct_repository_references']:continue
        stem=Path(path).stem;name=Path(path).name
        for sibling in siblings[str(Path(path).parent)]:
            if sibling==path:continue
            caller=facts[sibling]
            if caller['direct_repository_references'] and (stem in caller['imports'] or name in texts[sibling]):fact['sibling_callers'].append(sibling)
    weak=[f for f in facts.values() if not f['direct_repository_references'] and not f['sibling_callers']]
    output={'schema':'airk.source-content-review-aid/1','status':'content/caller evidence only; no automated authored/qualification verdict','origin_count':len(rows),'families':dict(Counter('/'.join(r['source'].split('/')[:2]) for r in rows)),'direct_reference_origins':sum(bool(f['direct_repository_references']) for f in facts.values()),'sibling_linked_origins':sum(bool(f['sibling_callers']) for f in facts.values()),'remaining_manual_review':weak,'files':list(facts.values())}
    print(json.dumps(output,indent=2))
if __name__=='__main__':main()
