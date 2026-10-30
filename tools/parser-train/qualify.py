# SPDX-License-Identifier: Apache-2.0
"""Saved, reproducible corpus generation, learning curves and held-out evaluation.

Validation chooses training size; calibration alone chooses thresholds. Repository
and independently authored fixtures are evaluated only after those choices freeze.
"""
import argparse
import hashlib
import json
from pathlib import Path
import random
import re
import struct

import numpy as np
from sklearn.linear_model import LogisticRegression
from sklearn.multiclass import OneVsRestClassifier

from train import LABELS, ROOT, TEMPLATES, matrix
from retention import retain

WORK=ROOT/'target/parser-train'
HERE=Path(__file__).parent
ROLES={
 'developer':['For this code change, ','While debugging, ','For implementation review, ','In this component, '],
 'tester':['For regression review, ','While checking test evidence, ','For a test investigation, ','During verification, '],
 'architect':['For architecture review, ','While examining dependencies, ','For a component design review, ','During design analysis, '],
 'writer':['For documentation review, ','While writing the technical guide, ','For the reference page, ','During terminology review, '],
}
# These additional action families come from the tool contracts and validation
# confusions (contents / path enumeration / include declarations), not test errors.
MORE_TEMPLATES={
 'search':['match passages about {s}','query indexed snippets for {s}','find text discussing {s}','look up the topic {s}','search project evidence concerning {s}','locate documentation related to {s}'],
 'find_definition':['locate the declaration named {s}','show the defining symbol {s}','find the class or function declaration {s}','give the declaration location for {s}','lookup the defined type {s}','show the implementation declaration of {s}'],
 'references':['find identifier mentions of {s}','collect textual usages of {s}','locate occurrences mentioning {s}','show files with textual references to {s}','find whole-identifier uses of {s}','enumerate textual occurrences of {s}'],
 'read_file':['display source contents from {p}','fetch file content in {p}','retrieve an indexed excerpt from {p}','give the source lines in {p}','show a code passage from {p}','read file contents from {p}'],
 'outline':['get the heading tree of {p}','enumerate definitions inside {p}','show the file outline of {p}','get the structural declaration list for {p}','list the headings and declarations in {p}','inspect the structure of {p}'],
 'list_files':['enumerate indexed paths beneath {p}','browse the indexed file list in {p}','show captured filenames under {p}','get the path inventory for {p}','list the files present in the indexed folder {p}','display the indexed filename listing for {p}'],
 'file_info':['inspect extraction metadata for {p}','get the saved content hash for {p}','show parser and language details for {p}','report file identity metadata for {p}','inspect indexed byte and chunk counts of {p}','get extraction profile information for {p}'],
 'imports':['locate include statements naming {s}','show static import dependencies of {s}','retrieve written include declarations for {s}','find module imports referring to {s}','list use declarations naming {s}','inspect import and include evidence for {s}'],
 'facts':['retrieve requirement statements about {s}','collect design facts concerning {s}','find documented acceptance rules for {s}','show decision evidence on {s}','retrieve architecture statements mentioning {s}','find documentation configuration facts about {s}'],
 'changes':['validate saved citation identities','report indexed evidence changes since a baseline','check remembered paths against the current publication','inspect what changed between publications','compare a saved evidence baseline','find removed evidence since a retained token'],
 'status':['report basic index readiness','check if the collection is searchable','show current indexed file and chunk counts','inspect current publication readiness','check whether vectors are search-ready','get the saved index health summary'],
 'diagnostics':['report detailed process and worker observations','inspect background job progress and queue waits','show source watcher diagnostics','get native runtime process details','inspect service troubleshooting information','report worker activity and ownership diagnostics'],
 'help':['show the AIRK command reference','explain AIRK tool usage','get the bundled AIRK usage manual','show instructions for AIRK CLI arguments','explain AIRK evidence and citation conventions','find guidance for AIRK MCP configuration'],
}
PATTERNS={intent:TEMPLATES[intent]+MORE_TEMPLATES[intent] for intent in LABELS}

def save_jsonl(kind,rows):
    content=b''.join((json.dumps(row,ensure_ascii=False,sort_keys=True)+'\n').encode('utf-8') for row in rows)
    digest=hashlib.sha256(content).hexdigest();file=WORK/'data'/f'{kind}-{digest[:16]}.jsonl';file.parent.mkdir(parents=True,exist_ok=True)
    if file.exists():
        if hashlib.sha256(file.read_bytes()).hexdigest()!=digest:raise ValueError('Existing dataset differs from its hash; it was preserved')
    else:
        with file.open('xb') as stream:stream.write(content)
    retain(file)
    return {'path':file.relative_to(ROOT).as_posix(),'sha256':digest,'rows':len(rows)}

def vocabulary():
    pins=json.loads((HERE/'repositories.lock.json').read_text(encoding='utf-8'))['repositories'];result=[]
    for pin in pins:
        slug=pin['repository'].replace('/','--')+'-'+pin['revision'][:12]
        candidates=sorted((WORK/'vocabulary').glob(slug+'-sample-*.json'))
        if len(candidates)!=1:raise ValueError(f'One declared bounded vocabulary sample required for {slug}')
        file=candidates[0];data=json.loads(file.read_text(encoding='utf-8'))
        result.append({**pin,'vocabulary_sha256':hashlib.sha256(file.read_bytes()).hexdigest(),'vocabulary_path':file.relative_to(ROOT).as_posix(),'data':data})
    return result

def examples(count,repositories,base_indices,seed,split):
    rng=random.Random(seed);rows=[];seen=set();roles=list(ROLES);attempts=0
    while len(rows)<count:
        attempts+=1
        if attempts>count*100:raise ValueError('Not enough distinct templates/slots for requested corpus size')
        intent=LABELS[len(rows)%13];role=roles[len(rows)%4];repo=rng.choice(repositories);data=repo['data']
        symbols=data['symbols'] or data['headings'] or ['DocumentSection'];symbol=rng.choice(symbols)
        path=rng.choice(data['paths']);base=rng.choice(base_indices);prefix=rng.randrange(4)
        # Identifier-only noise is seeded. It never changes a request's action verb.
        noise='none'
        if len(symbol)>4 and rng.random()<.08:
            at=rng.randrange(len(symbol));symbol=symbol[:at]+symbol[at+1:];noise='identifier-character-drop'
        elif rng.random()<.08:
            symbol=re.sub(r'(?<=[a-z0-9])(?=[A-Z])',' ',symbol);noise='identifier-subtokens'
        template=PATTERNS[intent][base]
        text=ROLES[role][prefix]+template.format(s=symbol,p=path)
        if '{s}' not in template and '{p}' not in template:text+=' for the project containing '+path
        if len(text.encode('utf-8'))>4096 or text in seen:continue
        seen.add(text)
        rows.append({'text':text,'intent':intent,'role':role,'split':split,'template_id':f'{intent}/{role}/base-{base}/prefix-{prefix}','repository':repo['repository'],'revision':repo['revision'],'vocabulary_sha256':repo['vocabulary_sha256'],'noise':noise})
    return rows

def quantize(model):
    weights=np.stack([m.coef_[0] for m in model.estimators_],axis=1);bias=np.array([m.intercept_[0] for m in model.estimators_])
    scale=120/np.max(np.abs(weights));return np.rint(weights*scale).astype(np.int8),np.rint(bias*scale).astype('<i4')

def scores(rows,weights,bias):
    return np.asarray(matrix([r['text'] for r in rows]) @ weights.astype(np.int32))+bias

def metrics(rows,values,threshold=0,margin=80):
    truth=np.array([LABELS.index(r['intent']) for r in rows]);predicted=values.argmax(axis=1);ordered=np.sort(values,axis=1)
    accepted=(ordered[:,-1]>=threshold)&((ordered[:,-1]-ordered[:,-2])>=margin)
    roles={role:float(np.mean(predicted[[r['role']==role for r in rows]]==truth[[r['role']==role for r in rows]])) for role in ROLES if any(r['role']==role for r in rows)}
    return {'rows':len(rows),'accuracy':float(np.mean(predicted==truth)),'per_role':roles,'accepted_precision':float(np.mean(predicted[accepted]==truth[accepted])) if accepted.any() else None,'ask_rate':float(np.mean(~accepted)),'wrong_accepted':int(np.sum(accepted&(predicted!=truth)))}

def thresholds(rows,values):
    truth=np.array([LABELS.index(r['intent']) for r in rows]);predicted=values.argmax(axis=1);ordered=np.sort(values,axis=1);top=ordered[:,-1];gap=top-ordered[:,-2]
    candidates=[]
    for threshold in np.unique(np.quantile(top,np.linspace(0,1,41)).astype(int)):
        for margin in np.unique(np.quantile(gap,np.linspace(0,1,41)).astype(int)):
            accepted=(top>=threshold)&(gap>=margin);coverage=float(np.mean(accepted))
            if accepted.any():
                precision=float(np.mean(predicted[accepted]==truth[accepted]))
                if precision>=.995:candidates.append((coverage,precision,int(threshold),int(margin)))
    # Precision takes priority over the <=10% ask-rate target. If no safe
    # threshold exists, abstain on the entire calibration set; never relax it.
    if not candidates:return int(top.max())+1,0
    _,_,threshold,margin=max(candidates);return threshold,margin

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--examples',type=int,default=40000);parser.add_argument('--promote',action='store_true');parser.add_argument('--development-only',action='store_true');args=parser.parse_args()
    if args.examples!=40000:raise ValueError('This qualification protocol fixes the final corpus at 40,000 examples')
    repos=vocabulary();train_repos=[r for r in repos if r['split']=='train'];held_repos=[r for r in repos if r['split']=='evaluation']
    train=examples(40000,train_repos,[0,1,2,3,6,7,8,9,10,11],84173,'train');validation=examples(4000,train_repos,[4],84174,'validation');calibration=examples(4000,train_repos,[5],84175,'calibration')
    if {r['text'] for r in train}&({r['text'] for r in validation}|{r['text'] for r in calibration}):raise ValueError('Training/evaluation text overlap')
    held=[(r['repository'],examples(2600,[r],list(range(6)),84176+i,'repository-evaluation')) for i,r in enumerate(held_repos)]
    datasets={'train':save_jsonl('train',train),'validation':save_jsonl('validation',validation),'calibration':save_jsonl('calibration',calibration)}
    datasets.update({name:save_jsonl('held-'+name.replace('/','--'),rows) for name,rows in held})
    print('Saved 40,000 training, 4,000 validation, 4,000 calibration and 7,800 held-out repository examples.',flush=True)
    learning=[];selected=None
    for count in [10000,20000,40000]:
        subset=train[:count]
        model=OneVsRestClassifier(LogisticRegression(C=2,max_iter=250,solver='liblinear',random_state=84173),n_jobs=1).fit(matrix([r['text'] for r in subset]),[LABELS.index(r['intent']) for r in subset])
        weights,bias=quantize(model);entry={'training_rows':count,'validation':metrics(validation,scores(validation,weights,bias))};learning.append(entry);print(json.dumps(entry),flush=True)
        # Choose using validation alone; a larger corpus is not automatically better.
        # Strict improvement retains the smaller model when accuracy ties.
        if selected is None or entry['validation']['accuracy']>selected[0]:
            selected=(entry['validation']['accuracy'],count,weights,bias)
    _,selected_count,weights,bias=selected;cal_scores=scores(calibration,weights,bias);threshold,margin=thresholds(calibration,cal_scores)
    evaluations={} if args.development_only else {name:metrics(rows,scores(rows,weights,bias),threshold,margin) for name,rows in held}
    fixture_file=HERE/'evaluation.json';fixture_result=None
    if fixture_file.exists() and not args.development_only:
        fixture=json.loads(fixture_file.read_text(encoding='utf-8'))
        if len(fixture)!=150:raise ValueError('Independent evaluation requires exactly 150 utterances')
        if {r['text'] for r in fixture}&{r['text'] for r in train}:raise ValueError('Independent fixture leaked into training')
        fixture_result=metrics(fixture,scores(fixture,weights,bias),threshold,margin)
    payload=weights.tobytes()+bias.tobytes();asset=b'AIRKINT1'+struct.pack('<IIIIii',1,65536,13,1,threshold,margin)+hashlib.sha256('\n'.join(LABELS).encode()).digest()+hashlib.sha256(payload).digest()+payload
    asset_hash=hashlib.sha256(asset).hexdigest();model_file=WORK/'models'/('intent-'+asset_hash[:16]+'.bin');model_file.parent.mkdir(parents=True,exist_ok=True);model_file.write_bytes(asset)
    retain(model_file)
    safety_file=HERE/'safety.json';safety_result=None;accepted=np.array([True])
    if not args.development_only:
        safety=json.loads(safety_file.read_text(encoding='utf-8'));safety_scores=scores(safety,weights,bias);ordered=np.sort(safety_scores,axis=1);accepted=(ordered[:,-1]>=threshold)&((ordered[:,-1]-ordered[:,-2])>=margin)
        safety_result={'rows':len(safety),'model_accepted_unsupported':int(np.sum(accepted)),'scope':'Raw classifier rejection only; runtime scope/slot/operation validation is tested separately.'}
    calibration_result=metrics(calibration,cal_scores,threshold,margin)
    gates={'repository_accuracy':bool(evaluations) and all(r['accuracy']>=.95 for r in evaluations.values()),'role_accuracy':bool(evaluations) and all(all(v>=.90 for v in r['per_role'].values()) for r in evaluations.values()),'independent_150':bool(fixture_result and fixture_result['accuracy']>=.92),'independent_safety_fixture':not bool(np.any(accepted)),'calibration_accepted_precision':(calibration_result['accepted_precision'] or 0)>=.995,'supported_ask_rate':calibration_result['ask_rate']<=.10}
    report={'schema':1,'protocol':4,'seed':84173,'datasets':datasets,'templates_sha256':hashlib.sha256(json.dumps({'base':PATTERNS,'roles':ROLES},sort_keys=True).encode()).hexdigest(),'evaluation_fixture_sha256':hashlib.sha256(fixture_file.read_bytes()).hexdigest() if fixture_file.exists() else None,'safety_fixture_sha256':hashlib.sha256(safety_file.read_bytes()).hexdigest(),'learning_curve':learning,'selected_training_rows':selected_count,'selection':'Highest validation accuracy; smallest corpus breaks ties. No calibration or test results select the training size.','threshold':threshold,'margin':margin,'calibration':calibration_result,'repository_evaluation':evaluations,'independent_150':fixture_result,'safety':safety_result,'gates':gates,'qualified':all(gates.values()),'model_path':model_file.relative_to(ROOT).as_posix(),'asset_sha256':asset_hash,'asset_bytes':len(asset),'scope':'Deterministic bounded source samples; generated evaluation varies repository vocabulary independently of training. Independent fixture labels require review. The fixtures were previously measured in the retained initial run; this is a regression measurement, not a fresh unseen test.'}
    report_text=json.dumps(report,indent=2)+'\n';report_hash=hashlib.sha256(report_text.encode()).hexdigest()
    archive=WORK/'reports'/('qualification-'+report_hash[:16]+'.json');archive.parent.mkdir(parents=True,exist_ok=True)
    if not archive.exists():archive.write_text(report_text,encoding='utf-8')
    retain(archive)
    destination=WORK/'reports'/('development-'+asset_hash[:16]+'.json') if args.development_only else HERE/'qualification.json';destination.parent.mkdir(parents=True,exist_ok=True);destination.write_text(report_text,encoding='utf-8')
    if args.development_only:retain(destination)
    if args.promote:
        if not report['qualified']:raise ValueError('Qualification gates failed; model was retained locally and runtime asset was preserved')
        (ROOT/'crates/airk-retrieval/assets/intent-v1.bin').write_bytes(asset)
    print(json.dumps(report),flush=True)

if __name__=='__main__':main()
