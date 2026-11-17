# SPDX-License-Identifier: Apache-2.0
"""Read-only registry crate source proof using the shared archive reader."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import tarfile
import tomllib
import project_archives as shared

REGISTRY = 'registry+https://github.com/rust-lang/crates.io-index'


def package_binding(raw, spec):
    packages = tomllib.loads(raw.decode('utf-8'))['package']
    rows = [p for p in packages if p.get('name') == spec['name'] and p.get('version') == spec['version']]
    if len(rows) != 1 or rows[0].get('source') != REGISTRY or rows[0].get('checksum') != spec['archive']['sha256']:
        raise ValueError('unique registry package/version/checksum binding required')


def lock_bytes(root, spec):
    pin = spec['lock']
    if not re.fullmatch('[0-9a-f]{40}', pin['commit']) or not re.fullmatch('[0-9a-f]{40}', pin['blob_oid']) or pin['path'] != 'Cargo.lock':
        raise ValueError('exact Git Cargo.lock identity required')
    def git(*args):
        return subprocess.check_output(['git', *args], cwd=root, timeout=30, stderr=subprocess.PIPE)
    git('merge-base', '--is-ancestor', pin['commit'], 'HEAD')
    if git('rev-parse', pin['commit'] + ':Cargo.lock').decode().strip() != pin['blob_oid']:
        raise ValueError('Git lock blob differs')
    size = int(git('cat-file', '-s', pin['blob_oid']))
    if size != pin['bytes'] or size > shared.SOURCE_LIMIT:
        raise ValueError('bounded lock size differs')
    raw = git('cat-file', 'blob', pin['blob_oid'])
    if len(raw) != size or hashlib.sha256(raw).hexdigest() != pin['sha256']:
        raise ValueError('raw Git lock bytes differ')
    package_binding(raw, spec)
    return raw


def selected_member(source, spec):
    shared.safe_relative(source)
    root = shared.safe_relative(spec['root'])
    prefix = shared.safe_relative(spec['archive_prefix'].rstrip('/')) + '/'
    return prefix + source[len(root) + 1:] if source.startswith(root + '/') else None


def project(delta_path, catalog_path, root=shared.REPOSITORY):
    delta_raw, delta_pin = shared.consumed(delta_path, 256 << 20)
    catalog_raw, catalog_pin = shared.consumed(catalog_path)
    delta = shared.strict_json(delta_raw)
    catalog = shared.strict_json(catalog_raw)
    if catalog.get('schema') != 'airk.registry-crate-archives/1':
        raise ValueError('unsupported crate catalog')
    candidates = {}
    for lane in delta['lanes'].values():
        for row in lane:
            if row['path'] in candidates and candidates[row['path']] != row:
                raise ValueError('conflicting frozen source identity')
            candidates[row['path']] = row
    evidence = [dict(path=catalog_path.relative_to(root).as_posix(), **catalog_pin)]
    for path in (Path(__file__).resolve(), Path(shared.__file__).resolve()):
        evidence.append(dict(path=path.relative_to(root).as_posix(), **shared.measure(path)))
    result = dict(schema='airk.source-disposition/1', files=[], errors=[], archives=[], inputs=[dict(path=delta_path.relative_to(root).as_posix(), **delta_pin)], qualification=False, no_vendor_code_executed=True, no_files_created=True)
    selected = set()
    for spec in catalog['archives']:
        rows = [(row, selected_member(path, spec)) for path, row in candidates.items() if selected_member(path, spec) is not None]
        if not rows:
            continue
        if any(row['path'] in selected for row, _ in rows):
            raise ValueError('overlapping crate roots')
        selected.update(row['path'] for row, _ in rows)
        pending = []
        try:
            lock_bytes(root, spec)
            raw = shared.pinned_bytes(root, spec['archive'], shared.ARCHIVE_LIMIT)
            with tarfile.open(fileobj=io.BytesIO(raw), mode='r:*') as archive:
                roster = shared.archive_roster(archive, 'tar')
                for row, member in rows:
                    try:
                        if member not in roster:
                            raise ValueError('candidate absent from archive')
                        original = shared.member_digest(archive, 'tar', roster[member])
                        actual = shared.measure(shared.checked(root, row['path']))
                        if actual != dict(sha256=row['source_sha256'], bytes=row['bytes']):
                            raise ValueError('source differs from frozen inventory')
                        if actual != original:
                            raise ValueError('source differs from exact archive member')
                        pending.append(dict(source=row['path'], source_sha256=actual['sha256'], source_bytes=actual['bytes'], disposition='upstream_build_input', representation='raw', reason='Exact registry archive member, authenticated by reachable raw Git Cargo.lock package checksum; no source copied or executed.', evidence=[*evidence, spec['archive']], upstream=dict(repository='https://crates.io/crates/' + spec['name'], revision=spec['version'], path=member, source_sha256=actual['sha256']), git_lock=spec['lock']))
                    except (ValueError, OSError, KeyError) as error:
                        result['errors'].append(dict(source=row['path'], error=str(error)))
            shared.pinned(root, spec['archive'], shared.ARCHIVE_LIMIT)
            result['files'].extend(pending)
            result['archives'].append(dict(name=spec['name'], version=spec['version'], selected=len(rows), verified=len(pending), lock=spec['lock']))
        except (ValueError, OSError, KeyError, tarfile.TarError, subprocess.SubprocessError) as error:
            result['errors'].append(dict(archive=spec['archive']['path'], error=str(error)))
    result.update(selected_count=len(selected), verified_count=len(result['files']), complete_within_selected_scope=not result['errors'] and len(selected)==len(result['files']))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--delta', required=True, type=Path)
    parser.add_argument('--catalog', type=Path, default=Path(__file__).with_name('crate-catalog.json'))
    args = parser.parse_args()
    result = project(args.delta.resolve(), args.catalog.resolve())
    print(json.dumps(result, separators=(',', ':')))
    if result['errors']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
