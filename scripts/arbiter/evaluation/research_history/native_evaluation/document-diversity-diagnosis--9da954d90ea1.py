# SPDX-License-Identifier: Apache-2.0
"""Read-only retained DEV20 top10 source concentration; no NN/regrade/final access."""
from collections import Counter
from pathlib import Path
import hashlib
import json
import sqlite3
import time
ROOT=Path(__file__).resolve().parents[4];BASE=ROOT/'artifacts/advanced-split/models'
started=time.monotonic();sha=lambda b:hashlib.sha256(b).hexdigest()
grade=BASE/'candidate-native-r20-r1/grading/grading.private.json';rubric=BASE/'a1-component-dev-native-fixture-r1/rubric.private.json'
raw={grade:grade.read_bytes(),rubric:rubric.read_bytes(),Path(__file__):Path(__file__).read_bytes()}
require=lambda value:None if value else (_ for _ in ()).throw(ValueError('retained DEV identity changed'))
require(sha(raw[grade])=='15a24387769dd1a8b62efe941f764c45fc47f9f6e61437e83671a9fbf0d68407');require(sha(raw[rubric])=='587bb503da85119b9f0f7fbb544a0216d655ac07691a3c6319a2a9b9fc1e3b2a')
tasks={t['id']:t for t in json.loads(raw[rubric])['tasks']};families=[]
for family in json.loads(raw[grade])['families']:
    rows=[]
    for row in family['rows']:
        if row['quality_status']!='ALIGNED':continue
        mapping={s['native_path']:s['file'] for s in row['source_spans']};hits=row['native_record']['primary_vector']['hits'];counts=Counter(mapping[h['citation']['path']] for h in hits)
        support={loc['file'] for alternative in tasks[row['id']]['alternatives'] for fact in alternative['facts'] for loc in fact['locations']}
        text_counts=Counter(sha(h['text'].encode()) for h in hits)
        rows.append({'id':row['id'],'hits':len(hits),'offered_file_counts':dict(sorted(counts.items())),'unique_offered_files':len(counts),'largest_same_file_concentration':max(counts.values(),default=0),'duplicate_file_slots':len(hits)-len(counts),'unique_shown_body_sha256':len(text_counts),'supporting_file_count':len(support),'supporting_files_offered':len(support&counts.keys()),'supporting_files_missing':len(support-counts.keys())})
    database=next((BASE/'candidate-native-r20-r1'/family['family']/'data').rglob('current.sqlite3'));before=sha(database.read_bytes())
    with sqlite3.connect(database.resolve().as_uri()+'?mode=ro&immutable=1',uri=True) as db:
        graph=dict(db.execute('select live,count(*) from graph_records group by live').fetchall());lexical=db.execute('select count(*) from current_lexical_docs').fetchone()[0]
    require(sha(database.read_bytes())==before)
    families.append({'family':family['family'],'quality_queries':len(rows),'unique_file_count_distribution':dict(Counter(r['unique_offered_files'] for r in rows)),'same_file_max_concentration_distribution':dict(Counter(r['largest_same_file_concentration'] for r in rows)),'queries_with10unique_files':sum(r['unique_offered_files']==10 for r in rows),'total_duplicate_file_slots':sum(r['duplicate_file_slots'] for r in rows),'raw_persisted_graph_record_counts_by_live':graph,'raw_lexical_document_rows':lexical,'database_sha256':before,'rows':rows})
for path,blob in raw.items():require(path.read_bytes()==blob)
result={'schema':'airk.private-DEV-document-diversity-diagnosis/1','source_inputs':{str(p.relative_to(ROOT)):sha(b) for p,b in raw.items()},'families':families,'no_NN_or_new_grade':True,'scope':'Offered top10 file concentration only. Raw SQLite graph rows are not an independently authenticated logical-unit census. No query vectors/exhaustive cosine ranks observed; cannot claim exact ANN recall or encoder-only cause. Original quality denominator/labels/outcomes unchanged.','elapsed_seconds':time.monotonic()-started}
output=Path(__file__).with_suffix('.json');require(not output.exists());output.write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps([{k:v for k,v in f.items() if k!='rows'} for f in families]),flush=True)
