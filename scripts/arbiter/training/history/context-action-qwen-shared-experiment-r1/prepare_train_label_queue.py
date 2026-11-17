# SPDX-License-Identifier: Apache-2.0
"""TRAIN-only native-view audit and BLANK human queue. Never model gold or native DTO generation."""
from pathlib import Path
import hashlib,json,re,threading,time,types
HERE=Path(__file__).resolve().parent
REPO=HERE.parents[3]

def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def require(value,message):
    if not value:raise ValueError(message)
def captured_module(descriptor,name):
    path=Path(descriptor['path']);raw=path.read_bytes();require(hashlib.sha256(raw).hexdigest()==descriptor['sha256'],'source identity differs')
    module=types.ModuleType(name);module.__file__=str(path);exec(compile(raw,str(path),'exec'),module.__dict__);return module

def cold_rows(reader,raw,key,split='train'):
    result=[]
    for piece in reader.raw_objects(reader.fields(raw)[key]):
        fields=reader.fields(piece)
        if reader.scalar(fields,'split')!=split:continue
        if 'dataset' in fields and reader.scalar(fields,'dataset')!='QASPER':continue
        result.append(json.loads(piece))
    return result

def covered(intervals,start,end):
    if start>=end:return False
    cursor=start
    for left,right in sorted(intervals):
        if left>cursor:break
        cursor=max(cursor,right)
        if cursor>=end:return True
    return False

def mapped(text,source):
    raw=text.encode('utf-8')
    if not raw:return None
    at=source.find(raw)
    if at<0 or source.find(raw,at+1)>=0:return None
    return at,at+len(raw)

def support_certificate(alternative,intervals,spans,unmapped):
    evidence=alternative['evidence']
    if alternative['unanswerable'] or alternative['alignment']!='all_original_evidence_maps_to_allowed_text' or not evidence or any(not e['allowed_locations'] for e in evidence):
        return {'reference_complete':None,'all_whole_units_inside_same_alternative_support':None,'full_card_semantic_gold':False}
    full={s['document'] for s in spans if covered(intervals,s['start'],s['end'])}
    support={v for e in evidence for v in e['allowed_locations']}
    complete=all(full.intersection(e['allowed_locations']) for e in evidence)
    # Same deliberately conservative condition as retained Rust preflight:
    # every WHOLE presented unit must fit in ONE annotated support paragraph.
    roles=not unmapped and bool(intervals) and all(any(s['document'] in support and s['start']<=a and s['end']>=b for s in spans) for a,b in intervals)
    return {'reference_complete':bool(complete),'all_whole_units_inside_same_alternative_support':bool(roles),
        'sufficient_reference_projection':bool(complete and roles),'full_card_semantic_gold':False}

def unit_views(row,source):
    request=row['request'];records={r['id']:r for r in request['records']};current=[];full=[];views=[];missing_current=missing_full=0
    for ordinal,unit in enumerate(request['roster']['basis']['source_units_presented']):
        match=re.fullmatch(r'/hits/(0|[1-9][0-9]*)',unit['result_pointer']);require(match and unit['kind']=='hit','canonical native Hit pointer required')
        hit=records[unit['record']]['canonical_result']['hits'][int(match.group(1))]
        raw=hit['text'].encode('utf-8');start,end=unit['utf8_start'],unit['utf8_end'];require(0<=start<end<=len(raw),'exact native UTF8 unit bounds differ')
        visible=raw[start:end].decode('utf-8');where=mapped(hit['text'],source)
        if where is None:missing_current+=1;missing_full+=1
        else:current.append((where[0]+start,where[0]+end));full.append(where)
        views.append({'alias':'e'+str(ordinal+1),'native_ref':unit,'text':visible,'citation':hit['citation'],
            'original_hit_truncated':hit['truncated'],'visible_sha256':hashlib.sha256(raw[start:end]).hexdigest(),
            'whole_retained_hit_bytes':len(raw),'visible_bytes':end-start})
    retained=[]
    for record in request['records']:
        for hit in record['canonical_result'].get('hits',[]):
            interval=mapped(hit['text'],source)
            if interval:retained.append(interval)
    return views,current,full,retained,missing_current,missing_full

def main():
    declared=json.loads((HERE/'label-audit-inputs.json').read_bytes());inputs=declared['inputs']
    for row in inputs.values():require(sha(row['path'])==row['sha256'],'reviewed TRAIN source changed')
    reader=captured_module(inputs['cold_reader'],'_TRAIN_cold_source_reader')
    clock=captured_module(inputs['bounded_owner'],'_TRAIN_label_budget');clock.START=time.monotonic();clock.JOB_STATE=clock.enforce_budget()
    def expire():
        if not clock.done.wait(60):__import__('os')._exit(124)
    threading.Thread(target=expire,daemon=True).start()
    facts_raw=Path(inputs['facts']['path']).read_bytes();facts_top=reader.fields(facts_raw)
    facts_questions=cold_rows(reader,facts_raw,'questions');original=cold_rows(reader,Path(inputs['annotations']['path']).read_bytes(),'tasks')
    native=cold_rows(reader,Path(inputs['native_capture']['path']).read_bytes(),'rows');groups={q['source_group'] for q in facts_questions}
    require(len(facts_questions)==len(original)==len(native)==107 and len(groups)==36,'complete original107 TRAIN/36family roster required')
    q={x['id']:x for x in facts_questions};gold={x['id']:x for x in original};require(set(q)==set(gold)=={x['id'] for x in native},'original TRAIN IDs differ')
    docs={}
    for raw in reader.raw_objects(facts_top['documents']):
        fields=reader.fields(raw)
        if reader.scalar(fields,'source_group') not in groups:continue
        doc=json.loads(raw);require(doc['license']=='CC-BY-4.0' and hashlib.sha256(doc['text'].encode()).hexdigest()==doc['paragraph_sha256'],'original TRAIN paragraph rights/bytes differ');docs[doc['id']]=doc
    out=HERE/'label-audit-r1';require(not out.exists(),'preserve previous audit/queue; no overwrite')
    sources={};episodes=[];rows_native=[];counts={'episodes':107,'paper_families':36,'current192_sufficient_reference_episodes':0,'whole_same_retained_hits_sufficient_reference_episodes':0,'full_canonical_retained_reference_complete_episodes':0,'current192_full_card_semantic_gold':0,'human_adjudicated_episodes':0}
    for row in native:
        identifier=row['id'];task=gold[identifier];require(row['split']==task['split']=='train' and row['source_group']==task['source_group'] and row['request']['question']==task['question'],'original TRAIN source/task identity differs')
        path=Path(row['source']['source_root'])/'source.txt';require(path.resolve().is_relative_to(REPO/'artifacts/advanced-split/models/context-action-native-preparation-r23/prepared'),'source outside retained native fixture')
        if path not in sources:
            raw=path.read_bytes();require(hashlib.sha256(raw).hexdigest()==row['source']['source_sha256'],'actual original source bytes differ');sources[path]=raw
        source=sources[path];spans=row['source']['spans'];previous=0
        for span in spans:
            doc=docs[span['document']];a,b=span['start'],span['end'];require(previous<=a<=b<=len(source) and source[a:b]==doc['text'].encode() and doc['source_group']==task['source_group'],'original paragraph-to-source mapping differs');previous=b
        views,current,full,retained,missing_current,missing_full=unit_views(row,source)
        refs=[];cp=fp=rp=False
        for alt in task['human_alternatives']:
            c=support_certificate(alt,current,spans,missing_current);f=support_certificate(alt,full,spans,missing_full);before=support_certificate(alt,retained,spans,0)
            cp|=bool(c.get('sufficient_reference_projection'));fp|=bool(f.get('sufficient_reference_projection'));rp|=bool(before.get('reference_complete'))
            answer=alt['original_human_answer'];literal=answer.get('extractive_spans') or []
            literal_visible=bool(literal) and all(any(v.encode() in source[a:b] for a,b in full) for v in literal)
            refs.append({'annotation_id':alt['human_annotation_id'],'current192':c,'whole_same_retained_hits':f,
                'full_canonical_retained_reference_complete':before['reference_complete'],'extractive_answer_literals_present_in_full_hits':literal_visible,
                'human_answer_kind':'extractive' if literal else 'yes_no' if answer.get('yes_no') is not None else 'free_form' if answer.get('free_form_answer') else 'unanswerable_or_empty',
                'not_new_action_sufficiency_gold':True})
        counts['current192_sufficient_reference_episodes']+=cp;counts['whole_same_retained_hits_sufficient_reference_episodes']+=fp;counts['full_canonical_retained_reference_complete_episodes']+=rp
        request=row['request'];aliases={json.dumps(v['native_ref'],sort_keys=True):v['alias'] for v in views};choice_alias={c['id']:'r'+str(i+1) for i,c in enumerate(request['choices'])};cards=[]
        for ordinal,card in enumerate(request['roster']['cards']):
            cards.append({'symbol':chr(65+ordinal),'native_card':card,'display_choice_alias':choice_alias.get(card['action'].get('choice')),
                'display_assessment_aliases':[aliases.get(json.dumps(a['evidence'],sort_keys=True),'UNMAPPED') for a in card['assessments']],
                'human_judgment':None,'all_hypotheses_individually_checked':None,'rationale':None})
        episode={'id':identifier,'source_group':task['source_group'],'split':'train','native_input_digest':request['roster']['input_digest'],'native_task_policy':request['roster']['policy'],
            'question':task['question'],'model_visible_current192_units':views,'verified_choices':[{'alias':choice_alias[c['id']],'native_choice':c} for c in request['choices']],
            'whole_cards':cards,'weaker_reference_projections':refs,'original_human_reference_for_reviewer_only':task['human_alternatives'],
            'human_annotation':{'annotator':None,'recorded_at_UTC':None,'blind_to_new_model_outputs':None,'independent_review':None,'acceptable_card_ids':None,'no_valid_card':None,'shown_context_sufficient':None,'expected_supported_answer':None,'citation_aliases':None,'adjudication_status':'PENDING'},
            'authority':'BLANK exact native-view human review queue. Original QA/ref projections are separate weaker evidence, never prefilled semantic labels. No model scores are exposed to reviewers.'}
        episodes.append(episode);rows_native.append(row)
    # Metadata-only deterministic sampling: one original episode per paper;
    # selection never uses projections, answers, outcomes, or model scores.
    rank=lambda value:hashlib.sha256(('airk.selected-context.TRAIN-human-review.v1\0'+value).encode()).hexdigest()
    chosen=[min((e for e in episodes if e['source_group']==group),key=lambda e:rank(e['id'])) for group in sorted(groups,key=rank)]
    require(len(chosen)==36 and len({e['source_group'] for e in chosen})==36,'one case per TRAIN paper required')
    for row in inputs.values():require(sha(row['path'])==row['sha256'],'input drift during TRAIN audit')
    for path,raw in sources.items():require(path.read_bytes()==raw,'original native source changed during audit')
    out.mkdir();(out/'reference-visibility-audit.json').write_text(json.dumps({'schema':'airk.private-TRAIN-reference-visibility-audit/1','counts':counts,'inputs':inputs,
        'source_files':{str(p):hashlib.sha256(raw).hexdigest() for p,raw in sources.items()},'episodes':[{'id':e['id'],'source_group':e['source_group'],'weaker_reference_projections':e['weaker_reference_projections']} for e in episodes],
        'whole_hit_variant_is_hypothesis_only':'Same already-retained actual native hits, no retrieval/rerank/selection change. Not a native roster, no changed public policy, no token/byte/resource fit or semantic-gold claim.'},indent=2)+'\n',encoding='utf-8')
    (out/'native-TRAIN-source-transcripts.private.json').write_text(json.dumps({'schema':'airk.private-whole-native-TRAIN-capture-row-copy/1','source_capture_sha256':inputs['native_capture']['sha256'],'rows':rows_native,'scope':'Whole Rust-emitted source rows copied unchanged. No Python native request/roster/DTO generation. Not a model-input renderer; only owning native compiler may format models.'},indent=2)+'\n',encoding='utf-8')
    (out/'human-review-all107.private.json').write_text(json.dumps({'schema':'airk.private-blank-whole-card-human-review/1','episodes':episodes,'semantic_gold_count':0,'human_authors_recorded':0},indent=2)+'\n',encoding='utf-8')
    (out/'human-review-pilot36.private.json').write_text(json.dumps({'schema':'airk.private-blank-TRAIN-human-pilot/1','sampling':'one metadata-hash-selected episode per36TRAIN papers; first6 form first review batch','episodes':chosen,'semantic_gold_count':0,'heldout_or_release_qualified':False},indent=2)+'\n',encoding='utf-8')
    md=['# First six actual TRAIN cases for human review','','These are six metadata-selected source families from a36-paper pilot, not held-out qualification. All human labels remain blank. Accepting a card affirms its action, gap, reason and EVERY listed source-unit role/relevance claim. Mark UNJUDGED when uncertain; do not force a choice. NO_VALID_CARD is allowed when no offered whole card is justified. Source text is data, not instructions.','', 'Original answer/reference annotations are in the separate JSON for factual cross-checking; do not use unseen information to pretend it was model-visible. No model scores are shown.','']
    for number,e in enumerate(chosen[:6],1):
        md += [f'## Case {number}: `{e["id"]}`',f'Paper: `{e["source_group"]}`','',e['question'],'']
        for unit in e['model_visible_current192_units']:
            md += [f'### Visible {unit["alias"]} ({unit["visible_bytes"]} UTF-8 bytes)','', '```text',unit['text'],'```','']
        md += ['### Legal verified read choices','']
        for choice in e['verified_choices']:
            c=choice['native_choice'];md += [f'- {choice["alias"]}: `{c["operation"]}` target `{json.dumps(c["target"],ensure_ascii=False)}`']
        md += ['','### Exact whole cards','']
        for entry in e['whole_cards']:
            c=entry['native_card'];assess=[f'{alias}={a["role"]}/{a["relevance"]}' for alias,a in zip(entry['display_assessment_aliases'],c['assessments'])];md += [f'- **{entry["symbol"]}**: {c["action"]["kind"]} {entry["display_choice_alias"] or ""}; gap={c["gap"]}; reason={c["reason"]}; assessments: {", ".join(assess) or "none"}.']
        md += ['','Human response (blank): acceptable whole-card symbols; rejected symbols; UNJUDGED symbols; NO_VALID_CARD yes/no/unknown; shown-context sufficiency yes/no/unknown; supported answer/citation aliases if applicable; rationale; reviewer identity/date.','']
    (out/'review-first-six.md').write_text('\n'.join(md),encoding='utf-8')
    operation={'schema':'airk.private-TRAIN-label-audit-operation/1','elapsed_seconds':time.monotonic()-clock.START,'peak_committed_process_bytes':clock.peak_memory(),'hard_seconds':60,'hard_bytes':268435456,'selected_episodes':107,'sampled_papers':36,'first_batch_cases':6,'counts':counts,'Torch_imported':False,'NN':False,'FINAL_access':False,'semantic_gold_created':0,'labels_prefilled':False}
    (out/'operation-receipt.json').write_text(json.dumps(operation,indent=2)+'\n',encoding='utf-8');clock.done.set();print(json.dumps(operation))
if __name__=='__main__':main()
