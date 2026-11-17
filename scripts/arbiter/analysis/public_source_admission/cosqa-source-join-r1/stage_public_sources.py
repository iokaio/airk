# SPDX-License-Identifier: Apache-2.0
"""Byte-identical complete source staging and original function spans; no model calls."""
from pathlib import Path,PurePosixPath
import json,hashlib,datetime,collections
ROOT=Path(__file__).resolve().parent
OUT=ROOT/'native-visibility-preparation-r1'
def sha(data):return hashlib.sha256(data).hexdigest()
def pin(path):
 data=path.read_bytes();return {'path':str(path.resolve()),'bytes':len(data),'sha256':sha(data)}
def create(path,obj):
 with path.open('x',encoding='utf-8',newline='\n') as f:json.dump(obj,f,indent=2,ensure_ascii=False);f.write('\n')
assert not OUT.exists()
# Inputs remain unchanged. Only a fresh tree receives identical whole source bytes.
acq=json.loads((ROOT/'original-source-file-acquisition.json').read_bytes())['files']
functions=json.loads((ROOT/'source-and-rights-joined-functions.json').read_bytes())
intake=json.loads((ROOT/'native-visible-input-candidates.private.json').read_bytes())['rows']
assert len(intake)==1955 and len({x['id'] for x in intake})==1955
assert sha((ROOT/'native-visible-input-candidates.private.json').read_bytes())=='7896c7d7435e146653c6ffd93a0b180ed49b92981c346808d19573dac2d15ea7'
OUT.mkdir();tree=OUT/'source-tree';tree.mkdir();inventory=[];paths=set()
for source in acq:
 assert source['status']==200
 rel=PurePosixPath(source['repository'])/source['logical_path']
 assert not rel.is_absolute() and '..' not in rel.parts and str(rel) not in paths
 paths.add(str(rel));raw=Path(source['path']).read_bytes();assert len(raw)==source['bytes'] and sha(raw)==source['sha256']
 target=tree.joinpath(*rel.parts);target.parent.mkdir(parents=True,exist_ok=True)
 with target.open('xb') as f:f.write(raw)
 assert target.read_bytes()==raw
 inventory.append({'file':str(rel),'bytes':len(raw),'sha256':sha(raw)})
assert len(inventory)==316
by_code={x['code_idx']:x for x in intake};spans=[]
for function in functions:
 if function['status']!='exact_source_literal_confirmed':continue
 idx=function['code_idx'];row=by_code[idx];source=function['original_source'];file=function['pinned_source_file'];rel=source['repository']+'/'+source['path']
 raw=(tree/rel).read_bytes();literal=row['document'].encode();at=function['literal_byte_matches'][0]['byte_offset']
 assert raw[at:at+len(literal)]==literal and raw.count(literal)==1
 assert sha(raw)==row['source_file_sha256']==file['sha256'] and sha(literal)==row['document_sha256']
 spans.append({'code_idx':idx,'file':rel,'start':at,'end':at+len(literal),'sha256':sha(literal)})
assert len(spans)==465 and len({x['code_idx'] for x in spans})==465
create(OUT/'source-files.json',inventory);create(OUT/'function-spans.json',spans)
# Pin every retained source/license/notice body and audit source without inventing rights approval.
evidence=[]
for p in sorted(ROOT.rglob('*')):
 if not p.is_file() or OUT in p.parents:continue
 if p.suffix in ('.parquet','.log'):continue
 evidence.append(pin(p))
summary={'schema':'airk.private.cosqa-native-source-staging/1','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'input_rows':pin(ROOT/'native-visible-input-candidates.private.json'),'source_root':str(tree.resolve()),'source_files':pin(OUT/'source-files.json'),'spans':pin(OUT/'function-spans.json'),'source_file_count':len(inventory),'whole_function_count':len(spans),'human_pair_count':len(intake),'labels':dict(collections.Counter(str(x['published_human_relevance_label']) for x in intake)),'source_bytes':sum(x['bytes'] for x in inventory),'frozen_evidence':evidence,'source_files_with_review_flags_are_preserved':True,'rights_admission':False,'native_visibility_executed':False,'training_or_scoring':False,'scope':'Complete source copies preserve original repository/path; full native extraction and label applicability remain for review. No snippet documents or crop. Separate code arm only.'}
create(OUT/'preparation.json',summary)
print(json.dumps({'path':str(OUT),'sha256':pin(OUT/'preparation.json')['sha256'],'source_files':len(inventory),'source_bytes':summary['source_bytes'],'functions':len(spans),'rows':len(intake)}))
