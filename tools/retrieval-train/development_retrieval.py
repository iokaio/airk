# SPDX-License-Identifier: Apache-2.0
"""Explicit DEV64 preparation/grading; never changes the frozen205 campaign."""
import argparse
from collections import Counter
from fractions import Fraction
import hashlib
import importlib.util
import os
import sys
import threading
import time
import json
from pathlib import Path

from grade_native_retrieval import evidence_recall, cluster_interval
from goal_sources import SourceLedger, FrozenSourceMetadata

ROOT = Path(__file__).resolve().parents[2]
CORPUS_SHA = 'f6c6616c1de29948deee9f37c70ccaadbb7a5dc71d0cd02c91d207c9ccbac097'
MASK_SHA = '76842592dab174fd90bc3918542759adbf9cea615080334a308acafd6924727d'
PROTOCOL_SHA = '0fa7ffbfe381d0804704710e683f85a897d59a661073ba24e849fe53b3e298e8'
ENCODERS_SHA = '3ead240726ebdba10577db6778bffbf5232eab10c50f2ae128d589e0a961ab81'
FAMILIES = frozenset(['trained-v2-u8', 'trained-v2-s8', 'original-shipped-u8-ort130-evaluation',
                     'original-shipped-s8-ort130-evaluation', 'original-mixed-u8-ort130-evaluation',
                     'original-mixed-s8-ort130-evaluation'])


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def mask_sets(mask):
    quality = {'hotpot:' + q for q in mask['quality_question_ids']}
    refused = {'hotpot:' + q for q in mask['refused_question_ids']}
    if len(quality) != 20 or len(refused) != 44 or quality & refused:
        raise ValueError('exact disjoint20/44 frozen mask required')
    return quality, refused


def build_fixture(corpus, mask, original_rows, output):
    """Exact text/ranges only; original_rows MUST already be filtered DEV64."""
    quality, refused = mask_sets(mask)
    expected = quality | refused
    tasks = {t['id']: t for t in corpus['tasks']}
    if len(corpus['tasks']) != 64 or set(tasks) != expected or len(corpus['documents']) != 366:
        raise ValueError('fixed64 tasks/366 documents required')
    if {t['split'] for t in tasks.values()} != {'development'}:
        raise ValueError('onlyDEV task roles allowed')
    rows = {r['id']: r for r in original_rows}
    if len(original_rows) != 64 or {'hotpot:' + q for q in rows} != expected:
        raise ValueError('filtered DEV64 source rows missing/duplicate/foreign')
    files, paragraphs, bodies, by_id = [], [], {}, {}
    groups = set()
    for doc in corpus['documents']:
        if doc['source_group'] in groups:
            raise ValueError('multiple publisher versions for one article family')
        groups.add(doc['source_group'])
        body = doc['text'].encode('utf-8')
        if sha(body) != doc['paragraph_sha256'] or doc['id'] != 'hotpot:' + sha((doc['title'] + '\n').encode() + body):
            raise ValueError('original document identity differs')
        file = 'sources/hotpot/' + sha(doc['source_group'].encode()) + '.txt'
        paragraph = {'id': doc['id'], 'source_group': doc['source_group'], 'file': file,
                     'paragraph_sha256': sha(body), 'byte_start': 0, 'byte_end': len(body),
                     'line_start': 1, 'line_end': body.count(b'\n') + 1,
                     'title': doc['title'], 'original_url': doc['url'], 'license': doc['license']}
        paragraphs.append(paragraph)
        by_id[doc['id']] = paragraph
        bodies[file] = body
        files.append({'source_group': doc['source_group'], 'file': file, 'bytes': len(body),
                      'sha256': sha(body), 'paragraphs': [paragraph]})
    rubric = []
    for identifier, task in sorted(tasks.items()):
        original = rows[task['question_id']]
        if original.get('question', task['question']) != task['question']:
            raise ValueError('original question changed')
        alternatives = []
        for alternative in task['human_alternatives']:
            facts = []
            for fact in alternative['evidence']:
                locations = []
                for source_id in fact['allowed_locations']:
                    paragraph = by_id[source_id]
                    title_index = original['context']['title'].index(fact['source_title'])
                    sentences = original['context']['sentences'][title_index]
                    index = fact['sentence_index']
                    sentence = sentences[index].encode('utf-8')
                    if sha(sentence) != fact['source_evidence_sha256'] or not sentence:
                        raise ValueError('original support sentence bytes differ/empty')
                    start = len(''.join(sentences[:index]).encode('utf-8'))
                    end = start + len(sentence)
                    if bodies[paragraph['file']][start:end] != sentence:
                        raise ValueError('support interval differs from exact indexed bytes')
                    locations.append({'file': paragraph['file'], 'byte_start': start, 'byte_end': end,
                                      'original_url': paragraph['original_url'], 'source_group': paragraph['source_group']})
                facts.append({'source_title': fact['source_title'], 'sentence_index': fact['sentence_index'],
                              'locations': locations})
            if alternative['original_evidence_count'] != len(facts):
                raise ValueError('original support cardinality changed')
            alternatives.append({'human_annotation_id': alternative['human_annotation_id'], 'facts': facts,
                                 'original_human_answer': alternative['original_human_answer'],
                                 'alignment': alternative['alignment']})
        if identifier in quality and (not alternatives or any(not a['facts'] or any(not f['locations'] for f in a['facts']) for a in alternatives)):
            raise ValueError('quality mask includes missing/empty original support')
        rubric.append({'id': identifier, 'dataset': 'HotpotQA', 'question': task['question'],
                       'source_root': str((output / 'sources/hotpot').resolve()),
                       'quality_status': 'ALIGNED' if identifier in quality else 'NOT_EVALUABLE',
                       'alternatives': alternatives})
    return bodies, {'schema': 'airk.private-native-human-fixtures/1', 'files': sorted(files, key=lambda x: x['file']),
                    'paragraphs': paragraphs, 'corpus_sha256': CORPUS_SHA, 'quality_mask_sha256': MASK_SHA,
                    'development_protocol_sha256': PROTOCOL_SHA, 'release_qualified': False,
                    'scope': 'DEV64 only,366 exact supplied context excerpts; no final/goal selection or model result'}, rubric


def summarize_quality(rows, quality, refused):
    if len(rows) != 64 or {r['id'] for r in rows} != quality | refused:
        raise ValueError('all64 unique native rows required; never drop/refill')
    aligned = [r for r in rows if r['id'] in quality]
    if len(aligned) != 20 or any(r['quality_status'] != 'NOT_EVALUABLE' or r['evidence_recall_fraction'] is not None or r['complete_original_alternative'] is not None for r in rows if r['id'] in refused):
        raise ValueError('refusals cannot become quality successes')
    mean = sum((Fraction(*r['evidence_recall_fraction']) for r in aligned), Fraction(0)) / 20
    complete = sum(r['complete_original_alternative'] for r in aligned)
    states = Counter(r.get('semantic_state', 'unreported') for r in rows)
    return {'all_requests': 64, 'semantic_states': dict(states), 'all64_native_ready': states['ready'] == 64, 'quality_denominator': 20, 'alignment_refusals': 44,
            'aligned_coverage_fraction': [20, 64], 'mean_evidence_recall_fraction': [mean.numerator, mean.denominator],
            'mean_evidence_recall': float(mean), 'complete_count': complete, 'complete_recall': complete / 20,
            'coverage_inclusive_complete_lower_bound_fraction': [complete, 64],
            'promotion_or_final_quality_pass': False, 'scope': 'DEV diagnostic only; original205FAIL unchanged'}


def grade_family(family, rubric, mask, fixture, manifest_sha, metadata_cache=None):
    quality, refused = mask_sets(mask)
    tasks = {t['id']: t for t in rubric}
    if family.get('complete') is not True or len(family['records']) != 64 or {r['id'] for r in family['records']} != quality | refused:
        raise ValueError('incomplete or foreign nativeDEV64 capture')
    rows = []
    for record in family['records']:
        task = tasks[record['id']]
        if not Path(record['source_root']).samefile(task['source_root']):
            raise ValueError('native scope differs from sharedDEV collection')
        value = record['primary_vector']
        if 'collections' in family and not any(c['publication'] == value.get('publication') and Path(c['source_root']).samefile(task['source_root']) for c in family['collections']):
            raise ValueError('native query publication differs from captured collection')
        if type(record.get('query_us')) is not int or record['query_us'] < 0:
            raise ValueError('native per-request timing absent')
        if len(value['hits']) > 10:
            raise ValueError('native top10 exceeded')
        ledger = SourceLedger(fixture, task['source_root'], manifest_sha, metadata_cache=metadata_cache)
        ledger.observe(value)
        spans = list(ledger.records.values())
        if task['id'] in quality:
            recall, complete = evidence_recall(task, spans)
            fraction = [recall.numerator, recall.denominator]
        else:
            fraction, complete = None, None
        rows.append({'id': task['id'], 'quality_status': task['quality_status'],
                     'evidence_recall_fraction': fraction, 'complete_original_alternative': complete,
                     'original_evidence_counts': [len(a['facts']) for a in task['alternatives']],
                     'source_spans': spans, 'semantic_state': value.get('semantic_readiness', {}).get('state', 'unreported'),
                     'native_truncated': value.get('truncated'), 'native_record': record})
    aligned = sorted((r for r in rows if r['id'] in quality), key=lambda r: r['id'])
    summary = summarize_quality(rows, quality, refused)
    summary['descriptive_mean_95_interval'] = cluster_interval([float(Fraction(*r['evidence_recall_fraction'])) for r in aligned], [r['id'] for r in aligned])
    summary['descriptive_complete_95_interval'] = cluster_interval([r['complete_original_alternative'] for r in aligned], [r['id'] for r in aligned])
    return {'family': family['family'], 'summary': summary, 'rows': rows}


def verify_fixture(fixture, metadata):
    fixture = fixture.resolve()
    expected = {r['file']: r for r in metadata['files']}
    actual = set()
    def walk(path):
        if path.is_symlink() or getattr(path, 'is_junction', lambda: False)():
            raise ValueError('fixture alias refused')
        if path.is_dir():
            for child in path.iterdir():
                walk(child)
        elif path.is_file():
            name = path.relative_to(fixture).as_posix()
            actual.add(name)
        else:
            raise ValueError('fixture special file refused')
    walk(fixture / 'sources')
    if actual != set(expected):
        raise ValueError('source fixture extra/missing files')
    for name, row in expected.items():
        path = fixture / name
        if path.resolve() != path or not path.resolve().is_relative_to(fixture / 'sources'):
            raise ValueError('fixture path escape')
        raw = path.read_bytes()
        if len(raw) != row['bytes'] or sha(raw) != row['sha256']:
            raise ValueError('source fixture bytes changed')


def prepare(args):
    for path, expected in [(args.corpus, CORPUS_SHA), (args.mask, MASK_SHA), (args.protocol, PROTOCOL_SHA)]:
        if sha(path.read_bytes()) != expected:
            raise ValueError('fixed DEV input identity drift')
    output = args.output.resolve()
    if output.exists() or not output.is_relative_to(ROOT / 'artifacts'):
        raise ValueError('fresh private output required')
    corpus, mask = read(args.corpus), read(args.mask)
    quality, refused = mask_sets(mask)
    import pyarrow
    import pyarrow.dataset as dataset
    pyarrow.set_cpu_count(1)
    pyarrow.set_io_thread_count(1)
    if sha(args.hotpot.read_bytes()) != 'c20b638ca82b21d04fe12e14ff417ad05153d4d215a65de54497fca4e972f7c6':
        raise ValueError('raw publisher source changed')
    ids = sorted(q.split(':', 1)[1] for q in quality | refused)
    scanner = dataset.dataset(args.hotpot, format='parquet').scanner(columns=['id', 'context'],
        filter=dataset.field('id').isin(ids), batch_size=64, batch_readahead=0, fragment_readahead=0, use_threads=False)
    selected = [row for batch in scanner.to_batches() for row in batch.to_pylist()]
    bodies, manifest, rubric = build_fixture(corpus, mask, selected, output)
    # Result closure must be reviewed before native execution. No .airk files here.
    output.mkdir(parents=True)
    for file, body in bodies.items():
        path = output / file
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(body)
    CLOCK.atomic_stream(output / 'manifest.private.json', manifest)
    CLOCK.atomic_stream(output / 'rubric.private.json', {'tasks': rubric, 'quality_mask_sha256': MASK_SHA})
    print(json.dumps({'prepared_DEV_only': True, 'files': len(bodies), 'requests': len(rubric),
                      'manifest_sha256': sha((output / 'manifest.private.json').read_bytes())}))


def grade(args):
    if sha(args.mask.read_bytes()) != MASK_SHA or sha(args.encoders.read_bytes()) != ENCODERS_SHA:
        raise ValueError('mask/encoder identity changed')
    manifest = args.fixture / 'manifest.private.json'
    manifest_sha = sha(manifest.read_bytes())
    if manifest_sha != args.manifest_sha256:
        raise ValueError('unreviewedDEV source closure')
    metadata = read(manifest)
    if metadata['corpus_sha256'] != CORPUS_SHA or metadata['quality_mask_sha256'] != MASK_SHA or len(metadata['files']) != 366:
        raise ValueError('foreignDEV fixture')
    verify_fixture(args.fixture, metadata)
    rubric_path = args.fixture / 'rubric.private.json'
    if sha(rubric_path.read_bytes()) != args.rubric_sha256:
        raise ValueError('unreviewedDEV rubric')
    families = [read(p) for p in args.captures]
    encoders = {x['family']: x for x in read(args.encoders)}
    if len(families) != 6 or {x['family'] for x in families} != FAMILIES:
        raise ValueError('exact six matched native families required')
    if len({f['binary_sha256'] for f in families}) != 1 or len({f['runtime'] for f in families}) != 1:
        raise ValueError('native families do not share exact captured binary/runtime')
    for family in families:
        if family['source_before_sha256'] != family['source_after_sha256'] or family['source_roster_count'] != 366 or family['development_protocol_sha256'] != PROTOCOL_SHA:
            raise ValueError('native source closure/protocol changed')
        if family['corpus_sha256'] != CORPUS_SHA or family['quality_mask_sha256'] != MASK_SHA or family['source_manifest_sha256'] != manifest_sha or family['encoder_input_sha256'] != ENCODERS_SHA or family['model_json_sha256'] != encoders[family['family']]['model_json_sha256']:
            raise ValueError('native model/input/fixture identity changed')
        if family['query'] != {'mode': 'vector', 'limit': 10, 'max_bytes': 32768, 'full_chunks': True}:
            raise ValueError('native query changed')
    metadata_cache = FrozenSourceMetadata(args.fixture, manifest_sha)
    result = {'schema': 'airk.private-native-DEV64-grading/1', 'release_qualified': False,
              'promotion_or_final_quality_pass': False, 'mask_sha256': MASK_SHA,
              'capture_sha256': {str(p): sha(p.read_bytes()) for p in args.captures},
              'families': [grade_family(f, read(rubric_path)['tasks'], read(args.mask), args.fixture, manifest_sha, metadata_cache) for f in families]}
    by_family = {f['family']: f for f in result['families']}
    result['paired_DEV_comparisons'] = []
    for candidate in ['trained-v2-u8', 'trained-v2-s8']:
        for control in ['original-shipped-u8-ort130-evaluation', 'original-mixed-u8-ort130-evaluation']:
            a, z = by_family[candidate]['summary'], by_family[control]['summary']
            gain = Fraction(a['complete_count'] - z['complete_count'], 20)
            delta = Fraction(*a['mean_evidence_recall_fraction']) - Fraction(*z['mean_evidence_recall_fraction'])
            result['paired_DEV_comparisons'].append({'candidate': candidate, 'control': control,
                'complete_gain_fraction': [gain.numerator, gain.denominator],
                'mean_delta_fraction': [delta.numerator, delta.denominator],
                'descriptive_DEV_paired_reference_met': gain >= Fraction(3, 100) and delta >= Fraction(-2, 100),
                'final_or_promotion_pass': False})
    verify_fixture(args.fixture, metadata)
    if args.output.exists() or not args.output.resolve().is_relative_to(ROOT / 'artifacts'):
        raise ValueError('fresh private grading output required')
    args.output.mkdir(parents=True)
    CLOCK.atomic_stream(args.output / 'grading.private.json', result)
    print(json.dumps({'development_only': True, 'families': [x['summary'] for x in result['families']]}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    prep = sub.add_parser('prepare')
    for name in ('corpus', 'mask', 'protocol', 'hotpot', 'output'):
        prep.add_argument('--' + name, type=Path, required=True)
    grading = sub.add_parser('grade')
    for name in ('fixture', 'mask', 'encoders', 'output'):
        grading.add_argument('--' + name, type=Path, required=True)
    grading.add_argument('--manifest-sha256', required=True)
    grading.add_argument('--rubric-sha256', required=True)
    grading.add_argument('--captures', nargs=6, type=Path, required=True)
    args = parser.parse_args()
    global CLOCK
    started = time.monotonic()
    helper = ROOT / 'artifacts/advanced-split/models/a1-next-experiment-blind-metadata-r5/audit.py'
    if sha(helper.read_bytes()) != '9c540c08c2e56d868059ab8425193e52f0d9141f75d16df01f622c679032425b':
        raise ValueError('reviewed hard-budget helper absent/changed')
    spec = importlib.util.spec_from_file_location('dev_bounded_runtime', helper)
    CLOCK = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(CLOCK)
    CLOCK.START = started
    CLOCK.JOB_STATE = CLOCK.enforce_budget()
    def expire():
        if not CLOCK.done.wait(max(0, 60 - (time.monotonic() - started))):
            os.write(2, b'REFUSED: development preparation/grading60second budget exhausted\n')
            os._exit(124)
    threading.Thread(target=expire, daemon=True).start()
    input_paths = [Path(__file__), Path(__file__).with_name('goal_sources.py'), Path(__file__).with_name('goal_rubric.py'), Path(__file__).with_name('grade_native_retrieval.py')]
    input_paths += [value for value in vars(args).values() if isinstance(value, Path) and value.is_file()]
    if args.command == 'grade':
        input_paths += args.captures + [args.fixture / 'manifest.private.json', args.fixture / 'rubric.private.json']
    before = {str(p): sha(p.read_bytes()) for p in input_paths}
    try:
        if args.command == 'prepare':
            prepare(args)
        else:
            grade(args)
        if any(sha(Path(p).read_bytes()) != expected for p, expected in before.items()):
            raise ValueError('source/input drift during DEV operation')
        receipt = {'schema': 'airk.private-DEV64-operation/1', 'command': args.command,
                   'source_input_sha256': before, 'elapsed_seconds': time.monotonic() - started,
                   'peak_committed_process_bytes': CLOCK.peak_memory(), 'hard_bytes': 268435456,
                   'hard_seconds': 60, 'threads': 1, 'no_model_or_native_execution': True}
        CLOCK.atomic_stream(args.output / 'operation-receipt.json', receipt)
        print(json.dumps(receipt))
    except Exception as error:
        print(json.dumps({'status': 'REFUSED', 'exception_type': type(error).__name__, 'reason': str(error),
                          'elapsed_seconds': time.monotonic() - started, 'peak_committed_process_bytes': CLOCK.peak_memory()}), file=sys.stderr)
        raise
    finally:
        CLOCK.done.set()


if __name__ == '__main__':
    main()
