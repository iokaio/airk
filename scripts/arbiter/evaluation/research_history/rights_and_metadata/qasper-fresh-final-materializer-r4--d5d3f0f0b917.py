# SPDX-License-Identifier: Apache-2.0
"""Opaque private FINAL materialization; real execution needs separately reviewed freeze."""
import argparse
import base64
from collections import Counter
import hashlib
import io
import json
from pathlib import Path
import sys
import subprocess
import stat
import tarfile
import types

ROOT = Path(__file__).resolve().parents[3]
OWNER = ROOT/'tools/retrieval-train/qasper_development.py'
owner = None
ANCHOR = ROOT/'artifacts/advanced-split/models/qasper-fresh-final-opaque-anchor-r4'
ANCHOR_OBSERVER = Path(__file__).with_name('observe-qasper-final-anchor-r3.ps1')
PHASE = 'metadata_preflight'

def sha(raw): return hashlib.sha256(raw).hexdigest()
def packed(value): return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':')).encode()
def refusal(code): raise ValueError(code)

def load_owner(exact_verified_bytes):
    global owner
    owner = types.ModuleType('qasper_owner_verified_bytes')
    owner.__file__ = str(OWNER)
    exec(compile(exact_verified_bytes, str(OWNER), 'exec'), owner.__dict__)

def align_paper(paper, row):
    """Whole original alternatives remain separate; no evidence/answer synthesis."""
    if owner is None: refusal('verified_owner_not_loaded')
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

def observe_anchor(anchor, observer_bytes):
    # Execute retained verified script bytes, never reopen a verified script path.
    script = '& {\n'+observer_bytes.decode('utf-8')+'\n} -Anchor \''+str(anchor).replace("'", "''")+"'"
    result = subprocess.run(['pwsh','-NoProfile','-NonInteractive','-EncodedCommand',
        base64.b64encode(script.encode('utf-16-le')).decode('ascii')], capture_output=True, text=True, timeout=15)
    if result.returncode != 0: refusal('private_anchor_owner_proof_failed')
    observation = json.loads(result.stdout)
    if observation.get('schema') != 'airk.private-qasper-final-anchor-observation/1' or observation.get('current_owner_only') is not True or observation.get('no_reparse') is not True:
        refusal('private_anchor_owner_proof_invalid')
    if not Path(observation['canonical_path']).samefile(anchor): refusal('anchor_proof_foreign_path')
    return observation

def output_guard(output, protected, anchor, ownership_observer):
    output = output.resolve()
    for path in (anchor, *anchor.parents):
        if path.is_relative_to(ROOT) and path.exists() and (path.is_symlink() or getattr(path.lstat(), 'st_file_attributes',0) & getattr(stat,'FILE_ATTRIBUTE_REPARSE_POINT',1024)):
            refusal('private_anchor_reparse_path')
    anchor = anchor.resolve(strict=True)
    if output.exists() or output.name != 'materialized' or not output.parent.exists() or not output.parent.samefile(anchor):
        refusal('fresh_private_output_required')
    for source in protected:
        source = Path(source).resolve(strict=True)
        if output.is_relative_to(source): refusal('output_inside_input_root')
        for ancestor in output.parents:
            if ancestor.exists() and ancestor.samefile(source): refusal('output_aliases_input_root')
    before = anchor.stat()
    observation = ownership_observer(anchor)
    after = anchor.stat()
    if (before.st_dev,before.st_ino) != (after.st_dev,after.st_ino): refusal('anchor_identity_changed')
    return output, observation, (after.st_dev,after.st_ino)

def execute(args):
    global PHASE
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
    observer_bytes = retained[ANCHOR_OBSERVER.resolve(strict=True)]
    if freeze.get('output_anchor') != str(ANCHOR.relative_to(ROOT)) or freeze.get('output_child') != 'materialized':
        refusal('frozen_output_anchor_changed')
    protected = [ROOT/path for path in freeze['protected_input_roots']]
    if args.raw_root.resolve(strict=True) != (ROOT/'artifacts/advanced-split/models/qasper-rights-audit-v03').resolve(strict=True):
        refusal('raw_root_changed')
    output, anchor_proof, anchor_identity = output_guard(args.output, protected, ANCHOR,
        lambda anchor: observe_anchor(anchor, observer_bytes))
    # Only execute the exact retained helper after its frozen hash was checked.
    load_owner(retained[OWNER.resolve(strict=True)])
    PHASE = 'content_alignment'
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
    PHASE = 'output_preflight'
    for path, raw in retained.items():
        if path.read_bytes() != raw: refusal('input_drift_during_materialization')
    # No human text/IDs/per-question outcomes leave this sealed payload.
    report = {'schema': 'airk.private-qasper-fresh-final-opaque-counts/1', 'complete': True,
        'freeze_sha256': sha(freeze_raw), 'families': 14, 'raw_questions': 46, 'eligible': counts.get('eligible',0),
        'refusal_counts': {k:v for k,v in counts.items() if k!='eligible'}, 'selected': len(selected),
        'minimum_met': len(selected)==34, 'sealed_payload_sha256': sha(payload_raw),
        'input_seals': {str(path):sha(raw) for path,raw in retained.items()},
        'scope': 'Counts/rights alignment only; no model scores, human-goal or native qualification.'}
    report['anchor_owner_proof'] = anchor_proof
    # Fresh ownership proof and same directory identity immediately before any mkdir.
    current_anchor = ANCHOR.stat()
    if (current_anchor.st_dev,current_anchor.st_ino) != anchor_identity: refusal('anchor_identity_changed')
    final_proof = observe_anchor(ANCHOR, observer_bytes)
    if final_proof != anchor_proof: refusal('anchor_acl_changed')
    output.mkdir()
    PHASE = 'sealed_write'
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
    except Exception as error:
        # Never print parser/exception objects which could contain heldout human text.
        print(json.dumps({'complete':False,'stage':PHASE,'infrastructure_refusal':
            'required_path_missing' if isinstance(error,FileNotFoundError) else 'materialization_failed_no_text_disclosed'}))
        return 1
    return 0

if __name__=='__main__': sys.exit(main())
