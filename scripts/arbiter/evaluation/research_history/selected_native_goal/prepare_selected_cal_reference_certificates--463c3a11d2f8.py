# SPDX-License-Identifier: Apache-2.0
"""Opaque CAL reference-gain certificates from exact native counterfactual reads.
No human semantic labels, native DTO construction, neural scoring or fitting.
"""
import argparse
import hashlib
import json
from pathlib import Path
from goal_sources import FrozenSourceMetadata,SourceLedger
from goal_rubric import next_read_utility

def sha(p):
    with Path(p).open('rb')as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def need(value,reason):
    if not value:raise ValueError(reason)
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('capture','corpus','rubric','source-manifest','output'):parser.add_argument('--'+name,type=Path,required=True)
    for name in ('capture-sha256','corpus-sha256','rubric-sha256','source-manifest-sha256','task-policy'):parser.add_argument('--'+name,required=True)
    args=parser.parse_args()
    paths=[args.capture,args.corpus,args.rubric,args.source_manifest]
    expected=[args.capture_sha256,args.corpus_sha256,args.rubric_sha256,args.source_manifest_sha256]
    need(all(sha(p)==h for p,h in zip(paths,expected)),'frozen selected CAL source drift')
    capture=json.loads(args.capture.read_bytes());corpus=json.loads(args.corpus.read_bytes());rubric=json.loads(args.rubric.read_bytes())
    need(capture['schema']=='airk.private-selected-Context-FocusON-CAL-native-capture/1' and capture['Focus_configuration']=='ON' and capture['Context_task_NN_invocations']==0 and capture['complete']is True
         and capture['corpus_sha256']==args.corpus_sha256
         and corpus['experiment_partition']=='CAL' and rubric['experiment_partition']=='CAL','CAL-only capture/source admission')
    tasks={row['id']:row for row in rubric['tasks']};originals={row['id']:row for row in corpus['tasks']}
    need(len(tasks)==len(originals)==100 and set(tasks)==set(originals),'complete independent CAL100 required')
    fixture=args.source_manifest.parent;root=fixture/'sources/hotpot'
    metadata=FrozenSourceMetadata(fixture,args.source_manifest_sha256)
    def ledger():return SourceLedger(fixture,root,args.source_manifest_sha256,metadata_cache=metadata)
    rows=[];seen=set();positives=0;unknown=0
    for row in capture['rows']:
        identifier=row['id'];need(identifier in tasks and identifier not in seen,'foreign/duplicate CAL row');seen.add(identifier)
        need(row['split']=='calibration','CAL split drift')
        family=tasks[identifier]['source_component']
        request=row['request'];certified=[];progress=[]
        if request is not None:
            need(request['question']==originals[identifier]['question'] and request['roster']['policy']==args.task_policy,'native question/task-policy drift')
            for outcome in row['counterfactual_choices']:
                before=ledger()
                for record in request['records']:before.observe(record['canonical_result'])
                before_spans=before.visible_candidates()
                after=ledger()
                for record in request['records']:after.observe(record['canonical_result'])
                after.observe(outcome['canonical_result'])
                utility=next_read_utility(tasks[identifier],before_spans,after.visible_candidates())
                progress.append({'choice':outcome['choice']['id'],'original_human_reference_utility':utility,
                                 'classification':'REFERENCE_GAIN'if utility['useful']else 'UNKNOWN',
                                 'human_semantic_gold':False,'semantic_negative':False})
                # IDs/action association come from the ORIGINAL native emitted roster.
                # Native fitter later checks the entire GenericRead card using Core JointRule.supports.
                for card in request['roster']['cards']:
                    if card['action'].get('kind')=='read' and card['action'].get('choice')==outcome['choice']['id']:
                        if utility['useful']:
                            certified.append({'card':card['id'],'proxy':'generic_read_original_reference_complement','choice':outcome['choice']['id'],'necessity_or_optimality_claim':False});positives+=1
                        else:unknown+=1
        rows.append({'id':identifier,'question':originals[identifier]['question'],'split':'calibration','source_group':family,
                     'request':request,'native_capture_status':row['native_capture_status'],
                     'compiler_census':row['admission']['compiler_census'] if row['admission']else [],
                     'remaining_ms_at_observation':min(request['remaining_ms'],row['admission']['remaining_ns_at_observation']//1000000) if request else None,
                     'reference_proxies':{'valid_reference_proxy_whole_cards':certified,'counterfactual_reference_progress':progress,'counterfactual_unresolved':row.get('counterfactual_refusals',[]),'undelivered_observations_are_not_scored':row.get('undelivered_native_observations',[])},
                     'human_semantic_gold':False,'semantic_negative':False})
    need(seen==set(tasks),'CAL capture silently omitted questions')
    need(all(sha(p)==h for p,h in zip(paths,expected)),'CAL source changed during certification')
    report={'schema':'airk.private-selected-Context-CAL-reference-preflight/1','complete':True,'qualified_profiles':[],
            'Focus_configuration':'ON','Focus_profile':capture['Focus_profile'],'MiniLM_pipeline':capture['MiniLM_pipeline'],
            'corpus_sha256':args.corpus_sha256,'capture_sha256':args.capture_sha256,'source_manifest_sha256':args.source_manifest_sha256,
            'rubric_sha256':args.rubric_sha256,'task_policy':args.task_policy,'human_semantic_gold_count':0,
            'scope':'Actual additional canonical Read original-reference complement only; UNKNOWN is notnegative. No Sufficient/Clarify/noRead/gap/role labels or product qualification.',
            'rows':rows}
    need(not args.output.exists() and args.output.resolve().is_relative_to(Path(__file__).resolve().parents[5]/'artifacts') and not args.output.resolve().is_relative_to(fixture.resolve()),'fresh private output outside indexed sources required')
    with args.output.open('x',encoding='utf-8')as stream:json.dump(report,stream,ensure_ascii=False,separators=(',',':'));stream.write('\n')
    print(json.dumps({'complete':True,'questions':100,'positive_reference_gain_cards':positives,'unknown_no_reference_gain_read_cards':unknown,'human_semantic_gold_count':0,'sha256':sha(args.output)}))
if __name__=='__main__':main()
