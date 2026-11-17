# SPDX-License-Identifier: Apache-2.0
"""Explicit opaque FINAL fixture/export and grading; never launches inference."""
import argparse
from collections import Counter
from fractions import Fraction
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import threading
import types

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
CORPUS_SHA = '7c579edd13c85b0bdadfe8308bca2a427c5364bc9abab6d1e753b313e9769d5c'
ROLES = frozenset(('candidate-u8', 'candidate-s8', 'control-shipped-u8', 'control-mixed-u8',
                   'diagnostic-shipped-s8', 'diagnostic-mixed-s8'))


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def require(condition):
    if not condition:
        raise ValueError('opaque_final_evaluation_contract_refused')


def import_retained(name, path, raw):
    module = types.ModuleType(name)
    module.__file__ = str(path)
    exec(compile(raw, str(path), 'exec'), module.__dict__)
    return module


def import_helper_closure(specifications):
    # Force the entire authenticated sibling closure, never cached modules or
    # path reopens. Restore unrelated process module state even on failure.
    missing = object()
    previous = {name: sys.modules.get(name, missing) for name, path, raw in specifications}
    modules = {}
    try:
        for name, path, raw in specifications:
            module = types.ModuleType(name)
            module.__file__ = str(path)
            sys.modules[name] = module
            exec(compile(raw, str(path), 'exec'), module.__dict__)
            modules[name] = module
        return modules
    finally:
        for name, original in previous.items():
            if original is missing:
                sys.modules.pop(name, None)
            else:
                sys.modules[name] = original


def atomic(path, raw):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('xb') as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + '\n').encode()


def summarize(rows):
    require(len(rows) == 205 and len({r['id'] for r in rows}) == 205)
    require(Counter(r['dataset'] for r in rows) == {'HotpotQA': 171, 'QASPER': 34})
    fractions = [Fraction(*r['evidence_recall_fraction']) for r in rows]
    require(all(0 <= f <= 1 for f in fractions))
    require(all(type(r['complete_original_alternative']) is int and r['complete_original_alternative'] in (0, 1) for r in rows))
    mean = sum(fractions, Fraction(0)) / 205
    require(all(r['complete_original_alternative'] == int(f == 1) for r, f in zip(rows, fractions)))
    complete = sum(r['complete_original_alternative'] for r in rows)
    ready = sum(r['semantic_state'] == 'ready' for r in rows)
    return {'questions': 205, 'mean_evidence_recall_fraction': [mean.numerator, mean.denominator],
            'complete_count': complete, 'all_native_semantic_ready': ready == 205,
            'absolute_quality_pass': mean >= Fraction(85, 100) and complete * 100 >= 70 * 205 and ready == 205,
            'dataset_families': {name: {'questions': sum(r['dataset'] == name for r in rows),
                                      'complete_count': sum(r['complete_original_alternative'] for r in rows if r['dataset'] == name),
                                      'mean_evidence_recall_fraction': (lambda f: [f.numerator, f.denominator])(sum((Fraction(*r['evidence_recall_fraction']) for r in rows if r['dataset'] == name), Fraction(0)) / sum(r['dataset'] == name for r in rows))}
                                 for name in ('HotpotQA', 'QASPER')}}


def prepare(args, protocol, retained, guard):
    corpus_path = ROOT / protocol['corpus']['path']
    raw = corpus_path.read_bytes()
    require(sha(raw) == CORPUS_SHA == protocol['corpus']['sha256'])
    # This is opaque execution only. Developer/reviewer never opens this payload.
    corpus = json.loads(raw)
    require(len(corpus['documents']) == 1685)
    anchor = ROOT / protocol['fixture_anchor']
    output = anchor / 'materialized'
    observer = lambda path: guard.anchor_observation(path, retained[ROOT / protocol['anchor_observer']['path']])
    proof, identity = guard.guard_output(anchor, output, [corpus_path, HERE], observer)
    fixture_owner = guard.import_bytes('fresh_final_fixture_owner', HERE / 'fixture.py', retained[HERE / 'fixture.py'])
    bodies, manifest, rubric = fixture_owner.build(corpus, output)
    require(len(rubric) == 205 and len(manifest['goal_ids']) == 100)
    manifest.update(corpus_sha256=CORPUS_SHA, fresh_final_protocol_sha256=args.protocol_sha256,
                    task_count=205, original_paragraph_count=1685,
                    goal_selection="Existing A0 first100 SHA256('airk.goal.final.v1:'+ID), no outcome selection",
                    usage='FINAL evaluation only; never training/CAL/threshold/candidate selection',
                    qasper_rights_decisions=corpus['qasper_rights_decisions'],
                    qasper_rights_evidence_private=corpus['qasper_rights_evidence_private'])
    for path, expected in retained.items():
        require(path.read_bytes() == expected)
    require(corpus_path.read_bytes() == raw)
    require(observer(anchor) == proof and (anchor.stat().st_dev, anchor.stat().st_ino) == identity)
    output.mkdir(exist_ok=False)
    for name, body in bodies.items():
        atomic(output / name, body)
    atomic(output / 'manifest.private.json', encoded(manifest))
    atomic(output / 'rubric.private.json', encoded({'tasks': rubric, 'corpus_sha256': CORPUS_SHA,
                                                 'fresh_final_protocol_sha256': args.protocol_sha256}))
    report = {'complete': True, 'tasks': 205, 'hotpot': 171, 'qasper': 34,
              'original_paragraphs': 1685, 'source_files': len(bodies), 'goal_tasks': 100,
              'manifest_sha256': sha((output / 'manifest.private.json').read_bytes()),
              'rubric_sha256': sha((output / 'rubric.private.json').read_bytes()),
              'corpus_sha256': CORPUS_SHA, 'native_or_quality_qualification': False,
              'gold_or_questions_opened_by_developer': False}
    atomic(output / 'opaque-counts.json', encoded(report))
    print(json.dumps(report), flush=True)


def grade(args, protocol, retained, guard):
    # No self-asserted mutable flag authorizes a FINAL run: primary reviews the
    # exact freeze bytes/hash before launching native capture or this observer.
    freeze_raw = args.candidate_freeze.read_bytes()
    require(sha(freeze_raw) == args.candidate_freeze_sha256)
    freeze = json.loads(freeze_raw)
    require(freeze['schema'] == 'airk.private-final-candidate-export-cal-freeze/1')
    require(freeze['status'] == 'FROZEN_BEFORE_ANY_FRESH_FINAL_MODEL_OUTCOME')
    roles = freeze['families']
    require(len(roles) == 6 and {r['role'] for r in roles} == ROLES and len({r['family'] for r in roles}) == 6)
    require(freeze['candidate_selection_sha256'] and freeze['calibration_sha256'] and freeze['native_binary_sha256'])
    fixture = args.fixture.resolve(strict=True)
    manifest_raw = (fixture / 'manifest.private.json').read_bytes()
    rubric_raw = (fixture / 'rubric.private.json').read_bytes()
    require(sha(manifest_raw) == args.manifest_sha256 and sha(rubric_raw) == args.rubric_sha256)
    manifest, rubric = json.loads(manifest_raw), json.loads(rubric_raw)
    require(manifest['fresh_final_protocol_sha256'] == rubric['fresh_final_protocol_sha256'] == args.protocol_sha256)
    tasks = {t['id']: t for t in rubric['tasks']}
    require(len(tasks) == len(rubric['tasks']) == 205)
    helper_root = ROOT / 'tools/retrieval-train'
    helpers = import_helper_closure([(name, helper_root / (name + '.py'), retained[helper_root / (name + '.py')])
                                     for name in ('goal_rubric', 'goal_sources', 'grade_native_retrieval')])
    evidence_recall, cluster_interval = helpers['grade_native_retrieval'].evidence_recall, helpers['grade_native_retrieval'].cluster_interval
    SourceLedger, FrozenSourceMetadata = helpers['goal_sources'].SourceLedger, helpers['goal_sources'].FrozenSourceMetadata
    metadata = FrozenSourceMetadata(fixture, args.manifest_sha256)
    require(len(args.captures) == 6 and len({guard.regular(p).resolve() for p in args.captures}) == 6)
    anchor = ROOT / protocol['grading_anchor']
    require(args.output.resolve() == anchor / 'materialized')
    observe = lambda path: guard.anchor_observation(path, retained[ROOT / protocol['anchor_observer']['path']])
    proof, identity = guard.guard_output(anchor, args.output, [fixture, HERE], observe)
    require(observe(anchor) == proof and (anchor.stat().st_dev, anchor.stat().st_ino) == identity)
    args.output.mkdir(exist_ok=False)
    native_hashes, results, seen = {}, {}, set()
    models = {r['family']: r for r in roles}
    for capture_path in args.captures:
        raw = capture_path.read_bytes()
        native_hashes[capture_path] = sha(raw)
        family = json.loads(raw)
        require(family['family'] in models and family['family'] not in seen)
        seen.add(family['family'])
        model = models[family['family']]
        require(family['schema'] == 'airk.native-fresh-final-retrieval-family/1')
        require(family['complete'] is True and family['corpus_sha256'] == CORPUS_SHA)
        require(family['source_roster_count'] == len(manifest['files']))
        require(family['fresh_final_protocol_sha256'] == args.protocol_sha256)
        require(family['candidate_freeze_sha256'] == args.candidate_freeze_sha256)
        require(family['source_manifest_sha256'] == args.manifest_sha256)
        require(family['binary_sha256'] == freeze['native_binary_sha256'])
        require(family['model_json_sha256'] == model['model_json_sha256'] and family['pipeline_digest'] == model['pipeline_digest'])
        require(family['encoder_input_sha256'] == model['encoder_input_sha256'] and family['runtime'] == model['runtime'])
        require(family['source_before_sha256'] == family['source_after_sha256'] == family['source_roster_sha256'])
        require(family['query'] == {'mode': 'vector', 'limit': 10, 'max_bytes': 32768, 'full_chunks': True})
        records = family['records']
        require(len(records) == 205 and len({r['id'] for r in records}) == 205 and {r['id'] for r in records} == set(tasks))
        rows = []
        for record in records:
            task = tasks[record['id']]
            require(Path(record['source_root']).samefile(task['source_root']))
            require(type(record['query_us']) is int and record['query_us'] >= 0)
            value = record['primary_vector']
            require(len(value['hits']) <= 10)
            require(any(c['publication'] == value['publication'] and Path(c['source_root']).samefile(task['source_root']) for c in family['collections']))
            ledger = SourceLedger(fixture, task['source_root'], args.manifest_sha256, metadata_cache=metadata)
            ledger.observe(value)
            answerable = [a for a in task['alternatives'] if not a['unanswerable']]
            require(answerable)
            recall, complete = evidence_recall({'alternatives': answerable}, list(ledger.records.values()))
            rows.append({'id': task['id'], 'dataset': task['dataset'], 'source_component': task['bootstrap_source_component'],
                         'evidence_recall_fraction': [recall.numerator, recall.denominator],
                         'complete_original_alternative': complete,
                         'semantic_state': value.get('semantic_readiness', {}).get('state', 'unreported'),
                         'native_record': record, 'source_spans': list(ledger.records.values())})
        rows.sort(key=lambda r: r['id'])
        summary = summarize(rows)
        summary['descriptive_mean_95_interval'] = cluster_interval([float(Fraction(*r['evidence_recall_fraction'])) for r in rows], [r['source_component'] for r in rows])
        details = {'family': model['family'], 'summary': summary, 'rows': rows,
                   'native_capture_sha256': native_hashes[capture_path]}
        details_path = args.output / (model['role'] + '.private.json')
        atomic(details_path, encoded(details))
        scalar_rows = [{key: row[key] for key in ('id', 'dataset', 'source_component', 'evidence_recall_fraction',
                                                'complete_original_alternative', 'semantic_state')} for row in rows]
        results[model['role']] = {'family': model['family'], 'summary': summary, 'rows': scalar_rows,
                                 'details_sha256': sha(details_path.read_bytes())}
        del details, rows, records, family, raw, record, value, ledger
    require(seen == set(models))
    comparisons = []
    for candidate in ('candidate-u8', 'candidate-s8'):
        for control in ('control-shipped-u8', 'control-mixed-u8'):
            current, baseline = results[candidate]['summary'], results[control]['summary']
            gain = current['complete_count'] - baseline['complete_count']
            delta = Fraction(*current['mean_evidence_recall_fraction']) - Fraction(*baseline['mean_evidence_recall_fraction'])
            comparisons.append({'candidate': candidate, 'control': control, 'complete_gain_count': gain,
                                'mean_delta_fraction': [delta.numerator, delta.denominator],
                                'paired_floor_pass': gain * 100 >= 3 * 205 and delta >= Fraction(-2, 100)
                                and current['all_native_semantic_ready'] and baseline['all_native_semantic_ready'],
                                'descriptive_complete_gain_95_interval': cluster_interval([a['complete_original_alternative'] - b['complete_original_alternative'] for a, b in zip(results[candidate]['rows'], results[control]['rows'])], [r['source_component'] for r in results[candidate]['rows']])})
    for path, raw in {**retained, args.candidate_freeze: freeze_raw,
                      fixture / 'manifest.private.json': manifest_raw, fixture / 'rubric.private.json': rubric_raw}.items():
        require(path.read_bytes() == raw)
    for path, expected_hash in native_hashes.items():
        with path.open('rb') as stream:
            require(hashlib.file_digest(stream, 'sha256').hexdigest() == expected_hash)
    require(observe(anchor) == proof and (anchor.stat().st_dev, anchor.stat().st_ino) == identity)
    result = {'schema': 'airk.private-fresh-final-retrieval-grading/1', 'families': results,
              'paired_comparisons': comparisons, 'quality_denominator': 205,
              'native_capture_sha256': {str(path): value for path, value in native_hashes.items()},
              'a1_minilm_quality_pass': all(results[r]['summary']['absolute_quality_pass'] for r in ('candidate-u8', 'candidate-s8')) and all(r['paired_floor_pass'] for r in comparisons),
              'release_or_full_A1_qualification': False, 'original205_failed_unchanged': True}
    atomic(args.output / 'grading.private.json', encoded(result))
    print(json.dumps({'complete': True, 'quality_denominator': 205,
                      'families': {k: v['summary'] for k, v in results.items()},
                      'paired_comparisons': comparisons, 'a1_minilm_quality_pass': result['a1_minilm_quality_pass'],
                      'grading_sha256': sha((args.output / 'grading.private.json').read_bytes()), 'release_or_full_A1_qualification': False}), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('prepare', 'grade'))
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--protocol-sha256', required=True)
    parser.add_argument('--execute-reviewed-opaque-operation', action='store_true', required=True)
    parser.add_argument('--candidate-freeze', type=Path)
    parser.add_argument('--candidate-freeze-sha256')
    parser.add_argument('--fixture', type=Path)
    parser.add_argument('--manifest-sha256')
    parser.add_argument('--rubric-sha256')
    parser.add_argument('--captures', type=Path, nargs='+')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    require(sys.flags.isolated)
    protocol_raw = args.protocol.read_bytes()
    require(sha(protocol_raw) == args.protocol_sha256)
    protocol = json.loads(protocol_raw)
    retained = {args.protocol: protocol_raw}
    for row in protocol['source_owners']:
        path = ROOT / row['path']
        raw = path.read_bytes()
        require(sha(raw) == row['sha256'])
        retained[path] = raw
    guard_path = ROOT / protocol['guard_source']['path']
    guard = import_retained('verified_opaque_final_guard', guard_path, retained[guard_path])
    budget_path = ROOT / protocol['budget_source']['path']
    budget = guard.import_bytes('verified_final_evaluation_budget', budget_path, retained[budget_path])
    budget.JOB_STATE = budget.enforce_budget()
    for key in ('OMP_NUM_THREADS', 'OPENBLAS_NUM_THREADS', 'MKL_NUM_THREADS', 'NUMEXPR_NUM_THREADS', 'VECLIB_MAXIMUM_THREADS'):
        os.environ[key] = '1'
    def expire():
        if not budget.done.wait(60):
            os.write(2, b'opaque_final_fixture_or_grading_deadline\n')
            os._exit(124)
    threading.Thread(target=expire, daemon=True).start()
    try:
        (prepare if args.command == 'prepare' else grade)(args, protocol, retained, guard)
    except BaseException as error:
        guard.STAGE = 'opaque_final_fixture_or_grading'
        print(json.dumps(guard.safe_failure(error)), flush=True)
        return 1
    finally:
        budget.done.set()
    return 0


if __name__ == '__main__':
    sys.exit(main())
