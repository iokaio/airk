# SPDX-License-Identifier: Apache-2.0
"""Pure F1/F2 selection/schedule logic. Never imports Torch or tokenizes."""
import collections, hashlib, json, math

def digest(raw):return hashlib.sha256(raw).hexdigest()
def stable_json(value):return json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()
def check_input(value):
    if len(value['input_ids'])!=512 or len(value['attention_mask'])!=512 or value['position_ids']!=list(range(512)) or value['candidate_positions']!=[2152,9693] or value['option_mask']!=[True,True] or value['decision_position']!=511:
        raise ValueError('native512 input/readout changed')
    mask=value['attention_mask'];live=sum(mask)
    if not 0<live<=512 or mask!=[0]*(512-live)+[1]*live:raise ValueError('native left padding changed')
    if len(value['input_digest'])!=64:raise ValueError('native input digest absent')
    return value

def native_rows(emission,roster,replay):
    census=emission['census']
    if emission['schema']!='airk.private.focus-instruction-emission/1' or census['schema']!='airk.private.focus-instruction-census/1' or census['qualification'] or census['model_requests_dispatched']:
        raise ValueError('genuine native compiler-only census required')
    original={r['id']:r for r in roster};general={r['id']:r for r in replay}
    if len(roster)!=1955 or len(replay)!=45 or len(original)!=1955 or len(general)!=45 or set(original)&set(general):raise ValueError('frozen1955+45 distinct IDs required')
    if len(census['rows'])!=2000 or census['original_denominator']!=2000:raise ValueError('entire original denominator required')
    rows={}
    for record in census['rows']:
        pair=record['pair'];id=pair['id']
        if id in rows:raise ValueError('duplicate native pair')
        if id in original:
            owner=original[id]
            if pair['label']!=owner['label'] or pair['role']!=owner['role'] or pair['rights_scope']!=owner['rights_review'] or pair['question_sha256']!=owner['question_sha256'] or pair['document_sha256']!=owner['function_sha256']:
                raise ValueError('original role/label/source binding changed')
            component=owner['component'];eligibility=owner['eligibility']
            if eligibility not in('NATIVE_FIT_SUPPORTED_PAIR','UNKNOWN_RIGHTS','NATIVE_FOCUS512_OVERFLOW'):raise ValueError('unknown frozen original eligibility')
        elif id in general:
            owner=general[id]
            if pair['role']!='REPLAY' or pair['label']!=1 or pair['question_sha256']!=owner['question_sha256'] or pair['document_sha256']!=owner['window_sha256'] or pair['rights_scope']!=owner['rights_review']:
                raise ValueError('replay original human evidence binding changed')
            component='qasper:'+owner['family'];eligibility=None
        else:raise ValueError('unknown native row')
        if digest(pair['question'].encode())!=pair['question_sha256'] or digest(pair['document'].encode())!=pair['document_sha256']:raise ValueError('native question/window bytes changed')
        for recipe in('publisher','software_evidence_v1'):
            result=record[recipe]
            if result['kind']=='compiled':check_input(result['input'])
            elif result['kind']!='refused' or not result['reason']:raise ValueError('native refusal absent')
        rows[id]=dict(record,component=component,eligibility=eligibility)
    if set(rows)!=set(original)|set(general):raise ValueError('native row subset')
    return rows

def effective_recipe(row,chosen):
    return 'publisher'if row['pair']['role']=='REPLAY'else chosen

def training_schedule(rows,recipe,seed=1908):
    # Eligibility is the original frozen F0 roster, never post-F1 fit/score.
    eligible=lambda role:[r for r in rows.values()if r['pair']['role']==role and r['pair']['rights_scope']=='PERMITTED_PRIVATE_RESEARCH_SCOPE']
    code=[r for r in eligible('TRAIN')if r['eligibility']=='NATIVE_FIT_SUPPORTED_PAIR'];replay=eligible('REPLAY')
    if len(code)!=400 or len(replay)!=45:raise ValueError('exact400code+45originalreplay required')
    for row in code+replay:
        if row[effective_recipe(row,recipe)]['kind']!='compiled':raise ValueError('frozen train/replay native refusal; no crop/refill or silent drop')
    order=lambda values:sorted(values,key=lambda r:digest((f'airk.F2.epoch1.seed{seed}\n'+r['pair']['id']).encode()))
    code,replay=order(code),order(replay);out=[];r=0
    for i,row in enumerate(code,1):
        out.append(row)
        if i%9==0:out.append(replay[r]);r+=1
    out.extend(replay[r:])
    if len(out)!=445 or len({x['pair']['id']for x in out})!=445:raise ValueError('exact one-epoch schedule changed')
    return out

def batches(rows,size=32):return [rows[i:i+size]for i in range(0,len(rows),size)]
def target(label):
    if type(label)is not int or label not in(0,1):raise ValueError('unknown human relevance')
    return(2152,9693)[label]
def full_vocab_ce(values,label):
    # Independent scalar control for the actual Torch full-head loss.
    index=target(label)
    if len(values)<=index or any(not math.isfinite(x)for x in values):raise ValueError('incomplete full vocabulary')
    peak=max(values);return peak+math.log(sum(math.exp(x-peak)for x in values))-values[index]
def metrics(labels,margins):
    if len(labels)!=len(margins)or not labels or any(not math.isfinite(x)for x in margins):raise ValueError('missing/nonfinite raw logits')
    pos=[s for y,s in zip(labels,margins)if y==1];neg=[s for y,s in zip(labels,margins)if y==0]
    if len(pos)+len(neg)!=len(labels)or not pos or not neg:raise ValueError('both original classes required')
    auc=sum((p>n)+.5*(p==n)for p in pos for n in neg)/(len(pos)*len(neg))
    bal=.5*(sum(x>0 for x in pos)/len(pos)+sum(x<=0 for x in neg)/len(neg))
    return{'auroc':auc,'fixed_zero_balanced_accuracy':bal}
def choose_instruction(report,rows):
    if not report['complete']or report['mode']!='score':raise ValueError('complete original fixedDEV score run required')
    expected={k for k,v in rows.items()if v['pair']['role']=='DEV'}
    if len(report['rows'])!=85:raise ValueError('exact85 score rows required; duplicates cannot collapse')
    observed={r['id']:r for r in report['rows']}
    if len(expected)!=85 or len(observed)!=85 or set(observed)!=expected:raise ValueError('full85DEV denominator required')
    groups=collections.defaultdict(list)
    for id in sorted(expected):
        pair=rows[id]['pair'];r=observed[id]
        if r['label']!=pair['label']:raise ValueError('DEV label changed')
        groups[rows[id]['component']].append((id,pair,r))
    allmetrics={};allfit=True
    for recipe in('publisher','software_evidence_v1'):
        component=[]
        for component_id,group in groups.items():
            labels=[];scores=[]
            for id,pair,result in group:
                observed=result[recipe];native=rows[id][recipe]
                if observed['status']!='scored':
                    if native['kind']!='refused':raise ValueError('missing submitted score is not a compiler refusal')
                    allfit=False;continue
                if native['kind']!='compiled'or observed['input_digest']!=native['input']['input_digest']:raise ValueError('score native input differs')
                no,yes=observed['no_logit'],observed['yes_logit']
                if not math.isfinite(no)or not math.isfinite(yes):raise ValueError('nonfinite score')
                labels.append(pair['label']);scores.append(yes-no)
            if len(labels)!=len(group):allfit=False;continue
            component.append(dict(component=component_id,**metrics(labels,scores)))
        allmetrics[recipe]={'components':component,'macro_auroc':sum(x['auroc']for x in component)/len(component)if component else None}
    selected='publisher';reason='retain publisher on refusal, tie or absence of every preregistered DEV condition'
    if allfit:
        f0=allmetrics['publisher'];f1=allmetrics['software_evidence_v1'];a={r['component']:r for r in f0['components']};b={r['component']:r for r in f1['components']}
        if f1['macro_auroc']>f0['macro_auroc']and all(b[k]['fixed_zero_balanced_accuracy']>=a[k]['fixed_zero_balanced_accuracy']for k in a):selected='software_evidence_v1';reason='strict DEV component-macroAUROC gain and no DEVcomponent fixed-zero balanced-accuracy regression'
    return{'selected':selected,'reason':reason,'metrics':allmetrics,'all85fit_and_scored':allfit,'promotion':False,'scope':'exploratory pointwise diagnostic only; no significance/quality/ranking/terminal promotion'}
