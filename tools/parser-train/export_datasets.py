# SPDX-License-Identifier: Apache-2.0
"""Publish or restore exact, attributable parser examples; never copy native state.

Only datasets explicitly identified by public model manifests/qualification
records are eligible. Compressed files retain the original JSONL bytes. This
module uses the standard library and neither downloads nor uploads anything.
"""
import argparse
from collections import Counter
import gzip
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import platform
import re
import stat
import zlib

ROOT = Path(__file__).resolve().parents[2]
PUBLIC = 'tools/parser-train/data'
GENERATED = 'target/parser-train/data'
ATTRIBUTES = 'tools/parser-train/.gitattributes'
FROZEN_WHITESPACE = 'whitespace=blank-at-eol,blank-at-eof,space-before-tab,cr-at-eol'
ATTRIBUTE_RULES = ['*.json -text ' + FROZEN_WHITESPACE, 'data/*.jsonl.gz -text',
                   'data/licenses/*.txt -text ' + FROZEN_WHITESPACE, 'data/licenses/*.txt.gz -text']
SCHEMA = 'airk.parser.datasets/1'
LABELS = {'search', 'find_definition', 'references', 'read_file', 'outline',
          'list_files', 'file_info', 'imports', 'facts', 'changes', 'status',
          'diagnostics', 'help', 'unsupported'}
MAX_FILE = 256 * 1024 * 1024
MAX_TOTAL = 1024 * 1024 * 1024
MAX_ROWS = 2_000_000
MAX_FILES = 256
MAX_LINE = 65536
DIGEST = re.compile(r'[0-9a-f]{64}')
RECORD = re.compile(r'(?:manifest(?:-smoke)?|qualification(?:-initial|-broader(?:-v[1-9][0-9]*)?)?)\.json')
BASE = {'text', 'intent', 'role', 'split'}
REPO = {'repository', 'revision', 'vocabulary_sha256'}
FAMILY = BASE | REPO | {'family', 'origin'}
LEGACY = BASE | REPO | {'noise', 'template_id'}
CLINC = BASE | {'origin', 'source_id', 'source_intent', 'source_split'}
SYNTHETIC = {'text', 'intent', 'origin', 'split', 'example_id'}
ROLE = FAMILY | {'authored_family', 'query_id', 'literal_slots', 'expected_tools',
                 'expected_target', 'label_provenance'}
QUERY = {'query_sha256', 'query_source'}


def digest(content):
    return hashlib.sha256(content).hexdigest()


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + '\n').encode('utf-8')


def relative(value):
    if not isinstance(value, str) or '\\' in value or ':' in value or '\x00' in value:
        raise ValueError('A publication path is not a safe relative POSIX path')
    path = PurePosixPath(value)
    if path.is_absolute() or not path.parts or any(part in ('', '.', '..') for part in value.split('/')):
        raise ValueError('A publication path escapes its allowed root')
    return path


def checked(root, value, *, missing=False):
    path = relative(value)
    current = Path(root)
    if current.is_symlink() or (getattr(current.stat(), 'st_file_attributes', 0)
                               & getattr(stat, 'FILE_ATTRIBUTE_REPARSE_POINT', 0)):
        raise ValueError('Publication root is redirected')
    for part in path.parts:
        current /= part
        if current.is_symlink():
            raise ValueError('Publication refuses symlinks: ' + value)
        if current.exists():
            info = current.stat()
            if getattr(info, 'st_file_attributes', 0) & getattr(stat, 'FILE_ATTRIBUTE_REPARSE_POINT', 0):
                raise ValueError('Publication refuses redirected paths: ' + value)
    if not current.resolve().is_relative_to(Path(root).resolve()):
        raise ValueError('Publication path escapes the repository')
    if not missing and not current.is_file():
        raise ValueError('Required publication input is missing: ' + value)
    return current


def read(root, value, limit=MAX_FILE):
    path = checked(root, value)
    if path.stat().st_size > limit:
        raise ValueError('Publication input exceeds its byte limit: ' + value)
    with path.open('rb') as stream:
        content = stream.read(limit + 1)
    if len(content) > limit:
        raise ValueError('Publication input grew beyond its byte limit: ' + value)
    return content


def json_input(root, value):
    return json.loads(read(root, value, 8 * 1024 * 1024).decode('utf-8'))


def validate_attributes(root):
    # Frozen JSON and original legal notices bind their exact bytes. Git's root
    # text/eol policy must not normalize them between the worktree and checkout.
    lines = read(root, ATTRIBUTES, 65536).decode('utf-8').splitlines()
    rules = [' '.join(line.split()) for line in lines if line.strip() and not line.lstrip().startswith('#')]
    if rules != ATTRIBUTE_RULES:
        raise ValueError('Frozen parser metadata requires the reviewed byte-preserving Git attributes')


def source_input(path):
    return path.is_file() and (path.suffix in {'.py', '.json', '.txt'} or path.name == '.gitattributes')


def pins(root):
    repo_lock = json_input(root, 'tools/parser-train/repositories.lock.json')
    online_lock = json_input(root, 'tools/parser-train/online.lock.json')
    repositories = {row['repository']: row for row in repo_lock['repositories']}
    sources = {row['id']: row for row in online_lock['sources']}
    for repository, row in repositories.items():
        if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository) or not re.fullmatch(r'[0-9a-f]{40}', row['revision']):
            raise ValueError('Unrecognized public repository provenance')
    if set(sources) - {'clinc150', 'codesearchnet-queries'}:
        raise ValueError('Unrecognized public query source')
    known_sources = {'clinc150': ('clinc/oos-eval', 'CC-BY-3.0'),
                     'codesearchnet-queries': ('github/CodeSearchNet', 'MIT')}
    for identifier, row in sources.items():
        if ((row.get('repository'), row.get('license')) != known_sources[identifier]
                or not re.fullmatch(r'[0-9a-f]{40}', row.get('revision', ''))):
            raise ValueError('Public query source identity/license changed without review')
    role_authority = None
    inventory = Path(root) / 'tools/parser-train/role-queries.json'
    split_map = Path(root) / 'tools/parser-train/role-phrase-splits-v5.json'
    if inventory.exists() and split_map.exists():
        role_rows = json_input(root, 'tools/parser-train/role-queries.json')
        groups = json_input(root, 'tools/parser-train/role-phrase-splits-v5.json')
        if groups.get('role_source_sha256') != digest(read(root, 'tools/parser-train/role-queries.json')) or groups.get('contradictory_labels') != 0:
            raise ValueError('Role source inventory differs from its frozen split authority')
        role_authority = {'queries': role_rows['queries'],
                          'families': {row['family']: row for row in groups['families']}}
    return repositories, sources, role_authority


def validate_row(row, repositories, sources, roles=None):
    if not isinstance(row, dict) or row.get('intent') not in LABELS:
        raise ValueError('Unrecognized dataset row or intent')
    text = row.get('text')
    if not isinstance(text, str) or not text.strip() or len(text.encode('utf-8')) > 4096:
        raise ValueError('Dataset row has invalid or oversized query text')
    if row.get('split') not in {'train', 'validation', 'calibration', 'test', 'repository-evaluation'}:
        raise ValueError('Unrecognized dataset split')
    keys, origin = set(row), row.get('origin')
    if keys == SYNTHETIC and origin == 'synthetic-template':
        if type(row['example_id']) is not int or row['example_id'] < 0 or row['split'] != 'train':
            raise ValueError('Invalid original synthetic provenance')
        return 'synthetic-template'
    if row.get('role') not in {'developer', 'writer', 'architect', 'tester', 'public-assistant-task'}:
        raise ValueError('Unrecognized dataset role')
    if keys == CLINC and origin == 'clinc150-non-repository-task':
        if 'clinc150' not in sources or row['intent'] != 'unsupported' or row['role'] != 'public-assistant-task':
            raise ValueError('CLINC provenance does not identify an unsupported public request')
        normalized = re.sub(r'\s+', ' ', text.strip().lower())
        if row['source_id'] != digest(normalized.encode()) or row['source_split'] not in {'train', 'val', 'test'}:
            raise ValueError('CLINC source identity is invalid')
        return origin
    if keys == LEGACY:
        if row['noise'] not in {'none', 'identifier-character-drop', 'identifier-subtokens'} or not isinstance(row['template_id'], str):
            raise ValueError('Unrecognized original template provenance')
        origin = 'original-template-with-public-vocabulary'
    elif keys in (FAMILY, FAMILY | QUERY):
        if origin not in {'original-phrase-family', 'original-conversational-family', 'codesearchnet-query-in-explicit-retrieval-request'}:
            raise ValueError('Unrecognized phrase-family provenance')
        if not isinstance(row['family'], str) or not row['family']:
            raise ValueError('Missing original phrase-family identity')
        if origin == 'codesearchnet-query-in-explicit-retrieval-request':
            if keys != FAMILY | QUERY or row['query_source'] != 'codesearchnet-queries' or row['query_source'] not in sources or not DIGEST.fullmatch(row['query_sha256']):
                raise ValueError('CodeSearchNet provenance is incomplete')
        elif keys != FAMILY:
            raise ValueError('Original phrase row has unexpected public-query metadata')
    elif keys in (ROLE, ROLE | QUERY) and origin in {'original-role-inventory', 'original-baseline-family'}:
        if roles is None:
            raise ValueError('Role examples lack their public frozen source authority')
        authority = roles['families'].get(row['authored_family'])
        if authority is None or (authority['group'], authority['split']) != (row['family'], row['split']):
            raise ValueError('Role example family/split differs from its public authority')
        slots = row['literal_slots']
        if (not isinstance(slots, dict) or set(slots) != {'s', 'p', 'scope', 'k', 'token', 'baseline'}
                or any(not isinstance(value, str) or len(value.encode('utf-8')) > 4096 for value in slots.values())
                or not re.fullmatch(r'airk-r1:1{32}:[1-9][0-9]{0,4}:2{64}', slots['token'])
                or slots['baseline'] != slots['token']):
            raise ValueError('Role example has unrecognized literal slots or a non-fictional receipt')
        relative(slots['p'])
        if slots['scope'] != '.':
            relative(slots['scope'])
        if (not isinstance(row['expected_target'], str) or not isinstance(row['expected_tools'], list)
                or any(tool not in {'search', 'symbols', 'references', 'read_file', 'outline', 'list_files',
                                    'file_info', 'imports', 'code_facts', 'document_facts', 'changes', 'context',
                                    'relationships', 'status', 'diagnostics', 'capabilities', 'help'} for tool in row['expected_tools'])
                or row['label_provenance'] != 'Original agent-authored finite-tool intent; human review pending.'):
            raise ValueError('Role example has unrecognized expected operations or provenance')
        if origin == 'original-role-inventory':
            if not re.fullmatch(r'role/[0-9]+', row['query_id']):
                raise ValueError('Role query identifier is not a public source row')
            index = int(row['query_id'].split('/')[1])
            if index >= len(roles['queries']):
                raise ValueError('Role query identifier is outside the public inventory')
            source = roles['queries'][index]
            if ((source['family'], source['intent'], source['role'], source['expected_tools'])
                    != (row['authored_family'], row['intent'], row['role'], row['expected_tools'])
                    or (source.get('target') or '').format(**slots) != row['expected_target']):
                raise ValueError('Role example differs from its public authored query contract')
        elif (not row['query_id'].startswith(('baseline/', 'conversational/'))
              or row['query_id'].rsplit('/', 1)[0] != row['authored_family']):
            raise ValueError('Baseline example lacks an exact original-family identity')
        if keys == ROLE | QUERY and (row['query_source'] != 'codesearchnet-queries'
                                    or row['query_source'] not in sources or not DIGEST.fullmatch(row['query_sha256'])):
            raise ValueError('Role public-query provenance is incomplete')
    else:
        raise ValueError('Unrecognized dataset row schema/provenance')
    repository = repositories.get(row['repository'])
    if repository is None or repository['revision'] != row['revision'] or not DIGEST.fullmatch(row['vocabulary_sha256']):
        raise ValueError('Dataset vocabulary is not bound to a pinned public repository')
    return origin


def inspect_rows(content, repositories, sources, roles=None):
    stats = {key: Counter() for key in ('splits', 'roles', 'intents', 'origins', 'repositories')}
    count = 0
    for line in io.BytesIO(content):
        if not line.strip() or len(line) > MAX_LINE:
            raise ValueError('Dataset has an empty or oversized JSONL record')
        try:
            row = json.loads(line.decode('utf-8'))
            origin = validate_row(row, repositories, sources, roles)
        except (ValueError, KeyError, TypeError) as error:
            raise ValueError(f'Dataset row {count + 1} failed provenance/schema validation: {error}') from error
        count += 1
        if count > MAX_ROWS:
            raise ValueError('Dataset exceeds its row budget')
        stats['splits'][row['split']] += 1
        stats['roles'][row.get('role', 'fictional-template')] += 1
        stats['intents'][row['intent']] += 1
        stats['origins'][origin] += 1
        if 'repository' in row:
            stats['repositories'][row['repository'] + '@' + row['revision']] += 1
    return {'rows': count, **{key: dict(sorted(value.items())) for key, value in stats.items()}}


def records(root):
    directory = Path(root) / 'tools/parser-train'
    selected = []
    for path in sorted(directory.iterdir()):
        if not RECORD.fullmatch(path.name):
            continue
        name = path.relative_to(root).as_posix()
        content = read(root, name, 8 * 1024 * 1024)
        value = json.loads(content)
        if not isinstance(value, dict) or not DIGEST.fullmatch(value.get('asset_sha256', '')):
            raise ValueError('Model record lacks an exact model identity: ' + name)
        if path.name == 'manifest.json':
            model = read(root, 'crates/airk-retrieval/assets/intent-v1.bin', 1024 * 1024)
            if digest(model) != value['asset_sha256'] or len(model) != value.get('asset_bytes'):
                raise ValueError('Embedded intent model differs from its current public manifest')
        references = dict(value.get('datasets', {}))
        if 'dataset' in value:
            references['train'] = value['dataset']
        if not references:
            raise ValueError('Model record lacks exact dataset references: ' + name)
        selected.append((name, content, value, references))
    if not selected or len(selected) > MAX_FILES:
        raise ValueError('No bounded authoritative model records were selected')
    current = next((row for row in selected if row[0] == 'tools/parser-train/manifest.json'), None)
    if current is not None:
        manifest = current[2]
        report_path = manifest.get('qualification_report')
        report = next((row for row in selected if row[0] == report_path), None)
        if (report is None or digest(report[1]) != manifest.get('qualification_report_sha256')
                or report[2]['asset_sha256'] != manifest['asset_sha256']):
            raise ValueError('Current model qualification report differs from its manifest')
    return selected


def current_model(model_records):
    current = next((row[2] for row in model_records if row[0] == 'tools/parser-train/manifest.json'), None)
    if current is None:
        return None
    report = next(row[2] for row in model_records if row[0] == current['qualification_report'])
    selected = report.get('fitted_training', report['datasets'].get('fitted_train', report['datasets']['train']))
    return {'model_version': current.get('model_version'), 'asset_sha256': current['asset_sha256'],
            'qualification_report': current['qualification_report'],
            'qualification_report_sha256': current['qualification_report_sha256'],
            'selected_training': selected, 'training_pool': report['datasets']['train'],
            'dataset_roles': {key: value['path'] for key, value in sorted(report['datasets'].items())},
            'note': 'Training pool and exact fitted subset are distinct when selection is recorded; history totals are not the current model training size.'}


def dataset_selection(model_records):
    selected = {}
    for record, _, model, references in model_records:
        for split, reference in references.items():
            if not isinstance(reference, dict) or set(reference) - {'path', 'sha256', 'rows', 'bytes'}:
                raise ValueError('Unrecognized exact dataset reference in ' + record)
            path = relative(reference.get('path'))
            if path.parent.as_posix() != GENERATED or not path.name.endswith('.jsonl'):
                raise ValueError('Dataset reference is outside the generated data root')
            sha = reference.get('sha256', '')
            if not DIGEST.fullmatch(sha) or not path.name.endswith('-' + sha[:16] + '.jsonl') or type(reference.get('rows')) is not int or not 1 <= reference['rows'] <= MAX_ROWS:
                raise ValueError('Dataset reference lacks an exact content identity')
            key = path.as_posix()
            item = selected.setdefault(key, {'sha256': sha, 'rows': reference['rows'], 'references': []})
            if (item['sha256'], item['rows']) != (sha, reference['rows']):
                raise ValueError('Conflicting dataset identities: ' + key)
            item['references'].append({'record': record, 'split': split, 'model_sha256': model['asset_sha256'],
                                       'model_version': model.get('model_version'), 'protocol': model.get('protocol'),
                                       'feature_spec': model.get('feature_spec', 1), 'seed': model.get('seed')})
    if len(selected) > MAX_FILES:
        raise ValueError('Too many exact dataset references')
    return selected


def compressed(content):
    output = io.BytesIO()
    with gzip.GzipFile(filename='', mode='wb', fileobj=output, compresslevel=9, mtime=0) as stream:
        stream.write(content)
    return output.getvalue()


def write_owned(root, value, content):
    path = checked(root, value, missing=True)
    if path.exists() and path.read_bytes() == content:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    # Only the explicitly selected publication namespace is mutable here. Restores
    # use exclusive writes below and never overwrite different generated inputs.
    path.write_bytes(content)


def licenses(root, repositories, sources):
    output, attribution, gzip_metadata, obsolete = {}, [], {}, []
    for repository, item in sorted(repositories.items()):
        entries = []
        for number, record in enumerate(item['license_files']):
            license_path = relative(record['path'])
            if len(license_path.parts) != 1 or not re.fullmatch(r'(?:LICENSE|COPYING|NOTICE)(?:[-.][A-Za-z0-9-]+)?', license_path.name, re.IGNORECASE):
                raise ValueError('Pinned repository license path is not a root legal notice')
            source = 'target/parser-train/repos/' + repository.replace('/', '--') + '/' + record['path']
            content = read(root, source, 1024 * 1024)
            if digest(content) != record['sha256'] or record.get('revision') != item['revision']:
                raise ValueError('Pinned public repository license differs: ' + repository)
            target = PUBLIC + '/licenses/repository-' + repository.replace('/', '--') + '-' + item['revision'][:12] + '-' + str(number) + '.txt'
            output[target] = content
            entries.append({'original_path': record['path'], 'path': target, 'sha256': digest(content),
                            'bytes': len(content), 'declared_license': record['detected_license']})
        attribution.append({'kind': 'public-vocabulary-source', 'repository': repository, 'revision': item['revision'],
                            'url': 'https://github.com/' + repository + '/tree/' + item['revision'], 'licenses': entries,
                            'use': 'Original templates contain extracted public identifier/path/heading vocabulary, not source file bodies.'})
    for identifier, item in sorted(sources.items()):
        entries = []
        for record in item['inputs']:
            if record['path'] not in {'LICENSE', 'README.md'}:
                continue
            source = relative(record['local_path'])
            expected = 'target/parser-train/online/' + identifier + '-' + item['revision'][:12] + '/' + record['path']
            if source.as_posix() != expected:
                raise ValueError('Online attribution points outside its pinned input')
            content = read(root, expected, 1024 * 1024)
            if digest(content) != record['sha256'] or len(content) != record['bytes']:
                raise ValueError('Pinned public query attribution differs: ' + identifier)
            target = PUBLIC + '/licenses/online-' + identifier + '-' + item['revision'][:12] + '-' + record['path'].replace('.', '-') + '.txt'
            stored = content
            if record['path'] == 'README.md':
                old = checked(root, target, missing=True)
                if old.exists():
                    if digest(read(root, target, 1024 * 1024)) != record['sha256']:
                        raise ValueError('Original generated README differs; it was preserved: ' + target)
                    obsolete.append((target, record['sha256']))
                target += '.gz'
                stored = compressed(content)
                gzip_metadata[target] = {'encoding': 'gzip', 'raw_sha256': record['sha256'], 'raw_bytes': record['bytes']}
            output[target] = stored
            entries.append({'original_path': record['path'], 'path': target, 'sha256': digest(stored),
                            'bytes': len(stored), 'url': record['url'], **gzip_metadata.get(target, {})})
        if not any(entry['original_path'] == 'LICENSE' for entry in entries):
            raise ValueError('Public query source has no exact license text')
        attribution.append({'kind': 'public-query-source', 'id': identifier, 'repository': item['repository'],
                            'revision': item['revision'], 'declared_license': item['license'],
                            'attribution': item['attribution'], 'licenses': entries,
                            'use': 'AIRK-generated CodeSearchNet wrappers or CLINC requests relabeled as unsupported; original source license applies.'})
    license_text = read(root, 'LICENSE', 1024 * 1024)
    output[PUBLIC + '/licenses/airk-Apache-2.0.txt'] = license_text
    attribution.append({'kind': 'original-AIRK-templates', 'declared_license': 'Apache-2.0',
                        'path': PUBLIC + '/licenses/airk-Apache-2.0.txt', 'sha256': digest(license_text),
                        'use': 'Original templates, role inventories and generated fictional phrasing; labels remain open to human review.'})
    output[PUBLIC + '/licenses/attribution.json'] = encoded({'schema': 1, 'sources': attribution})
    metadata = [{'path': path, 'sha256': digest(content), 'bytes': len(content), **gzip_metadata.get(path, {})}
                for path, content in sorted(output.items())]
    return output, metadata, obsolete


README = b'''# Exact parser-training datasets

These deterministic gzip files contain the exact expanded JSONL bytes referenced
by the public model manifests and qualification reports, including historical
failed/unselected campaigns and held-out splits. They are data for review and
reproduction, not evidence of universal query understanding. Reused test fixtures
are regression measurements, not fresh blind tests. Do not train on held-out rows.

`catalog.json` records original paths, raw/compressed hashes and sizes, row counts,
roles, splits, provenance and model references. Identical raw datasets are stored
once. `current_model.selected_training` identifies the exact fitted examples;
`current_model.training_pool` identifies the larger selection pool when present.
Historical totals can include overlapping pools/subsets and are not counts of
unique utterances or the current fitted model's training size.
Original AIRK templates are Apache-2.0; public vocabulary and query inputs
retain their pinned source licenses and attribution in `licenses/`. License
texts remain plain `.txt`; the two pinned upstream README provenance
artifacts are exact `.txt.gz` files, retaining their original Markdown whitespace.
Their catalog/attribution entries distinguish stored gzip hashes/sizes from the
original raw hashes/sizes. Preview them with `gzip -dc` or Python's `gzip.open`.
Training dependency pins remain in `../requirements.txt`; no dependency implementation,
source checkout, private corpus, vocabulary dump or native index is distributed.

Preview: `gzip -dc FILE.jsonl.gz` or Python's `gzip.open(FILE, 'rt')`.
Verify from the repository root:
`python tools/parser-train/export_datasets.py verify`
Restore the exact original ignored target paths for fitting/evaluation:
`python tools/parser-train/export_datasets.py restore`
Restore refuses to overwrite different existing bytes. Model building still needs
the pinned Python training dependencies. Exact fitted-data replay and custom
extra-training through `../fit-saved.py` use the restored examples and held-out
sets without source checkouts or harvested vocabulary. Only full corpus
regeneration needs the verified public vocabulary inputs described in
`../TRAINING.md`. The expanded examples can be inspected without training tools.
See the contributor workflow in `../../../docs/parser-training.md`.

After an authorized campaign freezes, refresh the reviewed branch data with:
`python tools/parser-train/export_datasets.py publish`
Publication preparation never stages, commits, pushes or uploads files. Raw
training directories, local retained copies, clones and native indexes stay
ignored. The publisher refuses unknown row provenance instead of redacting or
silently changing the exact trained examples. gzip mtime/name fields are fixed;
compressed bytes are reproducible with the same Python/zlib implementation.
Validation checks exact hashes, row schemas and declared source/model identities;
it does not certify labels or reconstruct every historical template. Review the
public generation scripts, reports and source inventory when assessing quality.
The local `.gitattributes` preserves frozen JSON, compressed data and original
license text bytes across Git staging/checkouts; do not normalize those files.
Its `cr-at-eol` whitespace setting accepts original CRLF endings while retaining
the normal end-of-line, end-of-file and space-before-tab whitespace checks.
'''


def publish(root=ROOT):
    validate_attributes(root)
    repositories, sources, roles = pins(root)
    model_records = records(root)
    selected = dataset_selection(model_records)
    entries, total = {}, 0
    for path, identity in sorted(selected.items()):
        content = read(root, path)
        total += len(content)
        if total > MAX_TOTAL or digest(content) != identity['sha256']:
            raise ValueError('Dataset total budget or exact digest validation failed: ' + path)
        stats = inspect_rows(content, repositories, sources, roles)
        if stats['rows'] != identity['rows']:
            raise ValueError('Dataset row count differs from its exact model record: ' + path)
        sha = identity['sha256']
        if sha in entries:
            entries[sha]['original_paths'].append(path)
            entries[sha]['references'].extend(identity['references'])
            continue
        zipped = compressed(content)
        target = PUBLIC + '/' + PurePosixPath(path).name + '.gz'
        entries[sha] = {'path': target, 'original_paths': [path], 'raw_sha256': sha, 'raw_bytes': len(content),
                        'gzip_sha256': digest(zipped), 'gzip_bytes': len(zipped), **stats,
                        'references': identity['references']}
    legal, legal_records, obsolete = licenses(root, repositories, sources)
    source_files = []
    for path in sorted((Path(root) / 'tools/parser-train').iterdir()):
        if source_input(path):
            name = path.relative_to(root).as_posix()
            content = read(root, name, 8 * 1024 * 1024)
            source_files.append({'path': name, 'sha256': digest(content), 'bytes': len(content)})
    catalog = {'schema': SCHEMA, 'scope': 'Exact expanded public parser-training dataset history, not clones or native state.',
               'datasets': sorted(entries.values(), key=lambda item: item['path']), 'source_files': source_files,
               'current_model': current_model(model_records),
               'compression': {'format': 'gzip', 'level': 9, 'mtime': 0, 'embedded_filename': '',
                               'python': platform.python_version(), 'implementation': platform.python_implementation(),
                               'zlib': zlib.ZLIB_RUNTIME_VERSION},
               'attribution': {'path': PUBLIC + '/licenses/attribution.json',
                               'sha256': digest(legal[PUBLIC + '/licenses/attribution.json'])},
               'license_files': legal_records,
               'totals': {'files': len(entries), 'rows': sum(row['rows'] for row in entries.values()),
                          'raw_bytes': sum(row['raw_bytes'] for row in entries.values()),
                          'gzip_bytes': sum(row['gzip_bytes'] for row in entries.values())}}
    # Validate every dataset and legal input before materializing public payloads.
    # Recheck the bytes on the second read so changes during preparation refuse.
    for item in entries.values():
        content = read(root, item['original_paths'][0])
        if digest(content) != item['raw_sha256']:
            raise ValueError('Dataset changed during publication preparation')
        zipped = compressed(content)
        if digest(zipped) != item['gzip_sha256']:
            raise ValueError('Compression changed during publication preparation')
        write_owned(root, item['path'], zipped)
    for path, content in legal.items():
        write_owned(root, path, content)
    for path, expected_sha in obsolete:
        old = checked(root, path)
        if digest(read(root, path, 1024 * 1024)) != expected_sha:
            raise ValueError('Generated README changed during retirement; it was preserved')
        old.unlink()
    write_owned(root, PUBLIC + '/README.md', README)
    write_owned(root, PUBLIC + '/catalog.json', encoded(catalog))
    return verify(root)


def verify(root=ROOT):
    validate_attributes(root)
    repositories, sources, roles = pins(root)
    catalog = json_input(root, PUBLIC + '/catalog.json')
    if catalog.get('schema') != SCHEMA or not 1 <= len(catalog.get('datasets', [])) <= MAX_FILES:
        raise ValueError('Unrecognized public dataset catalog')
    method = catalog.get('compression', {})
    if ({key: method.get(key) for key in ('format', 'level', 'mtime', 'embedded_filename')}
            != {'format': 'gzip', 'level': 9, 'mtime': 0, 'embedded_filename': ''}):
        raise ValueError('Unrecognized deterministic compression declaration')
    model_records = records(root)
    selected = dataset_selection(model_records)
    if catalog.get('current_model') != current_model(model_records):
        raise ValueError('Catalog current-model/fitted-subset identity differs from public records')
    originals, hashes, total = set(), set(), 0
    for item in catalog['datasets']:
        path = relative(item['path'])
        if path.parent.as_posix() != PUBLIC or not path.name.endswith('.jsonl.gz') or item['raw_sha256'] in hashes:
            raise ValueError('Catalog contains a redirected or duplicate dataset')
        hashes.add(item['raw_sha256'])
        zipped = read(root, item['path'])
        if len(zipped) != item['gzip_bytes'] or digest(zipped) != item['gzip_sha256']:
            raise ValueError('Compressed dataset differs from catalog')
        if len(zipped) < 18 or zipped[:10] != b'\x1f\x8b\x08\x00\x00\x00\x00\x00\x02\xff':
            raise ValueError('Dataset gzip header is not deterministic')
        with gzip.GzipFile(fileobj=io.BytesIO(zipped)) as stream:
            content = stream.read(MAX_FILE + 1)
        total += len(content)
        if len(content) > MAX_FILE or total > MAX_TOTAL or len(content) != item['raw_bytes'] or digest(content) != item['raw_sha256']:
            raise ValueError('Expanded dataset differs from catalog or exceeds its budget')
        if inspect_rows(content, repositories, sources, roles) != {key: item[key] for key in ('rows', 'splits', 'roles', 'intents', 'origins', 'repositories')}:
            raise ValueError('Expanded dataset provenance/counts differ from catalog')
        for original in item['original_paths']:
            identity = selected.get(original)
            if original in originals or identity is None or identity['sha256'] != item['raw_sha256'] or identity['rows'] != item['rows']:
                raise ValueError('Catalog dataset is not selected by exact public model records')
            originals.add(original)
        expected = [reference for original in item['original_paths'] for reference in selected[original]['references']]
        if item['references'] != expected:
            raise ValueError('Catalog model references differ from public records')
    if originals != set(selected):
        raise ValueError('Catalog does not cover every exact public dataset reference')
    actual_sources = {path.relative_to(root).as_posix() for path in (Path(root) / 'tools/parser-train').iterdir()
                      if source_input(path)}
    if {item['path'] for item in catalog['source_files']} != actual_sources:
        raise ValueError('Public training source inventory differs from catalog')
    expected_licenses = {PUBLIC + '/licenses/airk-Apache-2.0.txt': {'sha256': digest(read(root, 'LICENSE', 1024 * 1024))}}
    for repository, source in repositories.items():
        for number, item in enumerate(source['license_files']):
            name = PUBLIC + '/licenses/repository-' + repository.replace('/', '--') + '-' + source['revision'][:12] + '-' + str(number) + '.txt'
            expected_licenses[name] = {'sha256': item['sha256']}
    for identifier, source in sources.items():
        for item in source['inputs']:
            if item['path'] in {'LICENSE', 'README.md'}:
                name = PUBLIC + '/licenses/online-' + identifier + '-' + source['revision'][:12] + '-' + item['path'].replace('.', '-') + '.txt'
                if item['path'] == 'README.md':
                    expected_licenses[name + '.gz'] = {'encoding': 'gzip', 'raw_sha256': item['sha256'], 'raw_bytes': item['bytes']}
                else:
                    expected_licenses[name] = {'sha256': item['sha256']}
    legal_catalog = {item['path']: item for item in catalog['license_files']}
    attribution_path = PUBLIC + '/licenses/attribution.json'
    expected_licenses[attribution_path] = {'sha256': catalog['attribution']['sha256']}
    if (set(legal_catalog) != set(expected_licenses) or len(legal_catalog) != len(catalog['license_files'])
            or catalog['attribution']['path'] != attribution_path):
        raise ValueError('Catalog omits or changes a pinned license/attribution input')
    for path, expected in expected_licenses.items():
        item = legal_catalog[path]
        if any(item.get(key) != value for key, value in expected.items()):
            raise ValueError('Catalog changes a pinned license/provenance identity')
        if expected.get('encoding') == 'gzip':
            stored = read(root, path, 1024 * 1024)
            if stored[:10] != b'\x1f\x8b\x08\x00\x00\x00\x00\x00\x02\xff':
                raise ValueError('README provenance gzip header is not deterministic')
            with gzip.GzipFile(fileobj=io.BytesIO(stored)) as stream:
                content = stream.read(1024 * 1024 + 1)
            if len(content) != expected['raw_bytes'] or len(content) > 1024 * 1024 or digest(content) != expected['raw_sha256']:
                raise ValueError('Expanded README provenance differs from its pinned source')
        elif any(key in item for key in ('encoding', 'raw_sha256', 'raw_bytes')):
            raise ValueError('Plain license text has unexpected compression metadata')
    for item in catalog['source_files'] + catalog['license_files']:
        path = relative(item['path'])
        if path.parent.as_posix() not in {'tools/parser-train', PUBLIC + '/licenses'}:
            raise ValueError('Catalog metadata points outside reviewed public inputs')
        content = read(root, item['path'], 8 * 1024 * 1024)
        if digest(content) != item['sha256'] or len(content) != item['bytes']:
            raise ValueError('Publication provenance input changed: ' + item['path'])
    totals = {'files': len(catalog['datasets']), 'rows': sum(row['rows'] for row in catalog['datasets']),
              'raw_bytes': total, 'gzip_bytes': sum(row['gzip_bytes'] for row in catalog['datasets'])}
    if totals != catalog['totals']:
        raise ValueError('Catalog totals are inconsistent')
    return totals


def restore(root=ROOT):
    totals = verify(root)
    catalog = json_input(root, PUBLIC + '/catalog.json')
    for item in catalog['datasets']:
        for original in item['original_paths']:
            path = checked(root, original, missing=True)
            if path.exists() and digest(read(root, original)) != item['raw_sha256']:
                raise ValueError('Restore preserves different existing generated data: ' + original)
    for item in catalog['datasets']:
        zipped = read(root, item['path'])
        if digest(zipped) != item['gzip_sha256']:
            raise ValueError('Compressed dataset changed during restore')
        with gzip.GzipFile(fileobj=io.BytesIO(zipped)) as stream:
            content = stream.read(MAX_FILE + 1)
        if len(content) != item['raw_bytes'] or len(content) > MAX_FILE or digest(content) != item['raw_sha256']:
            raise ValueError('Expanded dataset changed during restore')
        for original in item['original_paths']:
            path = checked(root, original, missing=True)
            if not path.exists():
                path.parent.mkdir(parents=True, exist_ok=True)
                with path.open('xb') as stream:
                    stream.write(content)
    return totals


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=('publish', 'verify', 'restore'))
    args = parser.parse_args()
    try:
        result = {'publish': publish, 'verify': verify, 'restore': restore}[args.operation]()
    except (ValueError, KeyError, TypeError, OSError, EOFError) as error:
        parser.exit(1, 'Parser dataset ' + args.operation + ' failed: ' + str(error) + '\n')
    print(json.dumps({'operation': args.operation, **result}, sort_keys=True))


if __name__ == '__main__':
    main()
