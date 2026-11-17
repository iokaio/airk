# SPDX-License-Identifier: Apache-2.0
import pathlib,json,collections,hashlib,re,ast,datetime,time,urllib.parse
ROOT=pathlib.Path.cwd();P=ROOT/'artifacts/advanced-split/models/actual-composed-service-r2/public-human-label-admission-r1';J=P/'cosqa-source-join-r1';O=P/'split-clone-chronology-r1';B=ROOT/'artifacts/advanced-split/models';started=time.monotonic()
def load(p):return json.loads(pathlib.Path(p).read_text(encoding='utf8'))
def sha(b):return hashlib.sha256(b).hexdigest()
def pin(p):
 p=pathlib.Path(p);b=p.read_bytes();return dict(path=str(p.resolve()),bytes=len(b),sha256=sha(b))
inputs=[J/'source-rights-applicability-review-r1/review.json',J/'source-and-rights-candidate-tasks.json',J/'source-and-rights-joined-functions.json',J/'canonical-repository-identities.json',P/'techqa-source-disjoint-positive-pairs-r5/research-source-manifest.json',P/'contextbench150-local-research-rights-proposal.json',P/'contextbench-current-blind-train-r1/full450-source-audit-r1/receipt.json',B/'a1-next-experiment-owner-manifest-r1/old-qasper-family-versions.json',B/'a1-next-experiment-owner-manifest-r1/consumed-indexed-source-versions.json',B/'qasper-version-set-and-private-final-proposal-r1.json',B/'qasper-fresh-final-candidate-freeze-r4.json',B/'qasper-fresh-final-execution-r4.json']
inputpins=[pin(p)for p in inputs];rights=load(inputs[0]);tasks=load(inputs[1]);funcs=load(inputs[2]);canon=load(inputs[3]);tq=load(inputs[4]);c150=load(inputs[5]);cb=load(inputs[6]);oldq=load(inputs[7]);consumed=load(inputs[8]);qproposal=load(inputs[9]);qfreeze=load(inputs[10]);qexec=load(inputs[11]);fn={str(f['code_idx']):f for f in funcs};rightsrows={r['original_train_idx']:r for r in rights['human_pairs']};familymap={};canonrows=[]
for c in canon['results']:
 if c['status']!=200:continue
 identity=c['source_id'] or c['parent_id'] or c['id'];familymap[c['requested_repository'].lower()]='github-id:'+str(identity);familymap[c['full_name'].lower()]='github-id:'+str(identity);canonrows.append({k:c[k]for k in ['requested_repository','full_name','id','fork','parent_id','source_id']})
def repo_key(s):return familymap.get(s.lower(),'github-name-unverified-fork:'+s.lower())
rows=[];sources={};symbol_rows=[]
def source_add(f,dataset,repo,role):
 p=f.get('path') or f.get('source_path');h=f.get('sha256')or f.get('source_sha256');key=(str(pathlib.Path(p).resolve()),h)
 if key not in sources:sources[key]=dict(path=key[0],sha256=h,bytes=f.get('bytes'),datasets=set(),repos=set(),roles=set(),logical_path=f.get('upstream_path')or f.get('logical_path')or pathlib.Path(p).name)
 sources[key]['datasets'].add(dataset);sources[key]['repos'].add(repo);sources[key]['roles'].add(role)
for t in tasks:
 f=fn[str(t['code_idx'])];r=rightsrows[t['original_train_idx']];src=f['pinned_source_file'];status=r['rights_review'];source_add(src,'CoSQA',t['repository'],status);symbol=f['original_source']['function'];pk=f['original_source']['parsed_source'];rows.append(dict(dataset='CoSQA',id=t['original_train_idx'],original_role='publisher_TRAIN',rights=status,repo=t['repository'],repository_family=repo_key(t['repository']),commit=t['commit'],question_sha256=t['question_sha256'],labeled_code_sha256=t['code_sha256'],source_sha256=src['sha256'],source_path=pk['url_path'],symbol=symbol,symbol_family=(repo_key(t['repository'])+'|'+pk['url_path']+'|'+symbol),label=t['label'],source_time=None,question_time=None))
docmap={d['original_document_id']:d for d in tq['documents']};prod={};productnames=collections.defaultdict(set)
for d in tq['documents']:
 m=d['original_metadata'];ids=[s.strip()for s in m['productId'].split(',')];assert all(re.fullmatch('[A-Z0-9]+',s)for s in ids);prod[d['original_document_id']]=ids
 for s in ids:productnames[s].add(m['productName'])
 source_add(dict(path=d['source_path'],sha256=d['source_sha256'],bytes=d['bytes']),'TechQA','IBM-original-documentation','source-disjoint-positive-pair-only')
for t in tq['tasks']:
 d=docmap[t['source_document_id']];rows.append(dict(dataset='TechQA',id=t['original_TRAIN_id'],original_role='publisher_TRAIN',rights='original-CDLA1.0-retained-source-candidate',repo=None,repository_family=None,product_ids=prod[t['source_document_id']],source_document_id=t['source_document_id'],question_sha256=sha(t['question'].encode()),source_sha256=t['source_sha256'],human_answer_sha256=sha(t['original_human_answer'].encode()),source_time=d['original_metadata']['date'],source_time_semantics='publisher document metadata date; creation/update distinction not established',question_time=None,whole_original_menu_source_disjoint=not t['contains_heldout_ID_in_original_candidate_pool']))
selected={r['original_inst_id']for r in c150['rows']};assert len(selected)==150
ready=[r for r in cb['results']if r['whole_case_source_complete']];assert len(ready)==280
for c in ready:
 ss=[]
 for s in c['spans']:
  z=s['source_pin'];source_add(z,'ContextBench',c['repo'],'multifile150'if c['original_inst_id']in selected else'source-complete280-pool');ss.append(z['sha256'])
 issue=c['original_inst_id'];rows.append(dict(dataset='ContextBench',id=issue,publisher_instance_id=c['instance_id'],original_role='publisher_original_TRAIN_ID',source_pool='multifile150'if issue in selected else'other_source_complete280',rights='separately-proposed-source-disposition',repo=c['repo'],repository_family=repo_key(c['repo']),commit=c['base_commit'],issue_family=repo_key(c['repo'])+'|'+issue,question_sha256=c['question_sha256'],source_sha256_bag=sorted(set(ss)),language=c['language'],issue_origin=c['source'],source_time=None,question_time=None))
assert len(rows)==2525
# Product families: shared ANY atomic product ID, not distinct raw comma-joined labels.
class DSU:
 def __init__(self,keys):self.p={k:k for k in keys}
 def find(self,k):
  while self.p[k]!=k:self.p[k]=self.p[self.p[k]];k=self.p[k]
  return k
 def union(self,a,b):
  a,b=self.find(a),self.find(b)
  if a!=b:self.p[max(a,b)]=min(a,b)
pu=DSU(prod);seen={}
for d,ids in prod.items():
 for k in ids:
  if k in seen:pu.union(d,seen[k])
  seen[k]=d
pcs=collections.defaultdict(list)
for d in prod:pcs[pu.find(d)].append(d)
products=[]
for group,docs in pcs.items():
 ids=sorted({k for d in docs for k in prod[d]});ts=[r for r in rows if r['dataset']=='TechQA'and r['source_document_id']in docs];products.append(dict(component=group,atomic_product_ids=ids,source_document_ids=sorted(docs),source_documents=len(docs),human_pairs=len(ts),date_min=min(docmap[d]['original_metadata']['date']for d in docs),date_max=max(docmap[d]['original_metadata']['date']for d in docs)))
# Exact equality graphs, preserving ORIGINAL row/label multiplicity.
def groups(field):
 out=collections.defaultdict(list)
 for r in rows:
  vals=r.get(field,[]);vals=vals if isinstance(vals,list)else[vals]
  for value in vals:
   if value:out[value].append(r)
 return [{'fingerprint':k,'rows':[dict(dataset=r['dataset'],id=r['id'],repo=r.get('repo'),label=r.get('label'))for r in v],'datasets':sorted({r['dataset']for r in v}),'repositories':sorted({r['repo']for r in v if r.get('repo')})}for k,v in out.items()if len(v)>1]
qdups=groups('question_sha256');code_dups=groups('labeled_code_sha256');symboldups=groups('symbol_family');issues=groups('issue_family');both=collections.defaultdict(list)
for r in rows:
 if r['dataset']=='CoSQA':both[(r['question_sha256'],r['labeled_code_sha256'])].append(r)
contradictions=[dict(question_sha256=k[0],code_sha256=k[1],ids=[r['id']for r in v],labels=sorted({r['label']for r in v}))for k,v in both.items()if len({r['label']for r in v})>1]
# All read source files here are the originalTRAIN closure only; no heldout source bodies.
source_groups=collections.defaultdict(list);ast_groups=collections.defaultdict(list);astref=collections.Counter();total=0;verified=0
assert len(sources)<=3000
for s in sources.values():
 b=pathlib.Path(s['path']).read_bytes();assert sha(b)==s['sha256'];assert s['bytes']is None or len(b)==s['bytes'];s['bytes']=len(b);total+=len(b);assert total<=150*1024*1024;verified+=1;source_groups[s['sha256']].append(s)
 if 'TechQA'in s['datasets']or not str(s['logical_path']).endswith('.py'):continue
 if len(b)>2*1024*1024:astref['over_2MiB_scope_refusal']+=1;continue
 try:tree=ast.parse(b.decode('utf8'));astref['parsed_files']+=1
 except (SyntaxError,UnicodeDecodeError,ValueError,RecursionError):astref['parse_refusals']+=1;continue
 lines=b.decode('utf8').splitlines(keepends=True)
 for node in ast.walk(tree):
  if isinstance(node,(ast.FunctionDef,ast.AsyncFunctionDef))and sum(1 for _ in ast.walk(node))>=25:
   body=''.join(lines[node.lineno-1:node.end_lineno]);
   if len(body.encode())<120:continue
   dump=ast.dump(node,annotate_fields=True,include_attributes=False);fp=sha(dump.encode());ast_groups[fp].append(dict(source_sha256=s['sha256'],datasets=sorted(s['datasets']),repos=sorted(s['repos']),symbol=node.name,start_line=node.lineno,end_line=node.end_lineno,utf8_bytes=len(body.encode())));astref['substantive_functions']+=1
for s in sources.values():
 for k in ['datasets','repos','roles']:s[k]=sorted(s[k])
source_copies=[dict(sha256=h,files=ss,datasets=sorted({d for s in ss for d in s['datasets']}),repos=sorted({r for s in ss for r in s['repos']}))for h,ss in source_groups.items()if len(ss)>1]
clones=[dict(ast_sha256=h,occurrences=v,datasets=sorted({d for z in v for d in z['datasets']}),repos=sorted({d for z in v for d in z['repos']}))for h,v in ast_groups.items()if len({z['source_sha256']for z in v})>1 and len({r for z in v for r in z['repos']})>1]
# Combine conservative atomic families for a before-score proposal, not assigned splits.
family_nodes={r['repository_family']for r in rows if r.get('repository_family')};uf=DSU(family_nodes)
for c in code_dups+qdups+source_copies:
 rr=c.get('repositories')or c.get('repos')or[];kk=sorted({repo_key(x)for x in rr})
 for k in kk[1:]:uf.union(kk[0],k)
mandatory=collections.defaultdict(list)
for k in family_nodes:mandatory[uf.find(k)].append(k)
repo_overlap=sorted(set(r['repo'].lower()for r in rows if r['dataset']=='CoSQA'and r['rights']=='PERMITTED_PRIVATE_RESEARCH_SCOPE')&set(r['repo'].lower()for r in rows if r['dataset']=='ContextBench'))
# QASPER identity/version metadata only; no payload, questions, evidence or source bodies read.
oldids={r['paper_id']for r in oldq['families']};frozenids={r['paper_id']for r in qfreeze['rows']};consumedids=set()
for r in consumed['rows']:
 for field in ['id','source_group','file','original_url']:
  consumedids.update(re.findall(r'\b\d{4}\.\d{4,5}\b',str(r.get(field,''))))
qid='1707.07212';candidate=dict(paper_id=qid,scope='metadata-only possible future reservation candidate, not freshFINAL claim or adopted reservation',in_old65=qid in oldids,in_frozen14=qid in frozenids,in_reviewed_consumed_source_version_ids=qid in consumedids,source_metadata_reference=str(inputs[9].resolve()),remaining_unknowns=['Complete exposure ledger/source-version closure beyond reviewed metadata','Actual original publisher TEST identity/license/version acceptance','Any later gold opening must be separate reviewed opaque materialization','Adequate population/power; three rawquestions is not a completeFINAL cohort'])
qasper=dict(historical_retained_counts=dict(TRAIN=107,DEV=14,CAL=29,TEST=44),old65_paper_families=len(oldids),old65_publisher_split_counts=dict(collections.Counter(r['publisher_split']for r in oldq['families'])),all_old_family_versions_excluded=True,frozen14_materialization=dict(paper_ids=sorted(frozenids),raw_questions=qfreeze['raw_question_count'],aggregate=qexec['aggregate'],never_treat_as_new_unconsumed_FINAL=True),reviewed_consumed_metadata_paper_ids=sorted(consumedids),metadata_only_optional_candidate=candidate,new_FINAL_reservation_frozen=False)
summary=dict(schema='airk.private.TRAIN-split-clone-chronology-audit/1',utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),input_pins=inputpins,populations=dict(cosqa_all=1955,cosqa_supported_private_source_pairs=1869,cosqa_unknown_pairs=86,techqa_source_disjoint_positive_pairs=290,techqa_source_documents=254,context_source_complete_pool=280,context_multifile_subset=150),source_files_verified=verified,source_bytes_verified=total,source_files_source_scope='OriginalTRAIN/source-complete280 pool; no heldout source bodies',counts=dict(exact_question_duplicate_groups=len(qdups),exact_labeled_code_duplicate_groups=len(code_dups),symbol_family_multiple_rows=len(symboldups),issue_family_duplicate_groups=len(issues),same_question_code_conflicting_label_groups=len(contradictions),source_copy_groups=len(source_copies),cross_dataset_source_copy_groups=sum(len(x['datasets'])>1 for x in source_copies),substantive_cross_repository_AST_copy_candidate_groups=len(clones),cross_dataset_AST_copy_candidates=sum(len(x['datasets'])>1 for x in clones)),ast_candidate_scope=dict(counters=dict(astref),rule='Exact Python AST dump with original names/constants; include_attributes=False; >=25ASTnodes and>=120UTF8source bytes; candidate clone only, no gold/status/admission inference; no rename tolerant/fuzzy/semantic matching.'),supported_cosqa_context_repository_name_intersections=repo_overlap,canonical_cosqa_current_metadata=canonrows,context_fork_identity_status='Literalcase-insensitive and sharedCoSQAcatalogaliases only; currentContextfork/parent taxonomy not comprehensively acquired, no claim unrelated families closed.',repository_family_constraint_components=sorted(mandatory.values(),key=lambda v:(-len(v),v)),techqa_product_metadata=dict(documents=254,all_five_original_fields_present=True,fields=['sourceDocumentId','date','productName','productId','canonicalUrl'],literal_productId_strings=len({d['original_metadata']['productId']for d in tq['documents']}),atomic_product_ids=len(productnames),product_components=len(products),largest_component_source_documents=max(x['source_documents']for x in products),largest_component_pairs=max(x['human_pairs']for x in products),source_date_min=min(d['original_metadata']['date']for d in tq['documents']),source_date_max=max(d['original_metadata']['date']for d in tq['documents']),family_caveat='Atomicproduct co-occurrence components are defensible lower-bound grouping constraints. Producthierarchy/platform/aliases beyond sharedIDs and relationtoheldoutDEVproductfamilies remain unknown. IBM productgroups are not unrelated software repos.'),qasper=qasper,split_roles_assigned=False,pilot_size_frozen=False,model_scoring=False,teacher=False,training=False,nonTRAIN_gold_or_source_context_read=False,elapsed_seconds=time.monotonic()-started)
for pinrow in inputpins:assert pin(pinrow['path'])==pinrow
artifacts={'summary.json':summary,'TRAIN-row-family-descriptors.json':rows,'source-file-descriptors.json':list(sources.values()),'techqa-product-components.json':products,'exact-question-duplicates.json':qdups,'exact-labeled-code-duplicates.json':code_dups,'same-question-code-conflicts.json':contradictions,'exact-source-copy-groups.json':source_copies,'substantive-python-AST-copy-candidates.json':clones}
for n,v in artifacts.items():
 with(O/n).open('x',encoding='utf8')as f:json.dump(v,f,indent=2,ensure_ascii=True)
print(json.dumps(dict(summary=pin(O/'summary.json'),counts=summary['counts'],techqa=summary['techqa_product_metadata'],overlap=repo_overlap,languages150=dict(collections.Counter(r['language']for r in rows if r['dataset']=='ContextBench'and r['source_pool']=='multifile150')),repository_components=len(mandatory),qasper_possible=candidate,elapsed_seconds=summary['elapsed_seconds']),ensure_ascii=True))
