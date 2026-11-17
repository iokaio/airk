# SPDX-License-Identifier: Apache-2.0
"""Read only original DEV DOC_IDS as blind split metadata; never decode other DEV values."""
import collections,hashlib,importlib.util,io,json,pathlib,tarfile,time
BASE=pathlib.Path(__file__).resolve().parent
START=time.monotonic();CUTOFF=START+300
spec=importlib.util.spec_from_file_location('retained_forward_only_reader',BASE/'audit_original_archive_r3.py');helper=importlib.util.module_from_spec(spec);spec.loader.exec_module(helper)
def clock():
 if time.monotonic()>=CUTOFF:raise TimeoutError('original local DEV metadata audit cutoff')
helper.DEADLINE=CUTOFF

def skip_value(chars,first,capture):
 if first=='"':
  escaped=False
  while True:
   c=chars.char()
   if escaped:escaped=False
   elif c=='\\':escaped=True
   elif c=='"':return None
 if first in '[{':
  depth=1;string=False;escaped=False;raw=[first]if capture else None
  while depth:
   c=chars.char()
   if capture:
    raw.append(c)
    if len(raw)>100000:raise ValueError('DOC_IDS metadata exceeds fixed bound')
   if string:
    if escaped:escaped=False
    elif c=='\\':escaped=True
    elif c=='"':string=False
   elif c=='"':string=True
   elif c in '[{':depth+=1
   elif c in ']}':depth-=1
  return json.loads(''.join(raw))if capture else None
 raise ValueError('unexpected original DEV value framing; no semantic fallback')

def parse_metadata(binary):
 chars=helper.Characters(io.TextIOWrapper(io.BufferedReader(helper.ForwardOnly(binary)),encoding='utf-8'));assert chars.nonspace()=='[';first=chars.nonspace();result=[]
 while first!=']':
  assert first=='{';key_first=chars.nonspace();docs=None
  while key_first!='}':
   field=chars.key(key_first);assert chars.nonspace()==':';capture=field=='DOC_IDS';value=skip_value(chars,chars.nonspace(),capture)
   if capture:
    assert docs is None and isinstance(value,list)and all(type(x)is str for x in value);docs=value
   separator=chars.nonspace()
   if separator=='}':break
   assert separator==',';key_first=chars.nonspace()
  assert docs is not None;result.append(docs);separator=chars.nonspace()
  if separator==']':return result
  assert separator==',';first=chars.nonspace()
 return result

def controls():
 raw=b'[{"QUESTION_TEXT":"DO NOT MATERIALIZE","ANSWER":"NO","DOC_IDS":["doc-A","doc-B"]},{"DOC_IDS":["doc-C"],"QUESTION_TITLE":"UNSEEN"}]'
 observed=parse_metadata(io.BytesIO(raw));assert observed==[['doc-A','doc-B'],['doc-C']]
 print('Blind DOC_IDS-only lexical filter control PASS',flush=True)
def main():
 controls();archive=BASE/'TechQA.publisher-original.tar.gz';assert archive.stat().st_size==2959973525
 with archive.open('rb')as source:digest=hashlib.file_digest(source,'sha256').hexdigest()
 assert digest=='6b094ef9a69718f727ce8d7e15c4d961e51032cefaa952e0d6af9d176d7ba118'
 observed=None
 with archive.open('rb')as source,tarfile.open(fileobj=helper.TimedFile(source),mode='r|gz')as tar:
  for member in tar:
   clock()
   if member.name=='TechQA/training_and_dev/dev_Q_A.json':
    assert member.isfile()and member.size<16000000;observed=parse_metadata(tar.extractfile(member));break
 assert observed is not None and len(observed)==310;dev={value for row in observed for value in row}
 original=json.loads((BASE/'local-parse-r3/original-training_Q_A.json').read_bytes());prior=json.loads((BASE.parent/'techqa-original-train-audit-r1/source-and-split-audit.json').read_bytes());old_excluded={name.removesuffix('.txt')for name in prior['shared_filenames_excluded']};known_dev_count=prior['DEV_filename_only_metadata_count'];assert old_excluded<=dev
 records=[];count=collections.Counter();source_ids=set()
 for row in original:
  positive=row['ANSWERABLE']=='Y';gold_overlap=positive and row['DOCUMENT']in dev;pool_overlap=sorted(set(row['DOC_IDS'])&dev);count['original_TRAIN_rows']+=1;count['positive_TRAIN_rows']+=positive;count['positive_gold_source_overlap']+=gold_overlap;count['positive_gold_source_disjoint']+=positive and not gold_overlap;count['whole_candidate_pool_disjoint_positive']+=positive and not pool_overlap;count['whole_candidate_pool_disjoint_all_rows']+=not pool_overlap
  if positive:source_ids.add(row['DOCUMENT'])
  records.append({'original_TRAIN_id':row['QUESTION_ID'],'answerable':positive,'original_gold_document_id':row['DOCUMENT'],'gold_source_overlaps_heldout':gold_overlap,'heldout_ids_in_original_candidate_pool':pool_overlap,'source_pair_candidate_only':positive and not gold_overlap,'whole_original_context_closure_disjoint':not pool_overlap})
 overlap=source_ids&dev
 receipt={'schema':'airk.private.techqa-original-DEV-source-closure-fence/1','original_archive_sha256':digest,'original_DEV_rows_DOC_IDS_only':len(observed),'original_DEV_source_ids_count':len(dev),'mirrored_DEV_relevant_source_ids_count':known_dev_count,'all_mirror_DEV_refs_revalidated_this_run':False,'prior_gold_only_shared_ids':len(old_excluded),'original_TRAIN_gold_source_ids':len(source_ids),'full_heldout_overlap_ids_count':len(overlap),'additional_overlaps_beyond_prior_fence':len(overlap-old_excluded),'quarantined_source_ids':sorted(overlap),'entire_DEV_source_id_closure':sorted(dev),'counts':dict(count),'original_DEV_non_DOC_IDS_values_not_JSON_materialized_this_run':True,'no_mixed_mirror_redecode':True,'no_DEV_question_output_or_model_use':True,'prior_r4_scope_caveat_retained':True,'no_additional_download':True,'no_model_scoring_training_Cargo':True,'native_admitted':0,'source_pair_scope_not_equivalent_to_original_whole_menu':True,'elapsed_seconds':time.monotonic()-START,'records':records}
 path=BASE/'full-original-DEV-source-fence-r5.json'
 with path.open('x',encoding='utf-8',newline='\n')as output:json.dump(receipt,output,indent=2);output.write('\n')
 print('FINAL',json.dumps({k:v for k,v in receipt.items()if k not in['records','quarantined_source_ids','entire_DEV_source_id_closure']}),flush=True);print('FENCESHA',hashlib.sha256(path.read_bytes()).hexdigest(),flush=True)
if __name__=='__main__':main()
