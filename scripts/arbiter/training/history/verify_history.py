# SPDX-License-Identifier: Apache-2.0
"""Verify historical source identities without importing or executing recipes."""
import argparse
import ast
import hashlib
import json
from pathlib import Path

ROOT = next(p for p in Path(__file__).resolve().parents if (p / 'Cargo.toml').is_file() and (p / 'AGENTS.md').is_file())
MAPS = (ROOT / 'scripts/arbiter/training/history/source-map.json', ROOT / 'scripts/arbiter/export/history/source-map.json')


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def verify(originals=False):
    rows = [row for path in MAPS for row in json.loads(path.read_bytes())['files']]
    exclusions = json.loads((MAPS[0].parent / 'excluded-inputs.json').read_bytes())['files']
    if len(rows) != 189 or len({r['source'] for r in rows}) != 189 or len({r['original_sha256'] for r in rows}) != 135:
        raise ValueError('assigned authored source roster differs')
    if len(exclusions) != 18 or any(r['disposition'] != 'excluded_vendor_corpus_input' for r in exclusions):
        raise ValueError('vendor/corpus exclusion roster differs')
    owners = {}
    parsed = set()
    for row in rows:
        destination = (ROOT / row['destination']).resolve()
        if not destination.is_relative_to(ROOT / 'scripts'):
            raise ValueError('maintained source escaped scripts')
        raw = destination.read_bytes()
        if (len(raw), digest(raw)) != (row['maintained_bytes'], row['maintained_sha256']):
            raise ValueError('maintained source changed: ' + row['destination'])
        prefix = row['spdx_prefix'].encode()
        if prefix not in (b'', b'# SPDX-License-Identifier: Apache-2.0\n', b'# SPDX-License-Identifier: Apache-2.0\r\n') or not raw.startswith(prefix):
            raise ValueError('unapproved body transformation')
        original = raw[len(prefix):]
        if (len(original), digest(original)) != (row['original_bytes'], row['original_sha256']) or row['body_changes'] is not False:
            raise ValueError('original source identity differs: ' + row['source'])
        previous = owners.setdefault(row['original_sha256'], row['destination'])
        if previous != row['destination']:
            raise ValueError('identical bodies have multiple canonical owners')
        if row['fresh_execution_supported'] is not False:
            raise ValueError('source preservation cannot claim fresh execution')
        if originals and (ROOT / row['source']).read_bytes() != original:
            raise ValueError('retained artifact original differs')
        if destination.suffix == '.py' and destination not in parsed:
            ast.parse(raw, filename=str(destination))
            compile(raw, str(destination), 'exec')
            parsed.add(destination)
    if originals:
        for row in exclusions:
            raw = (ROOT / row['source']).read_bytes()
            if (len(raw), digest(raw)) != (row['bytes'], row['sha256']):
                raise ValueError('excluded corpus evidence changed')
    return {'authored_paths': len(rows), 'distinct_originals': len(owners), 'python_parsed_not_executed': len(parsed), 'excluded_vendor_corpus_paths': len(exclusions), 'originals_compared': originals, 'models_executed': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--originals', action='store_true', help='also compare provisioned artifact originals; read-only')
    args = parser.parse_args()
    print(json.dumps(verify(args.originals), sort_keys=True))


if __name__ == '__main__':
    main()
