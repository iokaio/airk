# SPDX-License-Identifier: Apache-2.0
"""Close six independently retained native family receipts before human grading."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
QUERY = {'mode': 'vector', 'limit': 10, 'max_bytes': 32768, 'full_chunks': True}
FAMILIES = {
    'trained-v2-u8', 'trained-v2-s8',
    'original-shipped-u8-ort130-evaluation', 'original-shipped-s8-ort130-evaluation',
    'original-mixed-u8-ort130-evaluation', 'original-mixed-s8-ort130-evaluation',
}
LEGACY_CAPTURE = '5fb11c1a6d204bb46c6cc59cfecb5be1a7abe73efd882ef99772cc8fd2999b4b'
LEGACY_BINARY = '2016caf851b93e7e1ec4267e1fa7fb2a965a4d9e0511c355e5e6a1badcf7f72c'


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def valid_digest(value):
    return isinstance(value, str) and re.fullmatch('[0-9a-f]{64}', value) is not None


def scope_key(value):
    resolved = str(Path(value).resolve())
    if os.name == 'nt':
        if resolved.startswith('\\\\?\\UNC\\'):
            resolved = '\\\\' + resolved[8:]
        elif resolved.startswith('\\\\?\\'):
            resolved = resolved[4:]
    return os.path.normcase(resolved)


def validate_legacy(receipt, original_bytes):
    provenance = receipt.get('legacy_provenance')
    if not provenance:
        return
    if digest(original_bytes) != LEGACY_CAPTURE:
        raise ValueError('legacy original capture identity changed')
    original = json.loads(original_bytes)
    if receipt['family'] != 'trained-v2-u8' or provenance.get('kind') != 'retrospective-unchanged-published-source-binding' or provenance.get('original_capture_sha256') != LEGACY_CAPTURE or provenance.get('original_binary_sha256') != LEGACY_BINARY or receipt['binary_sha256'] != LEGACY_BINARY or provenance.get('original_nextest_run') != '659c966a-e2fb-4da2-a6fb-cf2635727176' or provenance.get('original_campaign_complete') is not False or provenance.get('original_family_records') != 205:
        raise ValueError('unapproved legacy binding provenance')
    if provenance.get('original_source_tree_prerun_seal', 'missing') is not None or provenance.get('original_dirty_source_preseal', 'missing') is not None or not valid_digest(provenance.get('binding_binary_sha256')) or 'retrospective binding' not in provenance.get('source_closure_timing', ''):
        raise ValueError('legacy proof invents original pre-run evidence')
    if original.get('complete') is not False or len(original['families']) != 1 or original['families'][0]['family'] != receipt['family'] or original['families'][0]['records'] != receipt['records']:
        raise ValueError('legacy original raw records changed')


def validate_published_rosters(receipts, manifest, fixture):
    for receipt in receipts:
        for collection in receipt['collections']:
            index_root = Path(scope_key(collection['source_root']))
            expected = {}
            for row in manifest['files']:
                path = Path(scope_key(fixture / row['file']))
                if path.is_relative_to(index_root):
                    expected[path.relative_to(index_root).as_posix()] = [row['bytes'], row['sha256']]
            actual_digest = digest(json.dumps(expected, sort_keys=True, ensure_ascii=False, separators=(',', ':')).encode())
            if not expected or collection['indexed_source_roster_sha256'] != actual_digest:
                raise ValueError('native published source roster differs from exact manifest scope')


def aggregate(receipts, questions, encoders, pins):
    """Validate native-owned data; retain every family field without normalization."""
    question_rows = questions['questions']
    question_map = {row['id']: row for row in question_rows}
    encoder_map = {row['family']: row for row in encoders}
    if len(question_rows) != 205 or len(question_map) != 205:
        raise ValueError('exact 205 unique frozen questions required')
    if len(encoders) != 6 or set(encoder_map) != FAMILIES:
        raise ValueError('exact six frozen encoder families required')
    if len(receipts) != 6 or {row.get('family') for row in receipts} != FAMILIES:
        raise ValueError('exact six distinct complete native family receipts required')
    for key in ('question_input_sha256', 'encoder_input_sha256', 'source_manifest_sha256', 'source_roster_sha256'):
        if not valid_digest(pins[key]):
            raise ValueError('invalid expected input/source digest: ' + key)
    if pins['source_roster_count'] != 2772:
        raise ValueError('exact approved 2772-file source closure required')
    for receipt in receipts:
        if receipt.get('schema') != 'airk.native-retrieval-family/2' or receipt.get('complete') is not True or receipt.get('evaluation') is not True or receipt.get('release_qualified') is not False:
            raise ValueError('incomplete or non-evaluation native family')
        for key, expected in pins.items():
            if receipt.get(key) != expected:
                raise ValueError('native input/source closure mismatch: ' + key)
        if receipt.get('query') != QUERY:
            raise ValueError('frozen native query changed')
        family = receipt['family']
        if receipt.get('model_json_sha256') != encoder_map[family]['model_json_sha256']:
            raise ValueError('native family model identity changed')
        if not valid_digest(receipt.get('binary_sha256')) or not receipt.get('runtime'):
            raise ValueError('native binary/runtime artifact binding missing')
        if receipt.get('source_before_sha256') != pins['source_roster_sha256'] or receipt.get('source_after_sha256') != pins['source_roster_sha256']:
            raise ValueError('native before/after closed source proof missing or changed')
        if not valid_digest(receipt.get('pipeline_digest')):
            raise ValueError('native pipeline identity missing')
        if not isinstance(receipt.get('collections'), list) or not receipt['collections']:
            raise ValueError('actual native collection bindings missing')
        collections = receipt['collections']
        scopes = {scope_key(row['source_root']): row for row in collections}
        wanted_scopes = {scope_key(row['source_root']) for row in question_rows}
        if len(scopes) != len(collections) or set(scopes) != wanted_scopes:
            raise ValueError('actual native collection source population changed')
        for collection in collections:
            if collection.get('pipeline_digest') != receipt['pipeline_digest'] or collection.get('model_json_sha256') != receipt['model_json_sha256'] or not valid_digest(collection.get('indexed_source_roster_sha256')) or type(collection.get('reused')) is not bool or not isinstance(collection.get('publication'), dict) or not collection['publication']:
                raise ValueError('published native collection binding missing or changed')
        rows = receipt.get('records', [])
        if len(rows) != 205 or {row['id'] for row in rows} != set(question_map):
            raise ValueError('native family question population incomplete or duplicated')
        for row in rows:
            # Resolve absolute scopes, without changing either retained spelling.
            if scope_key(row['source_root']) != scope_key(question_map[row['id']]['source_root']):
                raise ValueError('native question source scope changed')
            if not isinstance(row.get('primary_vector'), dict) or not isinstance(row.get('query_us'), int) or row['query_us'] < 0:
                raise ValueError('native primary result/timing missing')
            if row['primary_vector'].get('publication') != scopes[scope_key(row['source_root'])]['publication']:
                raise ValueError('native query publication differs from bound collection')
    return {'schema': 'airk.private-native-retrieval-aggregate/2', 'complete': True,
            'evaluation': True, 'release_qualified': False, **pins,
            'query': QUERY, 'families': sorted(receipts, key=lambda row: row['family'])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--family', type=Path, action='append', required=True)
    parser.add_argument('--questions', type=Path, required=True)
    parser.add_argument('--encoders', type=Path, required=True)
    parser.add_argument('--source-manifest', type=Path, required=True)
    parser.add_argument('--legacy-original', type=Path,
                        help='Required original retained capture if a native retrospective binder is included')
    parser.add_argument('--source-roster-sha256', required=True,
                        help='Exact native closed-tree roster digest, not an inferred Python serialization')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT / 'artifacts'):
        parser.error('fresh private output file required')
    paths = [Path(__file__), args.questions, args.encoders, args.source_manifest, *args.family]
    if args.legacy_original:
        paths.append(args.legacy_original)
    if len(set(path.resolve() for path in args.family)) != 6:
        parser.error('six distinct retained receipt files required')
    retained = {path: path.read_bytes() for path in paths}
    pins = {'question_input_sha256': digest(retained[args.questions]),
            'encoder_input_sha256': digest(retained[args.encoders]),
            'source_manifest_sha256': digest(retained[args.source_manifest]),
            'source_roster_sha256': args.source_roster_sha256, 'source_roster_count': 2772}
    manifest = json.loads(retained[args.source_manifest])
    # Exact SourceRow declaration order from the native-emitted source closure,
    # deliberately distinct from the historical Python sorted-key audit digest.
    roster = [{key: row[key] for key in ('file', 'bytes', 'sha256')} for row in sorted(manifest['files'], key=lambda row: row['file'])]
    if len(roster) != 2772 or len({row['file'] for row in roster}) != 2772 or digest(json.dumps(roster, ensure_ascii=False, separators=(',', ':')).encode()) != pins['source_roster_sha256']:
        raise ValueError('expected native source roster digest differs from approved manifest')
    if pins['question_input_sha256'] != '2e74a873125834e17455c40879c0ba161626053fbd39742bcbbc73077d3dd046' or pins['encoder_input_sha256'] != '3ead240726ebdba10577db6778bffbf5232eab10c50f2ae128d589e0a961ab81' or pins['source_manifest_sha256'] != '2627e54241d8e961cc03c005c02f1655be364462ada16bbafdc0434cf4192a60':
        raise ValueError('unapproved frozen native campaign inputs')
    receipts = [json.loads(retained[path]) for path in args.family]
    for receipt in receipts:
        if receipt.get('legacy_provenance'):
            if not args.legacy_original:
                raise ValueError('retained original artifact required for retrospective binding')
            validate_legacy(receipt, retained[args.legacy_original])
    result = aggregate(receipts,
                       json.loads(retained[args.questions]), json.loads(retained[args.encoders]), pins)
    validate_published_rosters(receipts, manifest, args.source_manifest.resolve().parent)
    result.update({'aggregated_at_utc': datetime.now(timezone.utc).isoformat(),
                   'input_seals': {str(path.resolve()): digest(raw) for path, raw in retained.items()},
                   'family_artifacts': [{'path': str(path.resolve()), 'bytes': len(retained[path]),
                                         'sha256': digest(retained[path])} for path in args.family],
                   'scope': 'Mechanical complete-six-family closure only. Raw family fields preserved; no human scores or release qualification. Legacy provenance, if present, stays explicit.'})
    if any(path.read_bytes() != raw for path, raw in retained.items()):
        raise ValueError('aggregate runner/input drift')
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
    output.with_suffix('.runner.py').write_bytes(retained[Path(__file__)])
    print(json.dumps({'complete': True, 'families': 6, 'questions_each': 205,
                      'output_sha256': digest(output.read_bytes())}))


if __name__ == '__main__':
    main()
