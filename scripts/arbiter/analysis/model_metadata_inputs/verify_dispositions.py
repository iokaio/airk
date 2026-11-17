# SPDX-License-Identifier: Apache-2.0
"""Project pinned reference-page evidence; never execute or copy downloaded HTML."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath, PureWindowsPath
import re

BASE = Path(__file__).resolve().parent
ROOT = next(p for p in BASE.parents if (p / 'Cargo.toml').is_file() and (p / 'AGENTS.md').is_file())
MAX_BYTES = 16 * 1024 * 1024
PAPER = re.compile(r'[0-9]{4}\.[0-9]{4,5}')
SHA = re.compile(r'[0-9a-f]{64}')


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def relative_path(value):
    path = PurePosixPath(value)
    if path.is_absolute() or '..' in path.parts or '\\' in value or ':' in value or not str(path).startswith('artifacts/advanced-split/models/'):
        raise ValueError('reference path escaped reviewed model-input roots')
    return path.as_posix()


def filesystem_read(relative):
    relative = relative_path(relative)
    path = (ROOT / relative).resolve(strict=True)
    if not path.is_relative_to(ROOT / 'artifacts/advanced-split/models') or not path.is_file() or path.stat().st_size > MAX_BYTES:
        raise ValueError('reference input escaped root or exceeded source bound')
    raw = path.read_bytes()
    if len(raw) > MAX_BYTES:
        raise ValueError('reference input grew beyond source bound')
    return raw


def pinned(pin, read):
    raw = read(relative_path(pin['path']))
    if not SHA.fullmatch(pin['sha256']) or digest(raw) != pin['sha256'] or (pin.get('bytes') is not None and len(raw) != pin['bytes']):
        raise ValueError('pinned reference evidence differs')
    return raw


def family_rows(family, read=filesystem_read):
    if type(family['expected_files']) is not int or not 0 < family['expected_files'] <= 10000:
        raise ValueError('invalid family count')
    evidence = family['evidence']
    if not evidence:
        raise ValueError('reference family lacks evidence')
    captured = [pinned(pin, read) for pin in evidence]
    source_root = relative_path(family['source_root'])
    kind = family['kind']
    rows = []
    seen = set()

    def append(name, expected, url, size=None, gap=None):
        if not name.endswith('.html') or PurePosixPath(name).name != name or name in seen:
            raise ValueError('duplicate or unsafe reference filename')
        seen.add(name)
        source = relative_path(source_root + '/' + name)
        raw = read(source)
        if not SHA.fullmatch(expected) or digest(raw) != expected or (size is not None and len(raw) != size):
            raise ValueError('reference source differs from pinned evidence')
        if not url.startswith('https://') or (gap is not None and (not isinstance(gap, str) or not gap.strip())):
            raise ValueError('invalid reference URL/provenance gap')
        rows.append({'source': source, 'source_sha256': expected, 'source_bytes': len(raw),
                     'disposition': 'upstream_reference_document',
                     'reason': 'Downloaded publisher/rights metadata used as research input evidence, not AIRK authored implementation; no HTML payload copied or executed and no rights adoption inferred.',
                     'evidence': evidence,
                     'reference': {'url': url, 'content_sha256': expected, 'provenance_gap': gap}})

    if kind == 'qasper_bulk':
        packet = json.loads(captured[0])
        for record in packet['records']:
            identifier = record['paper_id']
            if not PAPER.fullmatch(identifier):
                raise ValueError('invalid arXiv identifier')
            append('arxiv-' + identifier + '.html', record['metadata_sha256'], 'https://arxiv.org/abs/' + identifier)
    elif kind == 'qasper_versioned':
        packet = json.loads(captured[0])
        if packet.get('metadata_only') is not True or packet.get('no_corpus_or_paper_text_adoption') is not True:
            raise ValueError('collection is not metadata-only reference evidence')
        for record in packet['requests']:
            identifier, version = record['paper_id'], record['version']
            if not PAPER.fullmatch(identifier) or not re.fullmatch('[1-9][0-9]*', version):
                raise ValueError('invalid versioned arXiv identifier')
            name = identifier + 'v' + version + '.html'
            url = 'https://arxiv.org/abs/' + identifier + 'v' + version
            if PureWindowsPath(record['file']).name != name or record['url'] != url or record['final_url'] != url:
                raise ValueError('acquisition filename/URL differs')
            append(name, record['sha256'], url, record['bytes'])
    elif kind == 'hotpot_reference':
        reference = family['reference']
        if not reference.get('provenance_gap'):
            raise ValueError('missing acquisition receipt must remain explicit')
        append('publisher-homepage.html', reference['content_sha256'], reference['url'], reference['bytes'], reference['provenance_gap'])
    else:
        raise ValueError('unknown reference family')
    if len(rows) != family['expected_files']:
        raise ValueError('reference family count differs')
    return rows


def project(definitions, read=filesystem_read):
    if definitions.get('schema') != 'airk.model-metadata-reference-families/1' or type(definitions['expected_files']) is not int or not 0 < definitions['expected_files'] <= 10000:
        raise ValueError('invalid projection definitions/count')
    rows = [row for family in definitions['families'] for row in family_rows(family, read)]
    if len(rows) != definitions['expected_files'] or len({r['source'] for r in rows}) != len(rows):
        raise ValueError('complete reference roster differs')
    return {'schema': 'airk.source-disposition/1', 'files': sorted(rows, key=lambda r: r['source'])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--emit', action='store_true', help='emit full standard disposition JSON to stdout; retain privately under a guarded parent')
    args = parser.parse_args()
    definitions = json.loads((BASE / 'families.json').read_bytes())
    result = project(definitions)
    if args.emit:
        print(json.dumps(result, indent=2))
    else:
        print(json.dumps({'verified_reference_documents': len(result['files']), 'family_count': len(definitions['families']),
                          'explicit_acquisition_receipt_gaps': sum(r['reference']['provenance_gap'] is not None for r in result['files']),
                          'authored_code_copied': False, 'html_executed_or_copied': False}, sort_keys=True))


if __name__ == '__main__':
    main()
