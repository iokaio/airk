# SPDX-License-Identifier: Apache-2.0
"""Prepare existing exact-manifest integration owners; never integrate/commit."""
import ast
import hashlib
import json
from pathlib import Path

base = Path(__file__).resolve().parent
outputs = {}
for old_name, new_name in [
    ('p11b-integrate-reviewed.py', 'p12-integrate-reviewed.py'),
    ('p11b-commit-and-reanchor.py', 'p12-commit-and-reanchor.py'),
]:
    text = (base / old_name).read_text(encoding='utf-8')
    text = text.replace('p11b', 'p12').replace('P11b', 'P12')
    text = text.replace('f2bf2d219eda7096986565fcbc57f7ef9b1c5c7a',
                        'aa28425d2d3031b42f358e41d6ec366822635379')
    text = text.replace("len(qualification['steps'])==11", "len(qualification['steps'])==12")
    text = text.replace("neo['passing_tests']==11", "neo['passing_tests']==19")
    text = text.replace('Add reviewed file-aware Lean and Rich retained-profile migration',
                        'Add explicit reviewed reference refresh and exact receipt recovery')
    text = text.replace('52 focused native plus2 current resource cases;11 release/fault/performance/repository gates with84unchanged limits;UI746/19;Mac11/two cleaned targets;',
                        '39 focused native cases including18new;12 release/fault/performance/repository stages with84unchanged limits;UI776/19;Mac19/one cleaned target;')
    text = text.replace('P12 through P14, deleted-source follow-up',
                        'Conditional P13 decision, P14, deleted-source follow-up')
    ast.parse(text)
    output = base / new_name
    assert not output.exists(), 'Preserve previously prepared integration owner'
    output.write_text(text, encoding='utf-8', newline='\n')
    outputs[new_name] = hashlib.sha256(output.read_bytes()).hexdigest()
print(json.dumps({'prepared_only': True, 'files': outputs,
                  'requires': 'Passed12-stage Windows qualification; independently verified19-test Neo result and cargo clean; root combined review and final docs before integration/commit.'}))
