# SPDX-License-Identifier: Apache-2.0
"""Finite vendor-document role audit; metadata stdout only, no code execution."""
from collections import Counter
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[4]
INPUT = 'artifacts/v190/final-plan/exec-20261116T052942Z/embedded-bounded-coverage-r2.json'
INPUT_SHA = 'e6b776a10990b71cd68adccd8bafbe9c2ab8235bcc7ff45d70a580ca8d98f666'
LIMIT = 16 << 20
ARCHIVE_LIMIT = 512 << 20
MODEL_MARKERS = ('/backend-research/', '/publisher/', '/candidate-assets/', '/selected-model-rights-r1/', '/qwen3-reranker06-assets-r1/', '/qwen3-challenger-assets-r1/', '/minilm-training-source/', '/training-env/', '/models-release/')
EXPECTED = {'rustsec-advisory': 172, 'model-publisher-doc': 29, 'node-package-doc': 376, 'vscode-doc': 12, 'actionlint-doc': 2, 'crate-doc': 4, 'cmake-doc': 160}


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def relative(value):
    if not isinstance(value, str) or not value or ':' in value or '\\' in value or value.startswith('/') or any(x in ('', '.', '..') for x in value.split('/')):
        raise ValueError('canonical relative path required')
    return value


def stamp(info):
    return info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns


def capture(name, limit=LIMIT):
    path = ROOT / relative(name)
    for item in (path, *path.parents):
        info = item.lstat()
        if stat.S_ISLNK(info.st_mode) or getattr(info, 'st_file_attributes', 0) & 0x400:
            raise ValueError('aliased evidence refused')
        if item == ROOT:
            break
    before = path.stat()
    if not stat.S_ISREG(before.st_mode) or before.st_size > limit:
        raise ValueError('bounded regular input required')
    with path.open('rb') as stream:
        if stamp(os.fstat(stream.fileno())) != stamp(before):
            raise ValueError('replaced input')
        raw = stream.read(limit + 1)
        if stamp(os.fstat(stream.fileno())) != stamp(before):
            raise ValueError('input changed while read')
    if len(raw) != before.st_size or stamp(path.stat()) != stamp(before):
        raise ValueError('input changed after read')
    return raw, dict(path=name, sha256=sha(raw), bytes=len(raw))


def parse(raw):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError('duplicate JSON key')
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=unique)


def family(path):
    if '/dependency-policy/' in path: return 'rustsec-advisory'
    if '/node_modules/' in path: return 'node-package-doc'
    if '/tools/cmake/' in path: return 'cmake-doc'
    if '/tools/vscode-' in path: return 'vscode-doc'
    if '/tools/actionlint/' in path: return 'actionlint-doc'
    if '/security-rmcp-' in path: return 'crate-doc'
    if any(part in path for part in MODEL_MARKERS): return 'model-publisher-doc'
    return None


def review():
    checked = {}
    def read(name, limit=LIMIT):
        raw, pin = capture(name, limit)
        if name in checked and checked[name] != pin:
            raise ValueError('evidence drift')
        checked[name] = pin
        return raw, pin
    raw, source_pin = read(INPUT)
    if source_pin['sha256'] != INPUT_SHA:
        raise ValueError('reviewed population changed')
    inventory = parse(raw)
    rows = [r for r in inventory['document_text_scope']['files'] if r['disposition'] == 'remaining_document_or_text_role_limit' and family(r['path'])]
    if Counter(family(r['path']) for r in rows) != Counter(EXPECTED) or len({r['path'] for r in rows}) != 755 or len({r['sha256'] for r in rows}) != 211:
        raise ValueError('reviewed family roster differs')
    _, owner_pin = read(Path(__file__).resolve().relative_to(ROOT).as_posix())
    catalog_raw, catalog_pin = read('scripts/arbiter/analysis/upstream_tools/archive-catalog.json')
    catalog = parse(catalog_raw)
    archives = {}
    for name in ('cmake-3.31.6', 'vscode-1.145.0'):
        spec = next(x for x in catalog['archives'] if x['name'] == name)
        data, pin = read(spec['archive']['path'], ARCHIVE_LIMIT)
        if pin != spec['archive']:
            raise ValueError('reviewed official archive differs')
        authority_raw, authority_pin = read(spec['authority']['path'])
        if authority_pin != spec['authority']:
            raise ValueError('archive authority differs')
        authority = parse(authority_raw)
        for key in spec['authority_selector']:
            authority = authority[key]
        if authority != pin['sha256']:
            raise ValueError('archive differs from retained authority')
        archives[name] = (spec, zipfile.ZipFile(io.BytesIO(data)), pin, authority_pin)
    action_raw, action_pin = read('artifacts/tools/actionlint/actionlint_1.7.12_windows_amd64.zip', ARCHIVE_LIMIT)
    if action_pin['sha256'] != '6e7241b51e6817ea6a047693d8e6fed13b31819c9a0dd6c5a726e1592d22f6e9':
        raise ValueError('retained actionlint archive differs')
    action_archive = zipfile.ZipFile(io.BytesIO(action_raw))
    results = []
    try:
        for row in rows:
            path = row['path']; role = family(path); body, pin = read(path)
            if pin['sha256'] != row['sha256'] or pin['bytes'] != row['bytes']:
                raise ValueError('document changed')
            text = body.decode('utf-8', errors='strict')
            evidence = [owner_pin, source_pin]
            indicators = dict(fenced_blocks=len(re.findall(r'^```[^\n]*$', text, re.M)) // 2, literal_AIRK_marker='AIRK' in text)
            strength = 'positive-document-role-only'
            limits = 'Role established from document content and retained package/publisher metadata; not an exact registry/archive reconstruction or security verdict.'
            if role in ('cmake-doc', 'vscode-doc', 'actionlint-doc'):
                if role == 'actionlint-doc':
                    member = path.split('/actionlint/', 1)[1]; archive = action_archive; evidence += [action_pin]
                    limits = 'Exact retained actionlint archive member; this audit does not newly authenticate the publisher download.'
                else:
                    name = 'cmake-3.31.6' if role == 'cmake-doc' else 'vscode-1.145.0'
                    spec, archive, archive_pin, authority_pin = archives[name]
                    root = next(x for x in spec['roots'] if path.startswith(x + '/'))
                    member = spec['archive_prefix'] + path[len(root) + 1:]
                    evidence += [catalog_pin, archive_pin, authority_pin]
                    limits = 'Exact retained archive member authenticated by its retained publisher metadata/tool lock; no tool execution or qualification.'
                matching = [x for x in archive.infolist() if x.filename == member]
                if len(matching) != 1 or matching[0].file_size > LIMIT:
                    raise ValueError('ambiguous or oversized selected archive member')
                with archive.open(matching[0]) as stream:
                    archived = stream.read(LIMIT + 1)
                if archived != body:
                    raise ValueError('document differs from archive member')
                strength = 'exact-retained-archive-member'; indicators['archive_member'] = member
            elif role == 'rustsec-advisory':
                match = re.search(r'```toml\s*\n(.*?)\n```', text.replace('\r\n', '\n'), re.S)
                advisory = tomllib.loads(match[1])['advisory'] if match else {}
                if advisory.get('id') != Path(path).stem or not advisory.get('package') or not advisory.get('date'):
                    raise ValueError('advisory role metadata differs')
                tail = path.split('/dependency-policy/', 1)[1]
                database = tail.split('/crates/', 1)[0].split('/rust/', 1)[0]
                config_raw, config_pin = read('artifacts/dependency-policy/' + database + '/.git/config')
                if b'github.com/RustSec/advisory-db' not in config_raw:
                    raise ValueError('retained advisory origin differs')
                evidence.append(config_pin); indicators.update(advisory_id=advisory['id'], package=advisory['package'], advisory_date=str(advisory['date']))
            elif role in ('node-package-doc', 'crate-doc'):
                filename = 'package.json' if role == 'node-package-doc' else 'Cargo.toml'
                parents = [p for p in (ROOT / path).parents if p.is_relative_to(ROOT) and (p / filename).is_file()]
                if not parents:
                    raise ValueError('package metadata missing')
                metadata_raw, metadata_pin = read((parents[0] / filename).relative_to(ROOT).as_posix())
                metadata = parse(metadata_raw) if role == 'node-package-doc' else tomllib.loads(metadata_raw.decode())['package']
                if not metadata.get('name') or not metadata.get('version'):
                    raise ValueError('package identity absent')
                evidence.append(metadata_pin); indicators.update(package=metadata['name'], version=metadata['version'])
                if role == 'crate-doc':
                    _, vcs_pin = read((parents[0] / '.cargo_vcs_info.json').relative_to(ROOT).as_posix()); evidence.append(vcs_pin)
            else:
                if Path(path).name == 'merges.txt':
                    lines = text.splitlines()
                    if lines[0] != '#version: 0.2' or any(len(line.split(' ')) != 2 for line in lines[1:] if line):
                        raise ValueError('tokenizer merge table differs')
                    indicators.update(document_kind='tokenizer-BPE-merge-table', merge_rows=len(lines)-1)
                elif '/backend-research/' in path:
                    if not ('Contrib Operator Schemas' in text or 'ONNX Runtime GenAI Model Builder' in text):
                        raise ValueError('unreviewed backend document')
                    _, metadata_pin = read('artifacts/advanced-split/models/backend-research/inventory.json'); evidence.append(metadata_pin)
                    indicators['document_kind'] = 'upstream-backend-reference-document'
                elif path.endswith('/typer/SKILL.md'):
                    if not text.startswith('---\nname: typer\n'):
                        raise ValueError('unreviewed package skill document')
                    indicators['document_kind'] = 'third-party-typer-package-instructions-not-executed'
                else:
                    if not text.startswith('---') or not re.search(r'(?m)^(?:license|base_model|library_name):', text):
                        raise ValueError('publisher model-card metadata absent')
                    indicators['document_kind'] = 'publisher-model-card'
                limits = 'Positive publisher/package/reference or tokenizer-data role from the retained contents; only source identity is rehashed here, not every publisher download chain. No model execution or AIRK authorship inferred.'
            results.append(dict(source=path, source_sha256=pin['sha256'], source_bytes=pin['bytes'], role=role, evidence=evidence, content_indicators=indicators, evidence_strength=strength, limits=limits))
    finally:
        action_archive.close()
        for _, archive, _, _ in archives.values(): archive.close()
    for name, pin in checked.items():
        _, actual = capture(name, ARCHIVE_LIMIT if pin['bytes'] > LIMIT else LIMIT)
        if actual != pin:
            raise ValueError('input changed before publication')
    return dict(schema='airk.private.vendor-document-role-review/1', input=source_pin, producer=owner_pin, files=results, paths=len(results), unique_source_sha256=len({r['source_sha256'] for r in results}), role_counts=dict(Counter(r['role'] for r in results)), exact_archive_member_count=sum(r['evidence_strength']=='exact-retained-archive-member' for r in results), source_contents_emitted=False, historical_code_executed=False, qualification=False, remaining_original_population=75, whole_directory_exclusions=False)


if __name__ == '__main__':
    print(json.dumps(review(), ensure_ascii=True, separators=(',', ':')))
