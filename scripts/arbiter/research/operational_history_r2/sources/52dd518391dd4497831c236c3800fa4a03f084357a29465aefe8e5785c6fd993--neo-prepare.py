# SPDX-License-Identifier: Apache-2.0
"""Freeze PUBLIC tracked source plus explicitly named public new inputs; no SSH/Cargo.

Primary reviews the scripts and announces freeze before calling this helper.
Usage: python neo-prepare.py --label readiness-1817-r1 --native-sha256 <seal>
       --native-files <count> [--extra-input crates/...]
Outputs are new immutable campaign artifacts; previous evidence is never replaced.
"""
import argparse
import ast
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
import tarfile

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[1]
EXCLUDED = {'.git', '.codex', '.agents', '.aws', '.mcp.json', '.ssh', 'temp', 'private',
            'index', 'indexes', 'state', 'airk-state', 'artifacts', 'target', 'node_modules', '__pycache__'}


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def plain(name):
    relative = PurePosixPath(name)
    if relative.is_absolute() or '..' in relative.parts or set(relative.parts) & EXCLUDED or ':' in name or '\\' in name:
        raise ValueError('Non-public or escaping input: ' + name)
    path = ROOT.joinpath(*relative.parts)
    if not path.is_file() or path.resolve(strict=True) != path:
        raise ValueError('Missing, linked or redirected input: ' + name)
    for parent in path.parents:
        if parent == ROOT:
            break
        if parent.is_symlink() or (hasattr(parent, 'is_junction') and parent.is_junction()):
            raise ValueError('Aliased input ancestor: ' + name)
    return path


def save(path, value):
    with path.open('x', encoding='utf-8') as output:
        json.dump(value, output, indent=2)
        output.write('\n')


def main():
    if sys.flags.optimize:
        raise RuntimeError('Preparation requires Python assertions enabled')
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--label', required=True)
    parser.add_argument('--native-sha256', required=True)
    parser.add_argument('--native-files', required=True, type=int)
    parser.add_argument('--extra-input', action='append', default=[])
    args = parser.parse_args()
    assert re.fullmatch(r'readiness-1817-[a-z0-9-]{1,40}', args.label)
    assert re.fullmatch(r'[a-f0-9]{64}', args.native_sha256) and args.native_files > 0
    expected = {'sha256': args.native_sha256, 'files': args.native_files}
    spec = importlib.util.spec_from_file_location('readiness_native_source_owner', ROOT / 'scripts/build-service-runtime.py')
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    # Observe the canonical owner's Git response rather than copy its input
    # catalog. The exact response provides the native roster for export proof.
    native_responses = []
    original_check_output = builder.subprocess.check_output
    def observe_native_git(command, *positional, **keywords):
        response = original_check_output(command, *positional, **keywords)
        if isinstance(command, list) and 'ls-files' in command:
            native_responses.append(response)
        return response
    builder.subprocess.check_output = observe_native_git
    try:
        observed_native = builder.source_identity()
    finally:
        builder.subprocess.check_output = original_check_output
    assert observed_native == expected, 'Primary native source freeze differs'
    assert len(native_responses) == 1
    native_names = sorted({name for name in native_responses[0].split(b'\0') if name})
    assert len(native_names) == expected['files']
    git = ['git', '-c', 'safe.directory=' + ROOT.as_posix()]
    tracked = subprocess.check_output([*git, 'ls-files', '-z', '--cached'], cwd=ROOT).split(b'\0')
    names = {os.fsdecode(name).replace('\\', '/') for name in tracked if name and not set(PurePosixPath(os.fsdecode(name).replace('\\', '/')).parts) & EXCLUDED}
    for name in args.extra_input:
        assert PurePosixPath(name).parts[0] in {'crates', 'fixtures', 'scripts', 'third_party', '.cargo', 'packages'}
        plain(name)
        names.add(name)
    assert {'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml'} <= names
    # No implicit export of ignored files or artifacts. New source dependencies
    # must be explicitly passed and the canonical native seal must still match.
    rows = {name: {'sha256': digest(plain(name)), 'bytes': plain(name).stat().st_size} for name in sorted(names)}
    native_inputs = {}
    exported_identity = hashlib.sha256()
    for raw in native_names:
        name = os.fsdecode(raw).replace('\\', '/')
        assert name in rows, 'Canonical native input omitted from public export; explicitly include/review before transfer: ' + name
        native_inputs[name] = rows[name]['sha256']
        exported_identity.update(len(raw).to_bytes(8, 'big'))
        exported_identity.update(raw)
        exported_identity.update(bytes.fromhex(rows[name]['sha256']))
    assert {'sha256': exported_identity.hexdigest(), 'files': len(native_inputs)} == expected, 'Exported canonical native roster differs'
    # All tracked test/fixture inputs are public repository inputs for this
    # batch. An exclusion matching a fixture needs explicit integrator review;
    # do not discover an omitted include_str!/include_bytes! at remote compile.
    omitted_fixtures = [os.fsdecode(raw).replace('\\', '/') for raw in tracked if raw
                        and ('fixtures' in PurePosixPath(os.fsdecode(raw).replace('\\', '/')).parts
                             or PurePosixPath(os.fsdecode(raw).replace('\\', '/')).parts[0] == 'fixtures')
                        and os.fsdecode(raw).replace('\\', '/') not in names]
    assert not omitted_fixtures, 'Tracked test fixture excluded; review exact roster before transfer: ' + repr(omitted_fixtures)
    plan = json.loads((BASE / 'neo-plan.json').read_text())
    assert plan['status'] == 'PREPARED_FOR_PRIMARY_REVIEW_NOT_LAUNCHED'
    for row in plan['steps']:
        for identity in row.get('tests', []):
            assert re.fullmatch(r'[a-zA-Z0-9_:]+', identity)
    for filename in ('neo-runner.py', 'neo-owner.py', 'neo-dependency-fetch.py'):
        ast.parse((BASE / filename).read_text())
    prefix = BASE / ('neo-' + args.label)
    archive = prefix.with_name(prefix.name + '-source.tar.gz')
    with tarfile.open(archive, 'x:gz') as output:
        for name in sorted(rows):
            output.add(plain(name), arcname=name, recursive=False)
    assert builder.source_identity() == expected
    assert all(digest(plain(name)) == row['sha256'] for name, row in rows.items()), 'Consumed input changed during freeze'
    plan_path = prefix.with_name(prefix.name + '-plan.json')
    save(plan_path, plan)
    snapshot = {'schema': 'airk.readiness.neo-snapshot/1', 'label': args.label,
                'entry_commit': subprocess.check_output([*git, 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                'source_state': 'Frozen live source; entry commit is metadata, not a clean-tree claim',
                'native_source': expected, 'native_inputs': native_inputs,
                'public_export_preflight': {'canonical_native_roster_equal': True, 'tracked_fixture_omissions': []},
                'files': rows, 'archive_sha256': digest(archive),
                'plan_sha256': digest(plan_path),
                'harness_sha256': {name: digest(BASE / local) for name, local in
                                   [('runner.py', 'neo-runner.py'), ('owner.py', 'neo-owner.py'),
                                    ('dependency-fetch.py', 'neo-dependency-fetch.py')]}}
    snapshot_path = prefix.with_name(prefix.name + '-snapshot.json')
    save(snapshot_path, snapshot)
    transfers = {str(archive): 'source.tar.gz', str(snapshot_path): 'snapshot.json', str(plan_path): 'plan.json',
                 str(BASE / 'neo-runner.py'): 'runner.py', str(BASE / 'neo-owner.py'): 'owner.py',
                 str(BASE / 'neo-dependency-fetch.py'): 'dependency-fetch.py'}
    save(prefix.with_name(prefix.name + '-prepared.json'), {
        'schema': 'airk.readiness.neo-prepared/1', 'prepared_only': True, 'launched': False,
        'label': args.label, 'remote_parent': '/Users/tylerjensen/airk-builds',
        'native_source': expected, 'transfers': transfers,
        'pins': {path: digest(Path(path)) for path in transfers},
        'root_review_required_before_launch': True,
        'launch': 'New exact owned remote directory; transfer only listed files; run pinned Python dependency-fetch.py once and inspect success receipt; after parent Cargo handoff start pinned Python owner.py with detached log',
        'collection': 'Collect all JSON, logs and cargo-timings before assessing pass; clean failure never blocks evidence collection'})
    print(json.dumps({'prepared_only': True, 'label': args.label, 'public_files': len(rows), 'native_source': expected}))


if __name__ == '__main__':
    main()
