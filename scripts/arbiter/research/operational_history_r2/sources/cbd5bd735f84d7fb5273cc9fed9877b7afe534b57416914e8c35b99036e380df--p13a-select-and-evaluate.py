# SPDX-License-Identifier: Apache-2.0
"""Emit future serial harness commands or evaluate saved S1 quality receipts. Never builds/applies/indexes/queries."""
import argparse, hashlib, importlib.util, json, math, statistics, sys
from pathlib import Path
BASE=Path(__file__).resolve().parent
ROOT=BASE/'p10-worktree'
FROZEN={'p13a-additional-labels-freeze-r1.json':'8f301390a92369fae5ab08e5dd3a973baf3183b17e25e3e9b4dd6a38f16d2db4','p13a-additional-corpus-r1.json':'c9ed78ac0b9cd8c2bf982bb1fe4148c278d174edb3484a84025ff8b2b51d235d'}
def read(p):return json.loads(p.read_text(encoding='utf-8'))
def digest(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def freeze():
 for n,h in FROZEN.items():assert digest(BASE/n)==h,n
 return read(BASE/'p13a-additional-corpus-r1.json')
def save(p,v):
 with p.open('x',encoding='utf-8') as f:f.write(json.dumps(v,indent=2)+'\n')
def commands(a):
 freeze();out=a.output_dir.resolve();assert not out.exists(),'Fresh report directory required; never overwrite prior evidence'
 binaries=[('baseline',a.baseline_binary.resolve()),('candidate',a.candidate_binary.resolve())]
 rows=[]
 for cohort,corpus,detail in [('supplemental',BASE/'p13a-additional-corpus-r1.json','detailed'),('original',ROOT/'scripts/fixtures/retrieval-v190.json','agent_lean')]:
  for role,binary in binaries:
   rows.append({'name':role+'-'+cohort,'cwd':str(ROOT),'command':[sys.executable,str(ROOT/'scripts/check-retrieval-quality.py'),'--binary',str(binary),'--output',str(out/(role+'-'+cohort+'.json')),'--trials','3','--detail',detail,'--corpus',str(corpus),'--model-dir',str(a.model_dir.resolve())]})
 rows.append({'name':'unchanged-original-comparator','cwd':str(ROOT),'command':[sys.executable,str(BASE/'p9-compare-quality.py'),'--candidate',str(out/'candidate-original.json'),'--output',str(out/'original-frozen-controls.json')]})
 print(json.dumps({'status':'commands only; NOT executed','serial':True,'commands':rows,'boundaries':['P12 acceptance and independently reviewed immutable labels required BEFORE candidate output.','Validate exact existing build receipts/runtime seals/model/assets/profile/host before and after every stage. Same build profile, no concurrent CPU/resource workloads.','No build commands here. Retain accepted P12 matched release baseline; one eventual reviewed candidate release pair only if earlier functional/preflight gates pass. No debug/release latency comparison.','Detailed supplemental and AgentLean original are separate explicit LEGACY profiles. No new retainedLean/Rich qualification until canonical harness adapter exists; never alias profiles.','Then evaluate: python '+str(Path(__file__).resolve())+' evaluate --report-dir '+str(out)+' --output <fresh-quality-result.json>','Quality success alone cannot authorize adoption: independent labels, native scope/disclosure/parity, all84latency and required resource evidence remain separate gates.']},indent=2))
def attempts(p,corpus,detail):
 r=read(p);assert r['passed'] and r['source']==r['source_after'] and r['binary_sha256']==r['binary_sha256_after']
 assert r['index_detail']==detail
 m=r['representative'];assert m['complete'] and m['limit']==10 and m['max_bytes']==65536
 assert m['corpus_sha256']==digest(corpus)
 c=read(corpus);assert m['source_hashes']=={n:hashlib.sha256(s.encode()).hexdigest() for n,s in c['sources'].items()}
 expected={(q['id'],mode,t) for q in c['queries'] for mode in q['modes'] for t in range(3)};rows={}
 for v in m['attempts']:
  key=(v['case'],v['mode'],v['trial']);assert key in expected and key not in rows
  assert v['status']=='ok' and isinstance(v['latency_ms'],(float,int)) and math.isfinite(v['latency_ms']) and v['latency_ms']>=0
  q=next(q for q in c['queries'] if q['id']==v['case']);assert v['family']==q['family'] and v['split']==q['split']
  if q['grades']:
   for field in ['ndcg_at_10','recall_at_10']:assert isinstance(v[field],(float,int)) and math.isfinite(v[field]) and 0<=v[field]<=1
  else:assert isinstance(v['negative_empty'],bool)
  rows[key]=v
 assert set(rows)==expected
 return r,m,rows

def evaluate(a):
 corpus=freeze();checks=[];pending=['Independent source/grade/range review before outputs must be documented; same-author labels alone are supplemental, not independent held-out qualification.','Canonical ranker/window disclosure and actual native scope/alias/permanent-deny-before-window, byte-budget refusal, tie/restart/recovery and full CLI/MCP/Workbench parity gates.','All84 original latency gates, same-basis exclusive CPU<=1.20, RSS growth<=32MiB, index/storage/indexing work<=5% growth. Unknown required resource counters remain inconclusive, never zero.','Matched accepted retained-profile native harness support if claiming new Lean/Rich evaluation. Existing commands cover explicit legacyDetailed/AgentLean only.']
 try:
  cb,mb,bb=attempts(a.report_dir/'baseline-supplemental.json',BASE/'p13a-additional-corpus-r1.json','detailed')
  cc,mc,bc=attempts(a.report_dir/'candidate-supplemental.json',BASE/'p13a-additional-corpus-r1.json','detailed')
  assert mb['source_hashes']==mc['source_hashes'] and mb['model_manifest']==mc['model_manifest'] and mb['verification_manifest_sha256']==mc['verification_manifest_sha256']
  assert cb['platform']==cc['platform'] and cb['architecture']==cc['architecture']
  def check(name,passed,**values):checks.append({'gate':name,'passed':bool(passed),**values})
  family_means={}
  for family in ['code','document']:
   keys=[k for k,v in bb.items() if k[1]=='lexical' and v['family'].startswith(family)]
   before=statistics.mean(bb[k]['ndcg_at_10'] for k in keys);after=statistics.mean(bc[k]['ndcg_at_10'] for k in keys);family_means[family]=(before,after)
   check('lexical-family-nonregression-'+family,after+1e-12>=before,baseline=before,candidate=after)
  gain=statistics.mean(v[1]-v[0] for v in family_means.values());check('equal-family-lexical-macro-gain',gain+1e-12>=.02,observed=gain,required=.02)
  for key,old in bb.items():
   new=bc[key];positive=old['ndcg_at_10'] is not None;label='/'.join(map(str,key))
   if not positive:check('negative-empty/'+label,old['negative_empty'] and new['negative_empty'])
   elif key[1] in ['lexical','hybrid']:check('perquery-recall-nonregression/'+label,new['recall_at_10']+1e-12>=old['recall_at_10'])
   else:
    check('vector-unchanged/'+label,all(math.isclose(old[f],new[f],rel_tol=0,abs_tol=1e-12) for f in ['ndcg_at_10','recall_at_10']))
  original=ROOT/'scripts/fixtures/retrieval-v190.json';ob,omb,orb=attempts(a.report_dir/'baseline-original.json',original,'agent_lean');oc,omc,orc=attempts(a.report_dir/'candidate-original.json',original,'agent_lean')
  assert omb['model_manifest']==omc['model_manifest']==mb['model_manifest']
  for family,mode in sorted({(v['family'],v['mode']) for v in orb.values()}):
   keys=[k for k,v in orb.items() if (v['family'],v['mode'])==(family,mode)]
   p95=lambda vals:sorted(vals)[math.ceil(.95*len(vals))-1]
   old=p95([orb[k]['latency_ms'] for k in keys]);new=p95([orc[k]['latency_ms'] for k in keys]);check('original-warm-p95/'+family+'/'+mode,new<=max(old*1.10,old+5),baseline=old,candidate=new)
  oldchecks=read(a.report_dir/'original-frozen-controls.json');check('unchanged-original-quality-comparator',oldchecks['passed'] and oldchecks['candidate_sha256']==digest(a.report_dir/'candidate-original.json'))
  status='quality_only_pass_adoption_pending' if all(c['passed'] for c in checks) else 'retain_current'
  result={'status':status,'quality_gates_passed':status!='retain_current','checks':checks,'supplemental_attempts_per_pair':len(bb),'family_means':family_means,'required_pending_gates':pending,'scope':'Same-author supplemental file-level ranking; graded ranges are frozen but not scored by existing harness. Not blind, totalCPU/physicalpeak or full native qualification. No diversity S2 bundled.','evidence':{p.name:digest(p) for p in a.report_dir.glob('*.json')}}
 except (AssertionError,KeyError,TypeError,ValueError,OSError,ZeroDivisionError) as e:
  result={'status':'retain_current','quality_gates_passed':False,'inconclusive_or_malformed':type(e).__name__,'checks':checks,'required_pending_gates':pending,'policy':'Failed/missing/unknown evidence never produces adoption. Frozen labels unchanged; no fallback retries/query execution.'}
 result['decision']='retain_current';result['adoption_evidence_complete']=False
 save(a.output,result);print(json.dumps({'status':result['status'],'output':str(a.output)}))
 if not result['quality_gates_passed']:raise SystemExit(1)
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);sub=p.add_subparsers(dest='action',required=True)
 c=sub.add_parser('commands');c.add_argument('--baseline-binary',type=Path,required=True);c.add_argument('--candidate-binary',type=Path,required=True);c.add_argument('--model-dir',type=Path,required=True);c.add_argument('--output-dir',type=Path,required=True)
 c=sub.add_parser('evaluate');c.add_argument('--report-dir',type=Path,required=True);c.add_argument('--output',type=Path,required=True)
 a=p.parse_args();commands(a) if a.action=='commands' else evaluate(a)
