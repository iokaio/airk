# SPDX-License-Identifier: Apache-2.0
"""Opaque private FINAL materialization; real execution needs separately reviewed freeze."""
import argparse
from collections import Counter
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tarfile

ROOT = Path(__file__).resolve().parents[3]
OWNER = ROOT/'tools/retrieval-train/qasper_development.py'
spec = importlib.util.spec_from_file_location('qasper_owner', OWNER)
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)

def sha(raw): return hashlib.sha256(raw).hexdigest()
def packed(value): return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':')).encode()
def refusal(code): raise ValueError(code)

def align_paper(paper, row):
    """Whole original alternatives remain separate; no evidence/answer synthesis."""
    if row['publisher_split'] != 'test': refusal('publisher_split_changed')
    observed_questions = [q['question_id'] for q in paper['qas']]
    observed_annotations = [a['annotation_id'] for q in paper['qas'] for a in q['answers']]
    if observed_questions != row['question_ids'] or observed_annotations != row['human_annotation_ids']:
        refusal('frozen_human_id_roster_changed')
    if len(observed_questions) != len(set(observed_questions)) or len(observed_annotations) != len(set(observed_annotations)):
        refusal('duplicate_original_ids')
    decision = {**row, 'expected_human_alternatives': len(observed_annotations)}
    view = owner.prepare(paper, decision)
    original_by_id = {q['question_id']: q for q in paper['qas']}
    for question in view['questions']:
        original = original_by_id[question['question_id']]
        for alternative, annotation in zip(question['human_alternatives'], original['answers'], strict=True):
            # Retain the actual complete publisher record, never a generated answer.
            alternative['original_human_record'] = annotation
        alternatives = question['human_alternatives']
        usable = any(not a['unanswerable'] and a['alignment'] == 'all_original_evidence_maps_to_allowed_text'
                     for a in alternatives)
        question['eligibility'] = 'eligible' if usable else ('no_human_alternatives' if not alternatives
            else 'all_alternatives_unanswerable' if all(a['unanswerable'] for a in alternatives)
            else 'no_complete_original_alternative')
    # Avoid inheriting the helper's historical development-only authority description.
    return {'paper_id': row['paper_id'], 'publisher_split': 'test', 'split': 'final',
        'decision': decision, 'passages': view['passages'], 'excluded_passages': view['excluded_passages'],
        'questions': view['questions'], 'disclosure': 'Mechanical original-paragraph alignment only. No new human adjudication, sufficiency or gap labels.'}

def choose(views, minimum, domain):
    questions = [q for v in views for q in v['questions']]
    ids = [q['question_id'] for q in questions]
    if len(ids) != len(set(ids)): refusal('cross_paper_duplicate_question_id')
    eligible = [q['question_id'] for q in questions if q['eligibility'] == 'eligible']
    eligible.sort(key=lambda identifier: (sha((domain+identifier).encode()), identifier))
    return eligible[:minimum] if len(eligible) >= minimum else [], Counter(q['eligibility'] for q in questions)

def output_guard(output, protected):
    output = output.resolve()
    if output.exists() or not output.is_relative_to((ROOT/'artifacts/advanced-split/models').resolve(strict=True)):
        refusal('fresh_private_output_required')
    for source in protected:
        source = Path(source).resolve(strict=True)
        if output.is_relative_to(source): refusal('output_inside_input_root')
        for ancestor in output.parents:
            if ancestor.exists() and ancestor.samefile(source): refusal('output_aliases_input_root')
    return output

def execute(args):
    retained = {}
    def sealed(path, expected=None):
        path = Path(path).resolve(strict=True)
        raw = path.read_bytes()
        if expected is not None and sha(raw) != expected: refusal('input_hash_changed')
        retained[path] = raw
        return raw
    freeze_raw = sealed(args.freeze, args.freeze_sha256)
    freeze = json.loads(freeze_raw)
    if freeze.get('schema') != 'airk.private-qasper-final-candidate-freeze/1': refusal('unknown_freeze')
    if len(freeze['rows']) != 14 or freeze['raw_question_count'] != 46 or freeze['minimum_usable'] != 34:
        refusal('frozen_population_changed')
    for path, expected in freeze['input_seals'].items(): sealed(ROOT/path, expected)
    # Protect all raw/old corpus inputs from accidental metadata creation, before mkdir.
    output = output_guard(args.output, [args.raw_root, ROOT/'artifacts/advanced-split/models/native-human-fixtures-r2',
        ROOT/'artifacts/advanced-split/models/human-corpus-r1'])
    archive_raw = sealed(args.raw_root/'qasper-test-and-evaluator-v0.3.tgz', freeze['archive_sha256'])
    if len(archive_raw) != freeze['archive_bytes']: refusal('archive_size_changed')
    with tarfile.open(fileobj=io.BytesIO(archive_raw), mode='r:gz') as archive:
        member = archive.getmember(freeze['member'])
        if not member.isfile() or member.size > 33554432: refusal('invalid_archive_member')
        member_raw = archive.extractfile(member).read(member.size+1)
        if len(member_raw) != member.size or sha(member_raw) != freeze['member_sha256']: refusal('member_hash_changed')
    corpus = json.loads(member_raw)
    views = [align_paper(corpus[row['paper_id']], row) for row in freeze['rows']]
    selected, counts = choose(views, freeze['minimum_usable'], freeze['selection']['domain'])
    if sum(counts.values()) != 46: refusal('raw_count_changed')
    rights_evidence = {str(path.relative_to(ROOT)): raw.decode('utf-8') for path, raw in retained.items()
        if path.suffix in ('.json', '.html') and path != args.freeze.resolve()}
    payload = {'schema': 'airk.private-qasper-fresh-final-sealed/1', 'freeze_sha256': sha(freeze_raw),
        'papers': views, 'selected_question_ids': selected, 'minimum_met': len(selected)==34,
        'rights_evidence': rights_evidence, 'publisher_split': 'test', 'usage': 'PRIVATE FINAL ONLY; no training/calibration/threshold/quantizer/weights/distribution',
        'attribution': 'Dasigi, Lo, Beltagy, Cohan, Smith and Gardner (2021), QASPER annotations CC BY4; underlying authors and complete official version-set grants retained separately.',
        'changes': ['QASPER/S2ORC normalized extraction; exact arXiv edition UNKNOWN',
            'Original paragraph bytes preserved; quote-bearing paragraphs/figures/tables excluded from consumable source view',
            'Original questions, all original alternatives and evidence retained; paragraph IDs/SHA and mechanical alignment added',
            'Private derived BY-SA paper views retain BY-SA4 attribution/sharealike; no corpus publication']}
    payload_raw = packed(payload)
    for path, raw in retained.items():
        if path.read_bytes() != raw: refusal('input_drift_during_materialization')
    # No human text/IDs/per-question outcomes leave this sealed payload.
    report = {'schema': 'airk.private-qasper-fresh-final-opaque-counts/1', 'complete': True,
        'freeze_sha256': sha(freeze_raw), 'families': 14, 'raw_questions': 46, 'eligible': counts.get('eligible',0),
        'refusal_counts': {k:v for k,v in counts.items() if k!='eligible'}, 'selected': len(selected),
        'minimum_met': len(selected)==34, 'sealed_payload_sha256': sha(payload_raw),
        'input_seals': {str(path):sha(raw) for path,raw in retained.items()},
        'scope': 'Counts/rights alignment only; no model scores, human-goal or native qualification.'}
    output.mkdir()
    (output/'sealed-final.private.json').write_bytes(payload_raw)
    (output/'opaque-counts.json').write_bytes(packed(report))
    print(json.dumps({'complete':True, 'minimum_met':report['minimum_met'], 'families':14,
        'raw_questions':46, 'eligible':report['eligible'], 'selected':len(selected),
        'refusal_counts':report['refusal_counts'], 'sealed_payload_sha256':sha(payload_raw),
        'opaque_report_sha256':sha(packed(report))}))

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--freeze',type=Path,required=True)
    parser.add_argument('--freeze-sha256',required=True)
    parser.add_argument('--raw-root',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--execute-reviewed-content-alignment',action='store_true',required=True)
    args=parser.parse_args()
    try: execute(args)
    except Exception:
        # Never print parser/exception objects which could contain heldout human text.
        print(json.dumps({'complete':False,'infrastructure_refusal':'materialization_failed_no_text_disclosed'}))
        return 1
    return 0

if __name__=='__main__': sys.exit(main())
