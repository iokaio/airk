# SPDX-License-Identifier: Apache-2.0
"""Read-only private fixture role projection; stdout metadata only."""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import stat

ROOT = Path(__file__).resolve().parents[4]
LIMIT = 16 << 20
METADATA_LIMIT = 256 << 20
GAP = 'No exact historical hash witness; source role established by pinned copy chain'
FAMILIES = ('detail-real-final', 'enhancements-qualified-final', 'enhancements-real-final')
COPY_ROOTS = tuple('artifacts/' + name + '/copies/bookbuilder' for name in FAMILIES)
INDEX_ROOT = 'artifacts/detail-real-final/state/essential/collections/comparison-bookbuilder-essential/generations/8c503990cf69fb4840a0f2a0057f7e60db75807967d2992d3e5a4b080200e7f6/'


def canonical(value):
    return (json.dumps(value, sort_keys=True, ensure_ascii=True, separators=(',', ':')) + '\n').encode()


def relative(value):
    if not isinstance(value, str) or not value or '\\' in value or ':' in value or value.startswith('/') or any(x in ('', '.', '..') for x in value.split('/')):
        raise ValueError('noncanonical relative path')
    return value


def checked(path):
    path = Path(path)
    if not path.is_absolute():
        raise ValueError('absolute input required')
    for parent in (path, *path.parents):
        info = parent.lstat()
        if stat.S_ISLNK(info.st_mode) or getattr(info, 'st_file_attributes', 0) & 0x400:
            raise ValueError('aliased input refused')
    return path


def stamp(info):
    return info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns


def capture(path, limit=LIMIT):
    path = checked(path)
    before = path.stat()
    if not stat.S_ISREG(before.st_mode) or before.st_size > limit:
        raise ValueError('bounded regular input required')
    with path.open('rb') as stream:
        if stamp(os.fstat(stream.fileno())) != stamp(before):
            raise ValueError('input replaced before capture')
        raw = stream.read(limit + 1)
        if stamp(os.fstat(stream.fileno())) != stamp(before):
            raise ValueError('input changed while captured')
    if len(raw) != before.st_size or stamp(path.stat()) != stamp(before):
        raise ValueError('input changed after capture')
    return raw


def pin_bytes(path, raw):
    return dict(path=path, sha256=hashlib.sha256(raw).hexdigest(), bytes=len(raw))


def pairs(items):
    result = {}
    for key, value in items:
        if key in result:
            raise ValueError('duplicate JSON key')
        result[key] = value
    return result


def parse(raw):
    return json.loads(raw, object_pairs_hook=pairs)


def normalized(path):
    return str(path).removeprefix('\\\\?\\').replace('\\', '/').rstrip('/').casefold()


def validate_external_root(value, repository):
    external = Path(value)
    current = normalized(repository)
    if not external.is_absolute() or normalized(external) == current or normalized(external).startswith(current + '/'):
        raise ValueError('first workspace must identify external original')
    return external


def workspace(config):
    rows = [row for row in config['workspaces'] if row['label'] == 'bookbuilder']
    if len(rows) != 1:
        raise ValueError('exact bookbuilder workspace required')
    return rows[0]


def validate_summary(summary):
    rows = [row for row in summary['runs'] if row['workspace'] == 'bookbuilder']
    if summary.get('original_workspaces_unchanged') is not True or len(rows) != 3 or {r['level'] for r in rows} != {'essential', 'balanced', 'detailed'}:
        raise ValueError('incomplete source-preservation summary')
    if any(r.get('isolation_and_citations_passed') is not True or 'one_file_edit_in_isolated_copy' not in r for r in rows):
        raise ValueError('copy/isolation evidence absent')


def original_observation(path, expected):
    try:
        actual = pin_bytes(str(path), capture(path))
        actual['status'] = 'matches' if all(actual[k] == expected[k] for k in ('sha256', 'bytes')) else 'differs'
        return actual
    except FileNotFoundError as error:
        return dict(path=str(path), status='missing', sha256=None, bytes=None, error=type(error).__name__)
    except (OSError, ValueError) as error:
        return dict(path=str(path), status='unavailable', sha256=None, bytes=None, error=type(error).__name__)


def historical_witness(inventory, manifest, inventory_pin, manifest_pin, rel, source, external):
    if normalized(inventory['root']) != normalized(external) or manifest['inventory_digest'] != inventory_pin['sha256']:
        raise ValueError('historical inventory identity differs')
    member = inventory['files'].get(rel)
    if member is None or member['state']['source_hash'] != source['sha256'] or member['source_bytes'] != source['bytes']:
        return None
    return dict(inventory=inventory_pin, manifest=manifest_pin, relative_path=rel, source_sha256=source['sha256'], source_bytes=source['bytes'])


def allow_gap(rel, observation, witness):
    if witness is not None or observation['status'] == 'matches':
        return False
    if not re.search(r'(^|/)obj/[^/]+/[^/]+/[^/]+\.AssemblyInfo\.cs$', rel):
        raise ValueError('unapproved historical identity gap')
    return True


def project(catalog, destination):
    relative(destination)
    if not destination.startswith('artifacts/'):
        raise ValueError('private destination required')
    catalog_path = Path(__file__).with_name('catalog.json')
    catalog_raw = capture(catalog_path)
    if parse(catalog_raw) != catalog or catalog.get('schema') != 'airk.private-fixture-projection-inputs/1':
        raise ValueError('catalog source identity differs')
    owner_path = Path(__file__).resolve()
    evidence = [pin_bytes(catalog_path.relative_to(ROOT).as_posix(), catalog_raw), pin_bytes(owner_path.relative_to(ROOT).as_posix(), capture(owner_path))]
    def load(row, limit=LIMIT):
        path = ROOT / relative(row['path'])
        raw = capture(path, limit)
        pin = pin_bytes(row['path'], raw)
        if pin != row:
            raise ValueError('evidence differs: ' + row['path'])
        evidence.append(pin)
        return raw
    delta = parse(load(catalog['delta'], METADATA_LIMIT))
    producer = catalog['producer']; load(producer)
    inputs = [parse(load(p)) for p in catalog['inputs']]
    summaries = [parse(load(p)) for p in catalog['summaries']]
    for summary in summaries:
        validate_summary(summary)
    first = workspace(inputs[0]); later = workspace(inputs[1])
    external = validate_external_root(first['root'], ROOT)
    if first.get('copied_tree', False):
        raise ValueError('first workspace must identify external original')
    if later.get('copied_tree') is not True or normalized(later['root']) != normalized(ROOT / COPY_ROOTS[0]):
        raise ValueError('later copy chain differs')
    inventory = parse(load(catalog['inventory']))
    manifest = parse(load(catalog['manifest']))
    rows = {}
    for lane in delta['lanes'].values():
        for row in lane:
            if any(row['path'].startswith(root + '/') for root in COPY_ROOTS):
                relative(row['path'])
                if row['path'] in rows and rows[row['path']] != row:
                    raise ValueError('conflicting source observations')
                rows[row['path']] = row
    if any(sum(p.startswith(root + '/') for p in rows) != 3669 for root in COPY_ROOTS):
        raise ValueError('frozen fixture roster differs')
    base = {}; results = []; observed_sources = []; gaps = set(); counts = Counter()
    for index, copy_root in enumerate(COPY_ROOTS):
        for path, row in sorted(rows.items()):
            if not path.startswith(copy_root + '/'):
                continue
            rel = relative(path[len(copy_root) + 1:])
            source = pin_bytes(path, capture(ROOT / path))
            if source['sha256'] != row['source_sha256'] or source['bytes'] != row['bytes']:
                raise ValueError('current source differs from inventory: ' + path)
            observed_sources.append(source)
            if index == 0:
                original = original_observation(external / rel, source)
                witness = historical_witness(inventory, manifest, catalog['inventory'], catalog['manifest'], rel, source, external)
                gap = allow_gap(rel, original, witness)
                base[rel] = (source, original, witness, gap)
                counts[original['status']] += 1
                if gap:
                    gaps.add(rel)
            else:
                if rel not in base or any(base[rel][0][k] != source[k] for k in ('sha256', 'bytes')):
                    raise ValueError('later copy differs from first copy')
                _, ultimate, witness, gap = base[rel]
                original = dict(base[rel][0], path=str(ROOT / COPY_ROOTS[0] / rel), status='matches')
            fixture = dict(producer=producer, input=catalog['inputs'][0 if index == 0 else 1], summary=catalog['summaries'][index], copy_root=copy_root, copy_relative_path=rel, label='bookbuilder', original=original)
            if index:
                fixture['upstream_fixture'] = dict(input=catalog['inputs'][0], summary=catalog['summaries'][0], copy_root=COPY_ROOTS[0], original=ultimate)
            if witness:
                fixture['historical_index'] = witness
            if gap:
                fixture['identity_gap'] = GAP
            results.append(dict(source=path, source_sha256=source['sha256'], source_bytes=source['bytes'], disposition='private_fixture_source', reason='Private indexed input copy established by pinned producer/configuration/completed isolation summaries; identity observations remain explicit.', evidence=evidence, fixture=fixture))
    if len(gaps) != 18:
        raise ValueError('reviewed 18-file role-only gap roster changed')
    for pin in [*evidence, *observed_sources]:
        actual = pin_bytes(pin['path'], capture(ROOT / pin['path'], METADATA_LIMIT if pin == catalog['delta'] else LIMIT))
        if actual != pin:
            raise ValueError('input drift before publication')
    proof = dict(schema='airk.private.fixture-projection/1', selected=len(results), original_status=dict(counts), role_only_first_copy_count=len(gaps), role_only_all_copies_count=sum('identity_gap' in r['fixture'] for r in results), qualification=False, whole_directory_exclusion=False)
    bundles = []
    for offset in range(0, len(results), 500):
        group = 'fixtures-' + str(offset // 500 + 1).zfill(3)
        outputs = {'projection.json': canonical(proof).decode(), 'source-disposition.json': canonical(dict(schema='airk.source-disposition/1', files=results[offset:offset + 500])).decode()}
        if any(len(v.encode()) > LIMIT for v in outputs.values()):
            raise ValueError('metadata output exceeds bound')
        bundles.append(dict(schema='airk.private.projection-bundle/1', destination=destination, group=group, files=outputs))
    return bundles


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--destination', required=True)
    args = parser.parse_args()
    catalog = parse(capture(Path(__file__).with_name('catalog.json')))
    for bundle in project(catalog, args.destination):
        print(json.dumps(bundle, ensure_ascii=True, separators=(',', ':')))


if __name__ == '__main__':
    main()
