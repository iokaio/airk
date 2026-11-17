# SPDX-License-Identifier: Apache-2.0
"""Compare cached source observations to retained original uncached A1 outputs.

This does not rerun models, change A1 grades, or manufacture new held-out data.
"""
from datetime import datetime, timezone
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import sys
import time

base=Path(__file__).resolve().parent
repo=base.parents[2]
sys.path.insert(0,str(repo/'tools/retrieval-train'))
import goal_sources
from grade_native_retrieval import evidence_recall

paths={'native':base/'native205-six-family-aggregate-r1.json',
       'uncached_result':base/'native205-human-grade-r1/grading.private.json',
       'uncached_source':base/'native205-human-grade-r1/goal_sources.original.py',
       'questions':base/'native-encoder-205-input-r1.json',
       'rubric':base/'human-goal-rubric-r4/rubric.private.json',
       'manifest':base/'native-human-fixtures-r2/manifest.private.json',
       'cached_source':Path(goal_sources.__file__),
       'unchanged_grader':repo/'tools/retrieval-train/grade_native_retrieval.py',
       'runner':Path(__file__)}
raw={name:path.read_bytes() for name,path in paths.items()}
data={name:json.loads(value) for name,value in raw.items()
      if name in ('native','uncached_result','questions','rubric','manifest')}
sha=lambda value:hashlib.sha256(value).hexdigest()
uncached=data['uncached_result']
source_seals=[value for name,value in uncached['input_seals'].items()
              if name.replace('\\','/').endswith('tools/retrieval-train/goal_sources.py')]
grader_seals=[value for name,value in uncached['input_seals'].items()
              if name.replace('\\','/').endswith('tools/retrieval-train/grade_native_retrieval.py')]
assert source_seals==[sha(raw['uncached_source'])]
assert grader_seals==[sha(raw['unchanged_grader'])]
assert sha(raw['uncached_result'])=='f6600737d6f1064347b5553ab625de0edad2eb71b2a154dd7fefb1cb8b23a3ae'
fixture=paths['manifest'].parent
questions={row['id']:row for row in data['questions']['questions']}
tasks={row['id']:row for row in data['rubric']['tasks'] if row['split']=='test'}
assert len(questions)==len(tasks)==205 and set(questions)==set(tasks)
started=time.monotonic();started_utc=datetime.now(timezone.utc).isoformat()
metadata=goal_sources.FrozenSourceMetadata(fixture)
comparisons=0;families={}
for family in data['native']['families']:
    old={row['id']:row for row in uncached['families'][family['family']]['rows']}
    assert len(old)==len(family['records'])==205 and set(old)==set(questions)
    total=Fraction(0);complete=0
    for record in family['records']:
        identifier=record['id']
        assert Path(record['source_root']).samefile(questions[identifier]['source_root'])
        ledger=goal_sources.SourceLedger(fixture,questions[identifier]['source_root'],metadata_cache=metadata)
        ledger.observe(record['primary_vector'])
        spans=list(ledger.records.values())
        assert spans==old[identifier]['source_spans'],'cached full JSON span/observations changed'
        recall,done=evidence_recall(tasks[identifier],spans)
        assert recall==Fraction(*old[identifier]['evidence_recall_fraction'])
        assert done==old[identifier]['complete_original_alternative']
        total+=recall;complete+=done;comparisons+=1
    families[family['family']]={'mean_recall_fraction':[(total/205).numerator,(total/205).denominator],
                              'complete_count':complete,'rows':205}
assert comparisons==1230
assert all(path.read_bytes()==raw[name] for name,path in paths.items())
output=base/'source-metadata-cache-1230-comparison-r1.json'
assert not output.exists()
report={'schema':'airk.private-cached-source-ledger-comparison/1',
        'started_at_utc':started_utc,'finished_at_utc':datetime.now(timezone.utc).isoformat(),
        'elapsed_seconds':time.monotonic()-started,'rows':comparisons,'families':families,
        'input_seals':{name:sha(value) for name,value in raw.items()},
        'all_full_span_json_and_exact_fractions_match':True,
        'a1_original_result_still_failed':uncached['a1_quality_pass'] is False,
        'scope':'Cached current SourceLedger compared against preserved ORIGINAL uncached full outputs/source hash. No model evaluation, altered rubric/grade algorithm, retuned cohort, authority/provenance substitution, or new A1 qualification.'}
output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
print(json.dumps({key:report[key] for key in ('elapsed_seconds','rows','all_full_span_json_and_exact_fractions_match','a1_original_result_still_failed')}))
