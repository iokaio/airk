# SPDX-License-Identifier: Apache-2.0
"""Publication /3 custody of the native-emitted optional-model asset projection.

This module never decodes Core Profile or derives model policy. Native emits the
reviewed projection only after its complete compiled catalog passes qualification.
Local/downloaded metadata cannot substitute for that source-owned projection.
"""
from copy import deepcopy
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shutil

ROOT = Path(__file__).resolve().parents[1]
PROJECTION = ROOT / 'fixtures/arbiter/publication_catalog.json'
CATALOG = 'airk-release-catalog/3'
RELEASE = 'airk-github-release/3'
SIGNED = 'airk-signed-release-producer/3'
PART_LIMIT = 1024 * 1024 * 1024


def _load():
    spec = importlib.util.spec_from_file_location('optional_publication_v2', Path(__file__).with_name('publication-contract.py'))
    owner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owner)
    return owner


previous = _load()


def current(version):
    return tuple(map(int, version.split('.'))) >= (1, 9, 0)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False)


def projection(tag, *, path=None):
    """Read the actual native capture, never a network/caller profile catalog."""
    source = PROJECTION if path is None else Path(path)
    if source.is_symlink() or not source.is_file() or source.stat().st_size > 1024 * 1024:
        raise ValueError('Native qualified optional publication projection is unavailable')
    value = json.loads(source.read_bytes())
    if (value.get('schema') != 'airk.arbiter.publication/1' or value.get('tag') != tag
            or not re.fullmatch(r'[0-9a-f]{64}', value.get('compiled_catalog_sha256', ''))):
        raise ValueError('Native optional publication source or tag differs')
    compiled = ROOT / 'crates/airk-retrieval/src/arbiter/release-catalog.json'
    if path is None and hashlib.sha256(compiled.read_bytes()).hexdigest() != value['compiled_catalog_sha256']:
        raise ValueError('Optional publication projection is stale for the compiled native catalog')
    # These are publication file bounds, not a mirror of Core Profile semantics.
    for name, asset in value['assets'].items():
        if (not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,191}', name)
                or '..' in name or type(asset['bytes']) is not int or not 0 < asset['bytes'] <= PART_LIMIT
                or not re.fullmatch(r'[0-9a-f]{64}', asset['sha256'])):
            raise ValueError('Native optional published file exceeds custody bounds')
    return value


def part_input(name, commit, run_id):
    return f'airk-arbiter-part-{name}-{commit}-{run_id}'


def part_inputs(owner, commit, run_id):
    return {part_input(name, commit, run_id) for name in owner['assets']}


def validate_projection(value, owner):
    if canonical(value) != canonical(owner):
        raise ValueError('Optional metadata differs from the native compiled publication projection')


def validate_signed(document, owner):
    fields = {'schema', 'repository', 'workflow_id', 'commit', 'run_id', 'attempt',
              'artifact_name', 'artifact', 'event', 'source_branch', 'qualified_commit',
              'parts', 'optional_models'}
    if not isinstance(document, dict) or set(document) != fields or document.get('schema') != SIGNED:
        raise ValueError('Optional signed producer schema or fields differ')
    validate_projection(document['optional_models'], owner)
    if (document['repository'] != 'iokaio/airk' or document['event'] != 'workflow_dispatch'
            or document['source_branch'] != 'feat/advanced-split'
            or document['qualified_commit'] != document['commit']
            or not re.fullmatch(r'[0-9a-f]{40}', document['commit'])):
        raise ValueError('Optional signed producer source differs')
    for key in ('run_id', 'attempt'):
        if not isinstance(document[key], str) or not re.fullmatch(r'[1-9][0-9]*', document[key]):
            raise ValueError('Optional signed producer run/attempt differs')
    if type(document['workflow_id']) is not int or document['workflow_id'] <= 0:
        raise ValueError('Optional signed producer workflow differs')
    if document['artifact_name'] != f"airk-release-{document['commit']}-{document['run_id']}-{document['attempt']}":
        raise ValueError('Optional signed assembly name differs')
    _artifact(document['artifact'])
    parts = document['parts']
    if not isinstance(parts, dict) or set(parts) != set(owner['assets']):
        raise ValueError('Optional signed part artifact roster differs')
    ids = {document['artifact']['id']}
    for name, artifact in parts.items():
        if not isinstance(artifact, dict) or set(artifact) != {'id', 'digest', 'artifact_name'}:
            raise ValueError('Optional signed part artifact fields differ')
        _artifact({key: artifact[key] for key in ('id', 'digest')})
        if (artifact['artifact_name'] != part_input(name, document['commit'], document['run_id'])
                or artifact['id'] in ids):
            raise ValueError('Optional signed part artifact identity is duplicated or differs')
        ids.add(artifact['id'])
    return SIGNED


def _artifact(value):
    if (not isinstance(value, dict) or set(value) != {'id', 'digest'}
            or type(value['id']) is not int or value['id'] <= 0
            or not isinstance(value['digest'], str) or not re.fullmatch(r'sha256:[0-9a-f]{64}', value['digest'])):
        raise ValueError('Optional signed artifact ID/digest differs')


def validate_catalog(document, owner):
    if not isinstance(document, dict) or document.get('schema') != CATALOG:
        raise ValueError('Optional publication catalog schema differs')
    validate_projection(document.get('optional_models'), owner)
    if document.get('tag') != owner['tag']:
        raise ValueError('Optional catalog tag differs from native authority')
    projected = deepcopy(document)
    projected.pop('optional_models')
    projected.pop('tag')
    projected['schema'] = previous.CATALOG
    inputs = part_inputs(owner, document['commit'], document['release']['run_id'])
    values = projected.get('release_inputs')
    if not isinstance(values, list) or len(values) != len(set(values)) or not inputs <= set(values):
        raise ValueError('Optional same-run part input closure differs')
    projected['release_inputs'] = [name for name in values if name not in inputs]
    _remove_assets(projected, owner)
    previous.validate_catalog(projected)
    return CATALOG


def _remove_assets(document, owner):
    assets = document.get('assets')
    if not isinstance(assets, dict):
        raise ValueError('Optional published asset inventory is missing')
    for name, asset in owner['assets'].items():
        if assets.pop(name, None) != asset['sha256']:
            raise ValueError('Optional published part is missing or has another digest')


def validate_release(document, owner):
    if not isinstance(document, dict) or document.get('schema') != RELEASE:
        raise ValueError('Optional release schema differs')
    validate_projection(document.get('optional_models'), owner)
    if document.get('tag') != owner['tag']:
        raise ValueError('Optional release tag differs from native authority')
    projected = deepcopy(document)
    projected.pop('optional_models')
    projected['schema'] = previous.RELEASE
    _remove_assets(projected, owner)
    previous.validate_release(projected)
    return RELEASE


def verify_part(path, asset):
    path = Path(path)
    if path.is_symlink() or not path.is_file() or path.stat().st_size != asset['bytes']:
        raise ValueError('Optional published part size/type differs')
    with path.open('rb') as source:
        actual = hashlib.file_digest(source, 'sha256').hexdigest()
    if actual != asset['sha256']:
        raise ValueError('Optional published part digest differs')


def stage_parts(inputs, output, owner, commit, run_id):
    """Each bounded same-run artifact holds exactly its original one part file."""
    selected = []
    for name, asset in owner['assets'].items():
        directory = Path(inputs) / part_input(name, commit, run_id)
        if directory.is_symlink() or not directory.is_dir() or {p.name for p in directory.iterdir()} != {name}:
            raise ValueError('Optional part artifact has a different exact roster')
        path = directory / name
        verify_part(path, asset)
        selected.append((path, name))
    Path(output).mkdir(parents=True, exist_ok=False)
    for path, name in selected:
        shutil.copyfile(path, Path(output) / name)
        verify_part(Path(output) / name, owner['assets'][name])
    return {name: asset['sha256'] for name, asset in owner['assets'].items()}
