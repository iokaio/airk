# SPDX-License-Identifier: Apache-2.0
"""Bounded original TechQA publisher provenance job. Never executes models or emits question text."""
import collections,hashlib,io,json,pathlib,re,shutil,tarfile,time,urllib.request
ROOT=pathlib.Path(__file__).resolve().parent
PARENT=ROOT.parent
PLAN=json.loads((ROOT/'plan.json').read_bytes())
START=time.monotonic();DEADLINE=START+PLAN['whole_original_job_seconds']
def clock():
 if time.monotonic()>=DEADLINE:raise TimeoutError('original source-job deadline expired')
def sha(raw):return hashlib.sha256(raw).hexdigest()
def write(path,value):
 with path.open('x',encoding='utf-8',newline='\n')as f:json.dump(value,f,indent=2,ensure_ascii=False);f.write('\n')
class TimedFile:
 def __init__(self,stream):self.stream=stream
 def read(self,n=-1):clock();return self.stream.read(n)
class Characters:
 def __init__(self,source):self.source=source;self.buffer='';self.index=0
 def char(self):
  if self.index==len(self.buffer):
   clock();self.buffer=self.source.read(65536);self.index=0
   if not self.buffer:raise EOFError('unexpected end of original source JSON')
  c=self.buffer[self.index];self.index+=1;return c
 def nonspace(self):
  c=self.char()
  while c.isspace():c=self.char()
  return c
 def key(self,first):
  assert first=='"';parts=['"'];escaped=False
  while True:
   c=self.char();parts.append(c)
   if c=='"'and not escaped:return json.loads(''.join(parts))
   if escaped:escaped=False
   elif c=='\\':escaped=True
 def object(self,first,capture):
  assert first=='{';depth=1;string=False;escaped=False;parts=['{']if capture else None
  while depth:
   c=self.char()
   if capture:
    parts.append(c)
    if len(parts)>2500000:raise ValueError('selected original document exceeds cap')
   if string:
    if escaped:escaped=False
    elif c=='\\':escaped=True
    elif c=='"':string=False
   elif c=='"':string=True
   elif c in '{[':depth+=1
   elif c in '}]':depth-=1
  return json.loads(''.join(parts))if capture else None

def documents(stream,allowed):
 chars=Characters(io.TextIOWrapper(stream,encoding='utf-8'));assert chars.nonspace()=='{';selected={};total=0;first=chars.nonspace()
 if first=='}':return selected,total
 while True:
  key=chars.key(first);assert chars.nonspace()==':'
  capture=key in allowed;value=chars.object(chars.nonspace(),capture);total+=1
  if capture:
   assert key not in selected;selected[key]=value
  separator=chars.nonspace()
  if separator=='}':return selected,total
  assert separator==',';first=chars.nonspace()

def main():
 report={'schema':'airk.private.techqa-original-publisher-TRAIN-provenance/1','complete':False,'failure':None,'plan_sha256':sha((ROOT/'plan.json').read_bytes()),'no_DEV_FINAL_questions_or_nonTRAIN_source_semantic_use':True,'no_model_scoring_training_Cargo':True}
 try:
  assert shutil.disk_usage(ROOT).free>PLAN['archive_bytes']+1024**3
  url='https://huggingface.co/datasets/PrimeQA/TechQA/resolve/'+PLAN['publisher_revision']+'/TechQA.tar.gz';temporary=ROOT/'TechQA.tar.gz.partial';archive=ROOT/'TechQA.publisher-original.tar.gz';hasher=hashlib.sha256();received=0;milestone=0
  with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'AIRK-authorized-original-TRAIN-provenance'}),timeout=35)as response,temporary.open('xb')as output:
   while True:
    clock();chunk=response.read(4*1024*1024)
    if not chunk:break
    received+=len(chunk)
    if received>PLAN['physical_download_cap_bytes']or received>PLAN['archive_bytes']:raise ValueError('original physical download cap exceeded')
    output.write(chunk);hasher.update(chunk)
    if received//(256*1024*1024)>milestone:milestone=received//(256*1024*1024);print('DOWNLOAD',received,'of',PLAN['archive_bytes'],'elapsed',round(time.monotonic()-START,3),flush=True)
  assert received==PLAN['archive_bytes']and hasher.hexdigest()==PLAN['archive_lfs_sha256']
  assert temporary.resolve().parent==ROOT.resolve()and archive.resolve().parent==ROOT.resolve()and not archive.exists();temporary.rename(archive)
  with archive.open('rb')as source:reopened=hashlib.file_digest(source,'sha256').hexdigest()
  assert reopened==PLAN['archive_lfs_sha256'];report['archive']={'path':str(archive),'bytes':received,'sha256':reopened,'publisher_revision':PLAN['publisher_revision'],'reopened_hash':True};print('ARCHIVE AUTHENTICATED',received,'elapsed',round(time.monotonic()-START,3),flush=True)
  mirrored=json.loads((PARENT/'techqa-original-train-audit-r1/original-TRAIN-only.json').read_bytes());source_audit=json.loads((PARENT/'techqa-original-train-audit-r1/source-and-split-audit.json').read_bytes());excluded=set(source_audit['shared_filenames_excluded']);allowed={c['filename'].removesuffix('.txt')for row in mirrored for c in row['contexts']if c['filename']not in excluded};assert len(allowed)==348
  (ROOT/'TRAIN-original-documents').mkdir();(ROOT/'notices').mkdir();original=None;selected_documents={};members=[];notices=[];document_total=0
  with archive.open('rb')as raw_archive,tarfile.open(fileobj=TimedFile(raw_archive),mode='r|gz')as tar:
   for member in tar:
    clock();name=pathlib.PurePosixPath(member.name);assert not name.is_absolute()and '..'not in name.parts
    members.append({'name':member.name,'bytes':member.size,'regular':member.isfile()})
    if not member.isfile():continue
    if member.name=='TechQA/training_and_dev/training_Q_A.json':
     assert member.size<16000000;raw=tar.extractfile(member).read();original=json.loads(raw);assert isinstance(original,list)and len(original)==600
     with(ROOT/'original-training_Q_A.json').open('xb')as output:output.write(raw)
     report['original_TRAIN_QA_pin']={'path':str(ROOT/'original-training_Q_A.json'),'sha256':sha(raw),'bytes':len(raw)};print('ORIGINAL TRAIN QA RETAINED',len(original),flush=True)
    elif member.name=='TechQA/training_and_dev/training_dev_technotes.json':
     selected_documents,document_total=documents(tar.extractfile(member),allowed)
     for key,value in selected_documents.items():
      assert re.fullmatch(r'[A-Za-z0-9_-]+',key);write(ROOT/'TRAIN-original-documents'/(key+'.json'),value)
     print('ORIGINAL DOCUMENT MAP BLIND FILTER',len(selected_documents),'of',document_total,'entries',flush=True)
    elif name.name.lower().startswith(('readme','license','cdla','notice'))and member.size<2000000:
     raw=tar.extractfile(member).read();path=ROOT/'notices'/(sha(member.name.encode())+'.bytes')
     with path.open('xb')as output:output.write(raw)
     notices.append({'archive_member':member.name,'path':str(path),'sha256':sha(raw),'bytes':len(raw)})
    # All other members, including DEV/validation questions and full corpus,
    # are skipped as opaque bytes by tar; they are never JSON-decoded or extracted.
  assert original is not None;byid={r['QUESTION_ID']:r for r in original};assert len(byid)==600 and set(byid)=={r['id']for r in mirrored}
  counts=collections.Counter();comparisons=[]
  for mirror in mirrored:
   row=byid[mirror['id']];title,body=row['QUESTION_TITLE'],row['QUESTION_BODY'];framings={'title_space_body':title+' '+body,'title_newline_body':title+'\n'+body,'body':body,'title':title,'title_double_newline_body':title+'\n\n'+body};matches=[k for k,v in framings.items()if v==mirror['question']];answerable=row['ANSWERABLE']=='Y';answer_same=row['ANSWER']==mirror['answer'];answerability_same=answerable is(not mirror['is_impossible']);document_ids={c['filename'].removesuffix('.txt')for c in mirror['contexts']};original_document=row['DOCUMENT'];relevant_same=document_ids=={original_document}if answerable else not document_ids
   record={'original_TRAIN_id':row['QUESTION_ID'],'original_title_sha256':sha(title.encode()),'original_body_sha256':sha(body.encode()),'mirror_question_sha256':sha(mirror['question'].encode()),'exact_question_framings':matches,'answer_exact':answer_same,'answerability_exact':answerability_same,'relevant_context_reference_exact':relevant_same,'original_candidate_document_ids':row['DOC_IDS'],'mirror_keeps_original_candidate_pool':set(row['DOC_IDS'])==document_ids,'original_human_answer_sha256':sha(str(row['ANSWER']).encode())}
   counts['TRAIN_rows']+=1;counts['exact_question_framing']+=bool(matches);counts['answer_exact']+=answer_same;counts['answerability_exact']+=answerability_same;counts['relevant_context_reference_exact']+=relevant_same;counts['original_candidate_pool_preserved']+=record['mirror_keeps_original_candidate_pool']
   if answerable and original_document in selected_documents:
    document=selected_documents[original_document];start,end=int(row['START_OFFSET']),int(row['END_OFFSET']);valid=document['text'][start:end]==row['ANSWER'];record['original_human_offset_valid']=valid;record['original_start_char']=start;record['original_end_char']=end;record['original_document_pin']={'path':str(ROOT/'TRAIN-original-documents'/(original_document+'.json')),'sha256':sha((ROOT/'TRAIN-original-documents'/(original_document+'.json')).read_bytes())};counts['original_answer_offsets_checked']+=1;counts['original_answer_offsets_valid']+=valid
   comparisons.append(record)
  report.update({'complete':True,'counts':dict(counts),'original_document_map_entries_metadata_only':document_total,'TRAIN_only_original_documents_retained':len(selected_documents),'predeclared_TRAIN_document_ids':len(allowed),'missing_predeclared_TRAIN_documents':sorted(allowed-set(selected_documents)),'notice_pins':notices,'archive_member_census':members,'comparisons':comparisons,'candidate_pool_warning':'mirror contexts preserve relevant reference only, not original DOC_IDS menu; never infer NoJustifiedRead or negative relevance for omitted documents','license':'actual original data and corpus CDLA-Permissive-1.0, wrapper declarations separate'})
 except BaseException as error:report['failure']={'type':type(error).__name__,'message':str(error)}
 finally:
  report['elapsed_seconds']=time.monotonic()-START;report['original_job_expired']=time.monotonic()>=DEADLINE
  if report['original_job_expired']:report['complete']=False
  write(ROOT/'receipt.json',report);print('FINAL',json.dumps({k:v for k,v in report.items()if k not in['comparisons','notice_pins','archive_member_census']}),flush=True)
 return 0 if report['complete']else 1
if __name__=='__main__':raise SystemExit(main())
