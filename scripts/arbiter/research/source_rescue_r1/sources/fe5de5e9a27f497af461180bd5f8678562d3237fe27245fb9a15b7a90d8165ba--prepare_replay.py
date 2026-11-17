# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import collections,datetime,hashlib,importlib.util,io,json,sys,tarfile,time,unicodedata
import replay_logic as logic
ROOT=Path(__file__).resolve().parents[5];E=ROOT/'artifacts/v190/final-plan/exec-20261116T052942Z';OUT=Path(__file__).resolve().parent;M=ROOT/'artifacts/advanced-split/models';B=M/'actual-composed-service-r2/public-human-label-admission-r1'
spec=importlib.util.spec_from_file_location('custody',E/'candidate-overlay-preparation-r1/candidate_overlay.py');h=importlib.util.module_from_spec(spec);sys.modules[spec.name]=h;spec.loader.exec_module(h);identity=h.identity();assert h.security(OUT)['owner_sid']==h.HUMAN
sys.path.insert(0,str(ROOT/'tools/retrieval-train'));import training_recipe as recipe;import training_recipe_audit as source
started=time.perf_counter();pins={};checks=[]
def check(name,condition):checks.append({'name':name,'passed':bool(condition)});assert condition,name
def raw(p):
 p=Path(p);b=p.read_bytes();pins[str(p.resolve())]={'path':str(p.resolve()),'bytes':len(b),'sha256':logic.sha(b),'security':h.security(p)};return b
def load(p):return json.loads(raw(p))
def put(name,value):
 p=OUT/name;assert not p.exists();p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(value if isinstance(value,bytes)else(json.dumps(value,ensure_ascii=False,indent=2)+'\n').encode());assert h.security(p)['owner_sid']==h.HUMAN;return {'path':str(p),'bytes':p.stat().st_size,'sha256':logic.sha(p.read_bytes())}
for p in [ROOT/'tools/retrieval-train/training_recipe.py',ROOT/'tools/retrieval-train/training_recipe_audit.py',ROOT/'tools/retrieval-train/qasper_development.py',ROOT/'tools/retrieval-train/test_training_recipe.py',ROOT/'docs/arbiter/final-plan.md',E/'f2-existing-pairs-closure-r1/candidate-card-r2.json']:raw(p)
corpus_path=M/'human-corpus-r1/corpus.private.json';corpus_raw=raw(corpus_path);check('original adopted corpus remains exact',logic.sha(corpus_raw)=='6e9d8ade6f799ed2a5e1903de69482db9fcd3cb1731a5fee4a504adede5fca6f')
# Maintained owner gates scalar dataset/split before decoding any task/document
# text. Nonselected object values stay opaque bytes, including Hotpot/FINAL.
selected=recipe.train_only_corpus(corpus_raw);tasks=selected['tasks'];docs={d['id']:d for d in selected['documents']};check('original QASPER107TRAIN36families',len(tasks)==107 and len({t['source_group']for t in tasks})==36 and all(t['dataset']=='QASPER'and t['split']=='train'and t['publisher_split']=='train'for t in tasks))
nontrain=set()
for item in recipe._objects(corpus_raw,'tasks'):
 dataset=json.loads(source._field(item,'dataset'))
 if dataset!='QASPER':continue
 split=json.loads(source._field(item,'split'));family=json.loads(source._field(item,'source_group'))
 if split!='train':nontrain.add(family)
old=load(M/'a1-next-experiment-owner-manifest-r1/old-qasper-family-versions.json');old_by_id={x['paper_id']:x for x in old['families']};freeze=load(M/'qasper-fresh-final-candidate-freeze-r4.json');extra_dev=old['extra_development_family'].split(' ')[0]
excluded=set(nontrain)|{'qasper:'+x['paper_id']for x in old['families']if x['publisher_split']!='train'}|{'qasper:'+x['paper_id']for x in freeze['rows']}|{'qasper:'+extra_dev}
rights=json.loads(source._field(corpus_raw,'qasper_rights_decisions'));decision={x['paper_id']:x for x in rights};title_groups=collections.defaultdict(set);text_groups=collections.defaultdict(set)
for row in rights:
 title_groups[' '.join(unicodedata.normalize('NFKC',row['title']).casefold().split())].add('qasper:'+row['paper_id']);text_groups[row['source_text_sha256']].add('qasper:'+row['paper_id'])
for groups in [title_groups,text_groups]:
 for group in groups.values():
  if group&excluded:excluded.update(group)
check('no originalTRAIN family heldout collision',not({t['source_group']for t in tasks}&excluded))
archive_path=M/'qasper-rights-audit-v03/qasper-train-dev-v0.3.tgz';archive_raw=raw(archive_path);check('exact publisher archive',len(archive_raw)==10835856 and logic.sha(archive_raw)=='a28fdf966db827bcee3d873107d6b6669864fb7ca8fbf73a192f5e39191bdb5a')
with tarfile.open(fileobj=io.BytesIO(archive_raw),mode='r:gz')as archive:
 member=archive.getmember('qasper-train-v0.3.json');check('publisherTRAIN member only',member.isfile()and member.size<32*1024*1024);publisher=archive.extractfile(member).read(member.size+1);check('publisherTRAIN member bounded',len(publisher)==member.size)
# Single-pass structural selection. Only the36 already-adopted TRAIN paper
# objects are decoded; neither publisherDEV nor publisherTEST member is opened.
needed={t['source_group'].split(':')[1]for t in tasks};papers={};at=source._white(publisher,0);assert publisher[at]==123;at+=1
while True:
 at=source._white(publisher,at)
 if publisher[at]==125:break
 end=source._string_end(publisher,at);key=json.loads(publisher[at:end]);at=source._white(publisher,end);assert publisher[at]==58;begin=source._white(publisher,at+1);end=source._end(publisher,begin)
 if key in needed:papers[key]=json.loads(publisher[begin:end])
 at=source._white(publisher,end)
 if publisher[at]==44:at+=1
 else:assert publisher[at]==125
check('only36selectedTRAINpaperobjectsdecoded',set(papers)==needed)
for paper_id,paper in papers.items():
 d=decision[paper_id];check('sourcefulltext '+paper_id,logic.sha(logic.canonical(paper['full_text']))==d['source_text_sha256']and paper['title']==d['title']and d['publisher_split']=='train');meta_path=M/'qasper-rights-audit-v03'/('arxiv-'+paper_id+'.html');check('sourcev1rightsmetadata '+paper_id,logic.sha(raw(meta_path))==old_by_id[paper_id]['metadata_sha256'])
ledger=[];candidates=[];all_positive=[];copied={}
for task in tasks:
 family=task['source_group'];logic.gate(task,excluded);paper_id=family.split(':')[1];paper=papers[paper_id];original=next(q for q in paper['qas']if q['question_id']==task['question_id']);check('question exact '+task['question_id'],original['question']==task['question']);byann={x['annotation_id']:x for x in original['answers']};locations=logic.positive_locations(task,docs,byann)
 record={'id':task['id'],'family':family,'upstream_role':'publisher_TRAIN_retained_TRAIN','question_sha256':logic.sha(task['question'].encode()),'original_alternative_count':len(task['human_alternatives']),'human_answerable_fully_aligned_positive_locations':len(locations),'native_fit':'UNOBSERVED','refusal':None if locations else 'NO_COMPLETE_ORIGINAL_ANSWERABLE_HUMAN_ALTERNATIVE_WITH_ALLOWED_EXACT_PARAGRAPH'}
 ledger.append(record)
 if not locations:continue
 # Exactly one fixed paragraph per question from exactly one original human
 # alternative, before native compilation/scoring. Never union alternatives.
 alt,docid=min(locations,key=lambda x:logic.sha((logic.DOMAIN+'paragraph:'+task['id']+x[0]['human_annotation_id']+x[1]).encode()));doc=docs[docid];body=doc['text'].encode();check('whole original paragraph '+docid,logic.sha(body)==doc['paragraph_sha256'])
 if docid not in copied:copied[docid]=put('qasper-source-paragraphs/'+doc['paragraph_sha256']+'.txt',body)
 d=decision[paper_id];annotation=byann[alt['human_annotation_id']];row={'id':task['id'],'dataset':'QASPER','family':family,'label':1,'rights_review':'PERMITTED_PRIVATE_RESEARCH_SCOPE','question':task['question'],'question_sha256':record['question_sha256'],'source_pin':copied[docid],'span':{'start':0,'end':len(body)},'window_sha256':doc['paragraph_sha256'],'original_source_id':docid,'native_fit':'UNOBSERVED','human_claim':{'annotation_id':alt['human_annotation_id'],'original_record_sha256':alt['original_record_sha256'],'original_evidence_sha256':doc['paragraph_sha256'],'original_evidence_count':alt['original_evidence_count'],'entire_alternative_alignment':alt['alignment'],'selected_single_paragraph_only':True,'complete_evidence_or_terminal_label':False,'authority':'original QASPER human explicitly selected evidence paragraph','source_archive_sha256':logic.sha(archive_raw),'source_archive_member':'qasper-train-v0.3.json','source_member_sha256':logic.sha(publisher),'original_question_id':task['question_id'],'original_answer_record':annotation},'source_rights':{'license':'CC-BY-4.0','url':doc['url'],'title':doc['title'],'attribution':d['attribution'],'source_text_sha256':d['source_text_sha256'],'exact_version':'v1','metadata':pins[str((M/'qasper-rights-audit-v03'/('arxiv-'+paper_id+'.html')).resolve())],'scope':'original retained private source/relevance scope; not new training release or distribution approval'},'wholefamily_exposure':'originalTRAINalreadyacquired/audited/previouslyconsumed; not pristineFINAL or newsource'}
 candidates.append(row);all_positive.append({'id':task['id'],'selected_annotation_id':alt['human_annotation_id'],'selected_paragraph':docid,'all_original_positive_location_ids':sorted({x[1]for x in locations}),'selected_before_native_fit':True})
check('original107denominator retained',len(ledger)==107);check('no unknown/replayed question identity',len({x['id']for x in candidates})==len(candidates));draws=logic.round_robin(candidates,45);check('fixed45uniqueTRAINdraws',len(draws)==45 and len({r['id']for r in draws})==45);check('fixed45familyrolespreserved',all(r['family']not in excluded for r in draws))
q_all=put('qasper-all107-question-ledger.json',ledger);q_candidates=put('qasper-all-positive-candidates.json',candidates);q_draws=put('qasper-fixed45-draws.json',draws);put('qasper-positive-alternative-location-metadata.json',all_positive)
# Separate software-document arm: original TechQA ANSWER and literal offsets
# prove a positive relevance anchor; this does not become general-domain replay.
tq_path=B/'techqa-source-disjoint-positive-pairs-r5/research-source-manifest.json';tq=load(tq_path);original_qa=load(tq['original_600_TRAIN_pin']['path']);check('original600TRAIN source pinned',logic.sha(Path(tq['original_600_TRAIN_pin']['path']).read_bytes())==tq['original_600_TRAIN_pin']['sha256']);qb={r['QUESTION_ID']:r for r in original_qa};fence=load(tq['fence_pin']['path']);check('entireDEVsourceIDfence pinned',logic.sha(Path(tq['fence_pin']['path']).read_bytes())==tq['fence_pin']['sha256']);denied=set(fence['entire_DEV_source_id_closure']);td={d['original_document_id']:d for d in tq['documents']};tc=[];tledger=[]
for t in tq['tasks']:
 d=td[t['source_document_id']];b=raw(d['source_path']);text=b.decode('utf8');original=qb[t['original_TRAIN_id']];start,end=t['derived_utf8_start'],t['derived_utf8_end'];human=t['original_human_answer'];check('TechQA originalpositive '+t['original_TRAIN_id'],original['ANSWERABLE']=='Y'and original['DOCUMENT']==t['source_document_id']and original['ANSWER']==human and int(original['START_OFFSET'])==t['original_human_char_start']and int(original['END_OFFSET'])==t['original_human_char_end']and text[t['original_human_char_start']:t['original_human_char_end']]==human and b[start:end].decode()==human and logic.sha(b)==d['source_sha256']==t['source_sha256']and t['source_document_id']not in denied)
 # Whole paragraph range enclosing the entire original answer; source-preserved
 # context is not itself presented as a newly human-annotated positive window.
 before=b.rfind(b'\n\n',0,start);ws=0 if before<0 else before+2;after=b.find(b'\n\n',end);we=len(b)if after<0 else after;window=b[ws:we];check('entirehumananswerinwindow '+t['original_TRAIN_id'],ws<=start<end<=we and b[start:end]in window)
 row={'id':t['original_TRAIN_id'],'dataset':'TechQA','family':'IBM-original-technical-documents','label':1,'rights_review':'PERMITTED_PRIVATE_RESEARCH_SCOPE','question':t['question'],'question_sha256':logic.sha(t['question'].encode()),'source_pin':{'path':d['source_path'],'bytes':len(b),'sha256':d['source_sha256']},'span':{'start':ws,'end':we},'window_sha256':logic.sha(window),'human_claim':{'authority':'original TechQA TRAIN ANSWER+character offsets','answer_sha256':logic.sha(human.encode()),'original_char_start':t['original_human_char_start'],'original_char_end':t['original_human_char_end'],'answer_utf8_start':start,'answer_utf8_end':end,'positive_anchor_complete':True,'window_is_new_human_judgment':False,'source_document_id':t['source_document_id'],'original_600TRAIN_pin':tq['original_600_TRAIN_pin']},'rights':{'data_license':tq['data_license'],'sourceID_disjoint':True,'fence_pin':tq['fence_pin'],'product_family_chronology_closed':False},'native_fit':'UNOBSERVED','candidate_policy_refusal':None if len(window)<=8192 else 'ORIGINAL_ANSWER_PARAGRAPH_RANGE_EXCEEDS_8192_SOURCE_BYTES','replay_kind':'software-document-positive-only_not_general-domain'}
 tc.append(row);tledger.append({'id':row['id'],'source_document_id':t['source_document_id'],'candidate_policy_refusal':row['candidate_policy_refusal'],'native_fit':'UNOBSERVED'})
eligible=[r for r in tc if r['candidate_policy_refusal']is None];t45=sorted(eligible,key=lambda r:logic.sha((logic.DOMAIN+'TechQA:'+r['id']).encode()))[:45];check('separateTechQA45fixedcandidatecount',len(t45)==45);put('techqa-all290-candidates.json',tc);put('techqa-all290-ledger.json',tledger);put('techqa-software-document-fixed45.json',t45)
rights_license=B/'techqa-source-disjoint-positive-pairs-r5/CDLA-Permissive-v1.0.original.pdf';raw(rights_license)
report={'schema':'airk.private.existing-replay-candidate-preparation/1','identity':identity,'scope':'candidate exact human-positive evidence only, native compiler/scoring/gradient/training/promotion unavailable until separate owner execution','qasper':{'originalTRAIN_questions':107,'originalTRAIN_families':36,'selectedTRAIN_doc_texts_decoded':len(docs),'nonTRAIN_or_Hotpot_texts_decoded':0,'positive_candidate_questions':len(candidates),'fixed45draws':len(draws),'fixed45families':len({r['family']for r in draws}),'all_candidates':q_candidates,'full107ledger':q_all,'draws':q_draws,'excluded_wholefamilies':sorted(excluded),'old65allversionfreshness_exclusion':old['all_version_exclusion'],'oldTRAINreuse_not_newsource':True,'native_fit':'UNOBSERVED','score_based_or_fit_refill':False,'general_document_candidate':True},'techqa':{'source_tasks':len(tc),'source_documents':len(td),'exact_human_answer_span_witnesses_verified':len(tc),'paragraph_ranges_over8192':len(tc)-len(eligible),'fixed45candidate_draws':len(t45),'use':'separate software-document-positive arm, never silentlygeneral-domain replay'},'controls':{'selected':len(checks),'passed':sum(x['passed']for x in checks)},'checks':checks,'new_acquisition':False,'model_or_native_or_GPU_or_Cargo':False,'teacher_or_new_human_annotation':False,'oldinputs_mutated':False,'elapsed_seconds':time.perf_counter()-started,'remaining':['Root native whole512 compiler census under exact two instructions; original refusals remain withoutrefill.','Root independent acceptance of exact originalpositive-window claim +rights/exposure and bounded ownedtraining card.','No claim of complete evidence, ranking grades, terminal gold, clean pretraining, broadsoftware/heldout quality or promotion.']}
put('report.json',report)
for p,pin in pins.items():check('input unchanged '+p,logic.sha(Path(p).read_bytes())==pin['sha256'])
put('input-custody.json',pins);put('actual-data-controls.json',{'selected':len(checks),'passed':sum(x['passed']for x in checks),'checks':checks});print(json.dumps({'report':str(OUT/'report.json'),'sha256':logic.sha((OUT/'report.json').read_bytes()),'qasper_positive_candidates':len(candidates),'draw45families':len({r['family']for r in draws}),'qasper_draws_sha256':q_draws['sha256'],'TechQA_candidates':len(tc),'control_count':len(checks),'elapsed_seconds':time.perf_counter()-started}))
