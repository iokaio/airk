# SPDX-License-Identifier: Apache-2.0
"""Mechanical private artifact closure, never a composed-proof acceptance oracle.

Root supplies the exact reviewed receipt-shaped mapping after actual execution.
Every artifact reference has only path (or path+existing digest); this assembler
authenticates full bytes and retains that mapping. Native promotion must parse
and reconcile all original evidence, ignoring these incomplete verdicts.
"""
import hashlib
import json
from pathlib import Path
import sys
from evidence_checks import REQUIRED_CASES

def close_references(value):
    if isinstance(value, dict):
        if set(value).issubset({'path', 'sha256'}) and 'path' in value:
            path = Path(value['path'])
            if not path.is_absolute() or not path.is_file():
                raise ValueError('actual absolute artifact required: ' + str(path))
            sha = hashlib.sha256()
            with path.open('rb') as stream:
                for block in iter(lambda: stream.read(1024 * 1024), b''):
                    sha.update(block)
            digest = sha.hexdigest()
            if value.get('sha256', digest) != digest:
                raise ValueError('original artifact identity changed: ' + str(path))
            return {'path': str(path), 'sha256': digest}
        return {key: close_references(item) for key, item in value.items()}
    if isinstance(value, list):
        return [close_references(item) for item in value]
    return value

def assemble(mapping, output):
    raw = Path(mapping).read_bytes()
    receipt = close_references(json.loads(raw))
    if receipt['schema'] != 'airk.private.actual-composed-service/1' or receipt['proof_class'] != 'ActualComposed':
        raise ValueError('wrong original mapping schema')
    if set(receipt['cases']) != set(REQUIRED_CASES):
        raise ValueError('exact native required case roster absent')
    for case in receipt['cases'].values():
        case['verdict'] = 'incomplete'
        case['reason'] = 'Original observations retained; native original-artifact reconciliation pending.'
    receipt['state'] = 'observed_pending_native_reconciliation'
    receipt['qualification_granted'] = False
    receipt['original_mapping_sha256'] = hashlib.sha256(raw).hexdigest()
    with Path(output).open('x', encoding='utf-8') as stream:
        stream.write(json.dumps(receipt, ensure_ascii=False, indent=2, allow_nan=False) + '\n')

if __name__ == '__main__':
    assemble(*sys.argv[1:])
