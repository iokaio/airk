# SPDX-License-Identifier: Apache-2.0
import pathlib,json,hashlib,collections,math,datetime
O=pathlib.Path(__file__).resolve().parent;P=O.parent;J=P/'cosqa-source-join-r1';N=J/'native-visibility-preparation-r2';load=lambda p:json.loads(pathlib.Path(p).read_text(encoding='utf8'))
def sha(x):return hashlib.sha256(x).hexdigest()
def pin(p):
 p=pathlib.Path(p);b=p.read_bytes();return dict(path=str(p.resolve()),bytes=len(b),sha256=sha(b))
paths=[N/'launch-native-optimized-r1/visibility-summary.json',N/'launch-native-optimized-r1/visible-arm-review-candidates.json',N/'launch-native-optimized-r1/execution.json',J/'source-rights-applicability-review-r1/review.json',J/'source-and-rights-candidate-tasks.json',J/'canonical-repository-identities.json',P/'split-clone-chronology-r2/packet.json',P/'split-clone-chronology-r1/substantive-python-AST-copy-candidates.json']
pins=[pin(p)for p in paths];ns,arm,ex,rights,tasks,canon,split,clones=map(load,paths);assert ex['complete']and ex['failure']is None and not ex['whole_phase_expired'];assert ns['all_original_rows']==1955 and arm['F2_candidate_count']==1867;ids=set(arm['F2_source_window_candidate_ids']);rights_by_id={r['original_train_idx']:r for r in rights['human_pairs']};obs={r['id']:r for r in ns['per_original_row_observations']};family={};canonical_name={}
for c in canon['results']:
 if c['status']==200:
  key='github-id:'+str(c['source_id']or c['parent_id']or c['id'])
  for name in [c['requested_repository'],c['full_name']]:family[name.lower()]=key;canonical_name[name.lower()]=c['full_name']
# No label/score/count affects the group permutation. The two CoSQA clone-related projects co-locate.
family_edges=[]
for c in clones:
 rr=[r for r in c['repos']if r.lower()in family]
 if len(rr)>1:family_edges.append([family[r.lower()]for r in rr])
keys={family[t['repository'].lower()]for t in tasks if t['original_train_idx']in ids};par={k:k for k in keys}
def find(k):
 while par[k]!=k:par[k]=par[par[k]];k=par[k]
 return k
for edge in family_edges:
 for b in edge[1:]:a,b=find(edge[0]),find(b);par[max(a,b)]=min(a,b)
groups=collections.defaultdict(set)
for k in keys:groups[find(k)].add(k)
assert len(groups)==36
component_key={k:'|'.join(sorted(v))for v in groups.values()for k in v};domain='airk.f2.code-pilot.metadata-proposal/1\n';ordered=sorted(set(component_key.values()),key=lambda k:sha((domain+k).encode()));roles={k:('TRAIN'if i<20 else'DEV'if i<24 else'CAL_RESERVED'if i<28 else'CONFIRMATION_RESERVED'if i<32 else'RESEARCH_TEST_RESERVED')for i,k in enumerate(ordered)};rows=[]
for t in tasks:
 id=t['original_train_idx'];r=rights_by_id[id];repo=t['repository'];fam=family[repo.lower()];component=component_key.get(fam);eligible=id in ids;state='ELIGIBLE_ORIGINAL_PAIR'if eligible else'QUARANTINED_UNKNOWN_SOURCE_RIGHTS'if r['rights_review']!='PERMITTED_PRIVATE_RESEARCH_SCOPE'else'FOCUS512_OVERFLOW_REFUSAL';role=roles[component]if component else'QUARANTINE_NO_ELIGIBLE_COMPONENT'
 if eligible:assert r['rights_review']=='PERMITTED_PRIVATE_RESEARCH_SCOPE'and obs[id]['focus_compile_visibility']=='accepted'
 rows.append(dict(id=id,code_idx=t['code_idx'],published_binary_label=t['label'],target_token=9693 if t['label']==1 else2152,repository=repo,canonical_repository_name=canonical_name[repo.lower()],family=fam,component=component,proposed_role=role,eligibility=state,source_url=t['source_url'],question_sha256=t['question_sha256'],labeled_code_sha256=t['code_sha256'],native_compiled_row_id=id,native_report=ns['native_report'],native_gold_admission=False,proposal_training_admission=False))
role_stats={};component_rows=[]
for key in ordered:
 z=[r for r in rows if r['component']==key and r['eligibility']=='ELIGIBLE_ORIGINAL_PAIR'];assert z;labs=collections.Counter(r['published_binary_label']for r in z);assert set(labs)=={0,1};component_rows.append(dict(component=key,permutation_sha256=sha((domain+key).encode()),role=roles[key],repositories=sorted({r['repository']for r in z}),pairs=len(z),labels=dict(labs),functions=len({r['code_idx']for r in z})))
for role in sorted(set(roles.values())):
 z=[r for r in rows if r['proposed_role']==role and r['eligibility']=='ELIGIBLE_ORIGINAL_PAIR'];gs=[g for g in component_rows if g['role']==role];role_stats[role]=dict(components=len(gs),repositories=len({r['repository']for r in z}),rows=len(z),labels=dict(collections.Counter(r['published_binary_label']for r in z)),functions=len({r['code_idx']for r in z}),largest_component_rows=max(g['pairs']for g in gs))
assert sum(x['rows']for x in role_stats.values())==1867
# Preserve eval-only CSN roles. Reserve metadata query families without touching questions/comments.
csn=load(O/'CSN-eval-only-row-metadata.json');byq=collections.defaultdict(list)
for r in csn:
 if r['language']=='Python':byq[r['query_sha256']].append(r)
family_role={f:roles[component_key[f]]for f in keys};non_test={f for f,v in family_role.items()if v!='RESEARCH_TEST_RESERVED'};external_forced={}
for c in clones:
 known={family[r.lower()]for r in c['repos']if r.lower()in family and family[r.lower()]in keys}
 if len(known)==1:
  f=next(iter(known))
  for repo in c['repos']:
   if repo.lower()not in family:external_forced[repo.lower()]=f
eligible_q=[];excluded_q=[]
for q,z in byq.items():
 touched={family.get(r['repository_literal'].lower(),external_forced.get(r['repository_literal'].lower()))for r in z};bad=sorted(f for f in touched if f in non_test);grades={r['published_relevance']for r in z}
 if bad or len({r['source_url']for r in z})<2 or len(grades)<2:excluded_q.append(dict(query_sha256=q,non_test_families=bad,grade_count=len(grades),URLs=len({r['source_url']for r in z})))
 else:eligible_q.append(q)
reserved=sorted(eligible_q,key=lambda q:sha(('airk.f2.CSN.eval-only.query-reservation/1\n'+q).encode()))[:20];reserved_rows=[dict(**r,reservation='PROPOSED_EVAL_ONLY_RESEARCH_TEST_METADATA_NATIVE_RIGHTS_PENDING')for q in reserved for r in byq[q]];assert len(reserved)==20
for r in reserved_rows:assert family.get(r['repository_literal'].lower(),external_forced.get(r['repository_literal'].lower()))not in non_test
# Conditional independent-unit sign-test bounds; not an observed power estimate.
power=[]
for n in [4,8,12,16,20]:
 threshold=next((k for k in range(n//2+1,n+1)if 2*sum(math.comb(n,i)for i in range(k,n+1))/2**n<=.05),None);alpha=0 if threshold is None else 2*sum(math.comb(n,i)for i in range(threshold,n+1))/2**n;powr=0 if threshold is None else sum(math.comb(n,i)*.8**i*.2**(n-i)for i in range(threshold,n+1));power.append(dict(independent_components=n,minimum_positive_signs=threshold,exact_two_sided_alpha=alpha,power_if_P_positive_sign_equals_0_8=powr,scope='Conditional mathematical illustration only; no model/variance/effective-independence estimate.'))
summary=dict(schema='airk.private.F2-known-pair-before-score-proposal/1',utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),input_pins=pins,original_denominator=1955,rights_unknown_excluded=86,compiler_overflow_excluded=2,candidate_pairs=1867,candidate_labels=dict(collections.Counter(r['published_binary_label']for r in rows if r['eligibility']=='ELIGIBLE_ORIGINAL_PAIR')),candidate_functions=len({r['code_idx']for r in rows if r['eligibility']=='ELIGIBLE_ORIGINAL_PAIR'}),repositories=37,clone_group_components=36,assignment_rule=dict(domain=domain,algorithm='SHA256 ascending canonicalGitHubIDcomponent keys; first20TRAIN,next4DEV,next4CAL,next4CONFIRMATION,last4RESEARCH_TEST. Uses no model score,label value or desired rowcount.'),proposed_roles=role_stats,external_future_source_family_constraints=external_forced,CSN_eval_only_metadata_reservation=dict(publisher_use='EVALUATION_ONLY_NEVER_GRADIENTS',Python_query_families=len(byq),eligible_query_families_after_non_test_repo_and_grade_screen=len(eligible_q),reserved_query_families=len(reserved),annotation_rows=len(reserved_rows),source_urls=len({r['source_url']for r in reserved_rows}),literal_repositories=len({r['repository_literal'].lower()for r in reserved_rows}),raw_grades=dict(collections.Counter(r['published_relevance']for r in reserved_rows)),rights_scopes=dict(collections.Counter(r['source_file_rights_review']for r in reserved_rows)),known_source_code_window_or_native512_accepted=False,question_or_notes_decoded=False,not_FINAL=True,proof_pending=['Exactsource function/line/revision/literalwindow and perfile rights/notices','Wholefamily/fork/clone/query correlation closure acrossall20queryfamilies','ExactnativeFocus512compiler refusal census preservingevery reserved row, no refill','Originalmultihuman grades/aggregation remain benchmarkowned; no inventedbinarygrade','Adequateeffectivecluster power;20query hashes are not automatically20independentcases']),conditional_power_examples=power,training_objective='PlanStep10 response-position full-vocabulary crossentropy; labels1->yes9693,0->no2152; promptlossmasked. Not sigmoidBCE or inbatchcontrastive.',general_replay='Await exact Root/UI97candidate window/source pins, native512no-crop compilation and semanticpositive-window applicability; no replayrow or0replayfraction silently authorized.',instruction='Current native publisher instruction only; changing F1instruction invalidates these exactcompiled fit counts and requires native recompile/refusal census before freeze.',executable=False,scope_acceptance=False,split_adoption=False,training=False,model_scoring=False,FINAL=False)
for n,v in [('proposed-cohort-all1955.json',rows),('proposed-family-components.json',component_rows),('CSN-proposed-eval-only-20query-metadata.json',reserved_rows),('CSN-metadata-query-exclusions.json',excluded_q),('proposal-summary.json',summary)]:
 with(O/n).open('x')as f:json.dump(v,f,indent=2)
for p in pins:assert pin(p['path'])==p
print(json.dumps(dict(summary_sha256=pin(O/'proposal-summary.json')['sha256'],roles=role_stats,CSN=summary['CSN_eval_only_metadata_reservation'],external=external_forced,power=power),ensure_ascii=True))
