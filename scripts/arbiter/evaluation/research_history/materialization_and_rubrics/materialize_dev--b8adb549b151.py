# SPDX-License-Identifier: Apache-2.0
"""Private sealed DEV adapter; no execution before reviewed input-seal release."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import threading
import time

HERE = Path(__file__).resolve().parent
BASE = HERE.parent
REPO = HERE.parents[3]
PROTOCOL_SHA = '775180b7fc872a3075d4bc392d2ad2fc93fd5dded9e3c6c60e98982d31becdc3'
AUDIT_SHA = 'e5fb7746ff06545af91dfc57496835d9d472d41fe3e1108de6ae0fbd21b51c77'
STAGE = 'initialization'
START = time.monotonic()


def sha(path):
    value = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1048576), b''):
            value.update(block)
    return value.hexdigest()


def load(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def import_file(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def choose(audit, count=64):
    """Metadata-only fixed ranking, never inspect answers or replace refusals."""
    components = [c for c in audit['components'] if c['disposition'] == 'DEV']
    if len(components) < count:
        raise ValueError('insufficient distinct DEV components')
    def component_rank(c):
        raw = bytes.fromhex(c['component_digest'])
        return hashlib.sha256(b'airk.a1.next-experiment.v1/dev64-component\0' + raw).digest(), raw
    selected = []
    for component in sorted(components, key=component_rank)[:count]:
        def question_rank(qid):
            raw = qid.encode('utf-8')
            return hashlib.sha256(b'airk.a1.next-experiment.v1/dev64-question\0' + raw).digest(), raw
        qid = min(component['question_ids'], key=question_rank)
        selected.append({'component_digest': component['component_digest'], 'question_id': qid,
                         'component_rank_sha256': component_rank(component)[0].hex(),
                         'question_rank_sha256': question_rank(qid)[0].hex()})
    if len({r['question_id'] for r in selected}) != count:
        raise ValueError('duplicate selected question IDs')
    return selected


def validate_selection(selection, audit):
    if selection['protocol_sha256'] != PROTOCOL_SHA or selection['metadata_result_sha256'] != AUDIT_SHA:
        raise ValueError('foreign protocol/metadata identity')
    if selection['selected'] != choose(audit):
        raise ValueError('selection differs from frozen exact64 ranking')


def scan_convert(raw, selected, audit, helper, clock=None):
    """Arrow may decode mixed pages; only filtered selected rows reach Python/helper."""
    import pyarrow.dataset as dataset
    wanted = {r['question_id'] for r in selected}
    if len(wanted) != len(selected):
        raise ValueError('duplicate selection')
    by_component = {c['component_digest']: set(c['article_digests']) for c in audit['components']}
    allowed = {r['question_id']: by_component[r['component_digest']] for r in selected}
    documents, tasks, exclusions, seen = {}, [], [], set()
    scanner = dataset.dataset(raw, format='parquet').scanner(
        columns=['id', 'question', 'answer', 'context', 'supporting_facts'],
        filter=dataset.field('id').isin(sorted(wanted)), batch_size=64,
        batch_readahead=0, fragment_readahead=0, use_threads=False)
    for batch in scanner.to_batches():
        rows = batch.to_pylist()
        # Validate all rows before passing any to the neutral converter.
        for row in rows:
            qid = row['id']
            if qid not in wanted or qid in seen:
                raise ValueError('unselected or duplicate row reached DEV adapter')
            titles, sentences = row['context']['title'], row['context']['sentences']
            if not isinstance(titles, list) or not 1 <= len(titles) <= 10 or len(titles) != len(sentences):
                raise ValueError('context title/sentence cardinality invalid')
            if any(not isinstance(t, str) or not t for t in titles):
                raise ValueError('invalid original context title')
            if any(not isinstance(s, list) or any(not isinstance(x, str) for x in s) for s in sentences):
                raise ValueError('invalid original sentence sequence')
            if clock is not None and not {clock.article(t) for t in titles} <= allowed[qid]:
                raise ValueError('selected row article outside its sealed DEV component')
            support = row['supporting_facts']
            if len(support['title']) != len(support['sent_id']):
                raise ValueError('original evidence title/position cardinality invalid')
            if not isinstance(row['question'], str) or not row['question'] or not isinstance(row['answer'], str):
                raise ValueError('invalid original question/answer type')
            seen.add(qid)
        helper.append_hotpot_rows(rows, wanted, documents, tasks, exclusions)
    if seen != wanted:
        raise ValueError('selected DEV rows missing; no replacement permitted')
    for task in tasks:
        task['split'] = 'development'  # Explicit experiment role; evidence/answer bytes remain unchanged.
    return {'documents': list(documents.values()), 'tasks': tasks, 'hotpot_exclusions': exclusions}


def main():
    global STAGE
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prepare', action='store_true', help='Metadata only; emit fixed selected IDs')
    parser.add_argument('--input-seal', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    clock = None
    if not args.prepare:
        clock = import_file('private_audit_clock', BASE / 'a1-next-experiment-blind-metadata-r5/audit.py')
        clock.START = START
        clock.JOB_STATE = clock.enforce_budget()
        def expire():
            if not clock.done.wait(max(0, 60 - (time.monotonic() - START))):
                os.write(2, b'REFUSED: hard60second materialization budget exhausted\n')
                os._exit(124)
        threading.Thread(target=expire, daemon=True).start()
    protocol = HERE / 'protocol.json'
    audit_path = BASE / 'a1-next-experiment-blind-metadata-r5/audit-result.json'
    if sha(protocol) != PROTOCOL_SHA or sha(audit_path) != AUDIT_SHA:
        raise ValueError('immutable protocol/audit drift')
    audit = load(audit_path)
    if args.prepare:
        path = HERE / 'dev-selection.json'
        if path.exists():
            raise ValueError('selection already frozen')
        value = {'schema': 'airk.private-component-dev-selection/1', 'protocol_sha256': PROTOCOL_SHA,
                 'metadata_result_sha256': AUDIT_SHA, 'selected': choose(audit),
                 'scope': 'ID metadata only; no DEV/CAL/FINAL content or eligibility consumed'}
        path.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')
        print(json.dumps({'status': 'PREPARED_METADATA_ONLY', 'selection_sha256': sha(path), 'count': 64}))
        return
    if args.input_seal is None or args.output is None:
        raise ValueError('reviewed input-seal and fresh private output required')
    try:
        STAGE = 'sealed-input-verification'
        seal_sha256 = sha(args.input_seal)
        seal = load(args.input_seal)
        mandatory = {Path(__file__).resolve(), protocol.resolve(), audit_path.resolve(),
                     (HERE / 'dev-selection.json').resolve(), (BASE / 'hotpot-dev.parquet').resolve(),
                     (BASE / 'a1-next-experiment-blind-metadata-r5/audit.py').resolve(),
                     (REPO / 'tools/retrieval-train/human_corpus.py').resolve(),
                     (REPO / 'tools/retrieval-train/qasper_development.py').resolve(), Path(sys.executable).resolve()}
        sealed_paths = [Path(row['path']).resolve() for row in seal['files']]
        if not mandatory <= set(sealed_paths) or len(sealed_paths) != len(set(sealed_paths)):
            raise ValueError('missing/duplicate mandatory source/runtime/selection seal')
        if seal['protocol_sha256'] != PROTOCOL_SHA or seal['selection_sha256'] != sha(HERE / 'dev-selection.json'):
            raise ValueError('seal protocol/selection differs from immutable owners')
        for row in seal['files']:
            if sha(row['path']) != row['sha256']:
                raise ValueError('input/source/runtime seal drift: ' + row['path'])
        selection = load(HERE / 'dev-selection.json')
        validate_selection(selection, audit)
        output = args.output.resolve()
        if output.exists() or output != (BASE / 'a1-component-development-materialized-r1').resolve() or output != Path(seal['output']).resolve():
            raise ValueError('fresh private output under repository artifacts required')
        # No output/source writes before every selection and seal check.
        raw = BASE / 'hotpot-dev.parquet'
        import pyarrow
        pyarrow.set_cpu_count(1)
        pyarrow.set_io_thread_count(1)
        sys.path.insert(0, str(REPO / 'tools/retrieval-train'))
        import human_corpus
        STAGE = 'filtered-selected-DEV-scan-and-neutral-conversion'
        result = scan_convert(raw, selection['selected'], audit, human_corpus, clock)
        result.update({'schema': 'airk.private-human-evidence-corpus/1', 'experiment_id': 'airk-a1-component-development-r1',
                       'protocol_sha256': PROTOCOL_SHA, 'selection_sha256': sha(HERE / 'dev-selection.json'),
                       'input_seal_sha256': sha(args.input_seal), 'private_development_only': True,
                       'training_or_calibration_permitted': False, 'release_qualified': False,
                       'limits': 'Original205FAIL unchanged; no CAL/FINAL content output; no eligibility/promotion claim.',
                       'attribution': 'Yang et al.2018 HotpotQA; Wikipedia contributors at each original article URL. CC-BY-SA4.0; private evaluation/development only; no weight training or calibration.'})
        STAGE = 'post-read-source-fences'
        for row in seal['files']:
            if sha(row['path']) != row['sha256']:
                raise ValueError('source/runtime changed during materialization')
        if sha(args.input_seal) != seal_sha256:
            raise ValueError('input seal changed during materialization')
        STAGE = 'atomic-output-fsync'
        output.mkdir(parents=True)
        clock.atomic_stream(output / 'corpus.private.json', result)
        receipt = {'schema': 'airk.private-component-dev-materialization/1', 'status': 'DEV_ONLY_PENDING_PRIMARY_REVIEW_NO_ELIGIBILITY',
                   'protocol_sha256': PROTOCOL_SHA, 'input_seal_sha256': sha(args.input_seal),
                   'corpus_sha256': sha(output / 'corpus.private.json'), 'selected_questions': len(result['tasks']),
                   'documents': len(result['documents']), 'exclusions': len(result['hotpot_exclusions']),
                   'alignment_refusals': [t['question_id'] for t in result['tasks'] if any(a['alignment'] != 'all_original_evidence_maps_to_allowed_text' for a in t['human_alternatives'])],
                   'peak_committed_process_bytes': clock.peak_memory(), 'elapsed_seconds': time.monotonic() - clock.START}
        if receipt['elapsed_seconds'] >= 60 or receipt['peak_committed_process_bytes'] > clock.LIMIT:
            raise ValueError('resource budget exhausted')
        clock.atomic_stream(output / 'materialization-receipt.json', receipt)
        print(json.dumps(receipt))
    except Exception as error:
        print(json.dumps({'status': 'REFUSED', 'stage': STAGE, 'exception_type': type(error).__name__,
                          'reason': str(error), 'elapsed_seconds': time.monotonic() - clock.START,
                          'peak_committed_process_bytes': clock.peak_memory()}), file=sys.stderr)
        raise
    finally:
        clock.done.set()


if __name__ == '__main__':
    main()
