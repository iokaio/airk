# SPDX-License-Identifier: Apache-2.0
"""Freeze already reviewed paper/annotation IDs and algorithm before outcome access."""
import datetime
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[2]
def sha(data): return hashlib.sha256(data).hexdigest()
def load(path): return json.loads(path.read_bytes())

proposal = ROOT / 'qasper-version-set-and-private-final-proposal-r1.json'
assert sha(proposal.read_bytes()) == '9d2d79137e09b21c0b11335c830341bdae6c05f9c6087464d12d9d28fc129f84'
candidate_ids = load(proposal)['smallest_fixed_publisher_TEST_pool']['paper_ids']
inventory = ROOT / 'qasper-rights-audit-v03/paper-rights-1794623188270924300.json'
assert sha(inventory.read_bytes()) == 'f407b276e4a259b00a78396b537700e35d38747a459ec98e2b7082829a3297e0'
all_rows = {r['paper_id']: r for r in load(inventory)['papers']}
projection = ROOT / 'qasper-review-title-abstract-projection-r1.json'
assert sha(projection.read_bytes()) == 'd8ad9a32d5b14f82e01ecdc86854f6bd46f9c7a1c17976a8ca89a496c8053fdb'
projection_rows = {r['paper_id']: r for r in load(projection)['rows']}
comparison = ROOT / 'qasper-fresh-family-old65-comparison-r1.json'
assert sha(comparison.read_bytes()) == '3c01fc641c8ce00c66d2e2928a9b4fb967b7eae7f4114f90be31d9ae6b2549de'
old_ids = {r['paper_id'] for r in load(comparison)['old_family_rows']}
example_ids = {'1705.00571', '1906.08593', '1706.01875'}
assert len(candidate_ids) == len(set(candidate_ids)) == 14
assert not (set(candidate_ids) & (old_ids | example_ids))
sa_ids = {'1803.02914', '1803.08614', '1906.00424', '1908.05803', '1912.06905'}
rows = []
for identifier in candidate_ids:
    row, projection_row = all_rows[identifier], projection_rows[identifier]
    assert row['publisher_split'] == 'test'
    assert row['title'] == projection_row['title']
    rows.append({'paper_id': identifier, 'publisher_split': 'test', 'title': row['title'],
        'source_text_sha256': row['source_text_sha256'], 'question_ids': row['question_ids'],
        'human_annotation_ids': row['human_annotation_ids'], 'archive': row['archive'],
        'member': row['member'], 'member_sha256': projection_row['member_sha256'],
        'license': 'CC-BY-SA-4.0' if identifier in sa_ids else 'CC-BY-4.0',
        'exact_arxiv_version': None, 'source_version_basis': 'Reviewed complete official version SET; exact QASPER extraction edition unknown'})
assert sum(len(r['question_ids']) for r in rows) == 46
inputs = [Path(__file__), proposal, inventory, projection, comparison,
    ROOT/'qasper-per-version-rights-review-r1.json', ROOT/'qasper-version-set-rights-fact-addendum-r1.json',
    ROOT/'qasper-methodological-publisher-r1/2021.naacl-main.365.pdf',
    REPO/'tools/retrieval-train/qasper_development.py',
    ROOT/'qasper-fresh-final-materializer-r3.py', ROOT/'test-qasper-fresh-final-materializer-r3.py',
    ROOT/'observe-qasper-final-anchor-r3.ps1', ROOT/'test-qasper-final-anchor-r3.ps1',
    ROOT/'qasper-final-anchor-controls-r3.json']
for directory in ('qasper-versioned-primary-metadata-r1', 'qasper-additional-versioned-metadata-r1'):
    inputs.extend(sorted((ROOT/directory).glob('*.html')))
    inputs.append(ROOT/directory/'metadata-collection.json')
result = {'schema': 'airk.private-qasper-final-candidate-freeze/1',
    'recorded_at_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'primary_approval': 'Root explicitly approved bounded version-SET/private-FINAL contract and four representation-only maps before this candidate freeze; no real content alignment/materialization execution yet authorized.',
    'archive_sha256': '72a52a41193e2838b8074f80ac074b94f956b84886c36a61c58a7df4171bdd72',
    'archive_bytes': 3865061, 'member': 'qasper-test-v0.3.json',
    'member_sha256': '6e29ad410e6e39aa1936017fb965b30a20eb2e7751997f55b97c9d281aa884e5',
    'rows': rows, 'raw_question_count': 46, 'minimum_usable': 34,
    'supersedes_pre_execution_freeze_sha256': '23d0ba85ec970704885f874e360acdf015dcac74b4d63c92c703ac8d36a196b5',
    'population_algorithm_changed': False,
    'output_anchor': str((ROOT/'qasper-fresh-final-opaque-anchor-r3').relative_to(REPO)),
    'output_child': 'materialized',
    'protected_input_roots': ['artifacts/advanced-split/models/qasper-rights-audit-v03',
        'artifacts/advanced-split/models/native-human-fixtures-r3', 'artifacts/advanced-split/models/human-corpus-r1',
        'artifacts/advanced-split/models/qasper-versioned-primary-metadata-r1',
        'artifacts/advanced-split/models/qasper-additional-versioned-metadata-r1',
        'artifacts/advanced-split/models/qasper-methodological-publisher-r1', 'tools/retrieval-train'],
    'selection': {'domain': 'airk.qasper.fresh-final.r1:', 'rule': 'Sort ALL mechanically eligible frozen question IDs by SHA256(domain + UTF8 question_id), lexical question_id tie; take first34. If fewer34, fail without refill. Preserve every family/all46 original records in sealed payload, including refusals.',
        'usable': 'At least one ORIGINAL answerable alternative with nonempty evidence and ALL its original evidence exactly aligned to allowed original paragraphs. Never union alternatives, drop evidence or rewrite answers.',
        'refusals': ['no_human_alternatives', 'all_alternatives_unanswerable', 'no_complete_original_alternative'],
        'other_dataset_requirement': 'At least171 independently frozen fresh Hotpot questions, separate owner/protocol. No Hotpot training/calibration. This adapter does not select/adopt Hotpot.'},
    'exclusions': {'old65_paper_ids': sorted(old_ids), 'incidental_publisher_example_paper_ids': sorted(example_ids)},
    'rights': 'Private FINAL evaluation only for ALL14. No weight training, fine-tuning, quantizer fitting, calibration, thresholds, margin tuning, teacher labels or distribution. Preserve QASPER annotations and separate underlying-paper licenses, authors/version-set URLs/actual grants/hash closure, unknown edition and change records; SA private derivative notices retained.',
    'outcome_access': 'No candidate questions/answers/evidence/full_text accessed by this freezer. Original raw JSON was historically decoded for rights/schema metadata; not a pristine-unread-corpus claim.',
    'input_seals': {str(p.relative_to(REPO)): sha(p.read_bytes()) for p in inputs}}
destination = ROOT/'qasper-fresh-final-candidate-freeze-r3.json'
assert not destination.exists()
destination.write_text(json.dumps(result, ensure_ascii=False, indent=2)+'\n', encoding='utf-8')
print(json.dumps({'path': str(destination), 'sha256': sha(destination.read_bytes()), 'families': len(rows), 'raw_ids':46, 'aligned': 'NOT EXECUTED'}))
