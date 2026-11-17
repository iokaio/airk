# SPDX-License-Identifier: Apache-2.0
"""Actual FocusON service CAL capture; no answering agent or Context task forward.
Original native templates/probe arguments are copied unchanged. This program is
an experiment driver, not a mirror of any Core request/response schema.
"""
import argparse
from datetime import datetime,timezone
import hashlib
import json
from pathlib import Path
import time
from audit_sources import audit_pass
from native_goal_client import NativeGoalClient,require_normal_session_close
from aggregate_native_retrieval import scope_key
from selected_cal_refusals import continuable_budget_refusal,refusal_row

def sha(path):
    with Path(path).open('rb')as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def need(value,message):
    if not value:raise ValueError(message)
def checked_reply(reply):
    need(reply.get('validated')is True and not reply.get('identity_error') and not reply.get('parse_error')
         and not reply['timed_out'] and reply['returncode']==0 and not reply['envelope'].get('isError'),'actual canonical call failed')
    return reply['full_canonical_value']
def associate(record,reply,question,release):
    need(record['schema']=='airk.private-selected-Context-native-admission-observation/1'
         and record['Context_NN_dispatch']is False and record['Focus_configuration']=='ON'
         and record['actual_Focus_profile']==release['Focus_profile'] and record['MiniLM_pipeline']==release['MiniLM_pipeline']
         and record['actual_Context_profile']['manifest']==release['Context_bootstrap_profile'],'foreign native admission observation')
    request=record['request']
    need(request['question']==question['question'] and request['scope']==reply['assessment']['scope'],'actual source/question association changed')
    matching=[event for event in reply['assessment_history'] if event['ordinal']==request['ordinal']]
    need(len(matching)==1 and matching[0]['result']['advice']['input_digest']==record['planned_input_digest']
         and matching[0]['basis']==request['roster']['basis'],'actual canonical admission digest/basis differs')
    advice=matching[0]['result']['advice']
    need(advice['observed_input_tokens']==0 and advice['work']['input_tokens']==0 and advice['outcome']['status']=='unavailable','capture accidentally dispatched/selected Context')
    return request

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('runtime-pair','control-root','fixture-ready','index-bindings','request-template','question-scopes','fixture','admission-directory','output','release'):
        parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--release-sha256',required=True)
    args=parser.parse_args();started=time.monotonic()
    need(sha(args.release)==args.release_sha256,'reviewed capture release changed')
    release=json.loads(args.release.read_bytes())
    need(release.get('schema')=='airk.private-selected-Context-FocusON-CAL-capture-release/1'
         and release.get('approved')is True and release.get('Context_task_forward')is False
         and release.get('maximum_questions')==100 and release.get('maximum_probes')==500,'actual Root capture release required')
    for name in ('runtime_pair','control_root','fixture_ready','index_bindings','request_template','question_scopes','fixture','admission_directory'):
        need(str(getattr(args,name).resolve())==release['paths'][name],'foreign frozen capture path')
    for path,digest in release['input_pins'].items():need(sha(path)==digest,'capture source/artifact input changed')
    need(not args.output.exists() and args.output.resolve().is_relative_to(Path(__file__).resolve().parents[5]/'artifacts'),'fresh private output required')
    scopes=json.loads(args.question_scopes.read_bytes());questions=scopes['questions']
    need(len(questions)==100 and len({q['id']for q in questions})==100,'complete CAL100 native scopes required')
    bindings=json.loads(args.index_bindings.read_bytes())
    roots={scope_key(row['source_root']):row['index_id']for row in bindings['bindings']}
    need(len(roots)==1 and all(scope_key(q['source_root'])in roots for q in questions),'global whole CAL collection required')
    manifest=json.loads((args.fixture/'manifest.private.json').read_bytes())
    need(manifest['experiment_partition']=='CAL' and sha(args.fixture/'manifest.private.json')==release['source_manifest_sha256'],'foreign source corpus')
    template=json.loads(args.request_template.read_bytes())
    args.output.mkdir();before=audit_pass(args.fixture,manifest)
    client=NativeGoalClient(args.runtime_pair,args.control_root,next(iter(roots.values())),args.output/'canonical-calls',args.fixture_ready,index_bindings=args.index_bindings)
    stage_before=client.observe_stage_status(args.output/'stage-status','before',release['Context_bootstrap_profile'],release['Focus_profile'])
    status=stage_before['full_native_response']
    need(status['focus']['availability']==status['context']['availability']=='ready','actual warmed ready stages required; never spoof readiness')
    record={'schema':'airk.private-selected-Context-FocusON-CAL-native-capture/1','complete':False,
        'task_policy':release['task_policy'],'Focus_configuration':'ON','Focus_profile':release['Focus_profile'],'MiniLM_pipeline':release['MiniLM_pipeline'],
        'source_manifest_sha256':release['source_manifest_sha256'],'corpus_sha256':release['corpus_sha256'],
        'release_sha256':args.release_sha256,'Context_task_NN_invocations':0,'human_semantic_gold_count':0,
        'stage_before':stage_before,'rows':[],'counterfactual_scope':'Independent source-reference forks using only actual Rust-emitted verified reads. Their preparation overhead is retained separately, not a learned trajectory or a fresh allowance inside the captured user request.'}
    output=args.output/'capture.private.json'
    def save():output.write_text(json.dumps(record,ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
    save()
    preparation_succeeded=False
    try:
        for question in questions:
            previous={p.name for p in args.admission_directory.glob('record-*.json')}
            arguments=dict(template);arguments['question']=question['question']
            answer_call=client.call('answer',arguments,30)
            new=[p for p in args.admission_directory.glob('record-*.json')if p.name not in previous]
            need(len(new)<=1,'unexpected extra native assessment')
            if continuable_budget_refusal(answer_call):
                orphans=[{'path':str(p),'sha256':sha(p),'association':'UNDELIVERED_NOT_A_REQUEST_OR_LABEL'}for p in new]
                record['rows'].append(refusal_row(question,answer_call,orphans));save()
                print(json.dumps({'completed_questions':len(record['rows']),'planned_questions':100,'canonical_refusals_retained':True}),flush=True)
                continue
            item={'id':question['id'],'split':'calibration','question':question['question'],'answer_call':answer_call,
                  'request':None,'native_capture_status':'pending_delivery_validation','counterfactual_choices':[],'counterfactual_refusals':[],'admission':None}
            record['rows'].append(item);save()
            answer=checked_reply(answer_call)
            need(answer['schema']=='airk.arbiter.context/2','actual native Context2 required')
            item['native_capture_status']='no_native_assessment_request'
            if new:
                raw=new[0].read_bytes();admission=json.loads(raw)
                request=associate(admission,answer,question,release)
                item.update(request=request,native_capture_status='native_request_captured',admission=admission,
                            admission_file=str(new[0]),admission_sha256=hashlib.sha256(raw).hexdigest())
                need(len(admission['verified_probe_requests'])<=5,'original total probe request count exceeded')
                for probe in admission['verified_probe_requests']:
                    # Never reconstruct, expand or refill a model-generated operation.
                    reply=client.call(probe['tool'],probe['arguments'],30)
                    if continuable_budget_refusal(reply):
                        item['counterfactual_refusals'].append({'choice':probe['choice'],'tool':probe['tool'],'arguments':probe['arguments'],'actual_call':reply,'classification':'UNKNOWN','semantic_negative':False});save();continue
                    value=checked_reply(reply)
                    need(value['publication']==probe['publication'],'counterfactual actual publication changed')
                    item['counterfactual_choices'].append({'choice':probe['choice'],'arguments':probe['arguments'],
                        'tool':probe['tool'],'canonical_result':value,'actual_call':reply})
                need(len(item['counterfactual_choices'])+len(item['counterfactual_refusals'])<=5,'original total scored/read-refused count exceeded')
            save()
            print(json.dumps({'completed_questions':len(record['rows']),'planned_questions':100}),flush=True)
        record['stage_after']=client.observe_stage_status(args.output/'stage-status','after',release['Context_bootstrap_profile'],release['Focus_profile'])
        need(record['stage_after']['full_native_response']['configuration']==status['configuration'],'actual stage configuration changed')
        need(audit_pass(args.fixture,manifest)==before,'frozen source bytes changed')
        for path,digest in release['input_pins'].items():need(sha(path)==digest,'capture source/artifact drift')
        need(len(record['rows'])==100 and [row['id']for row in record['rows']]==[q['id']for q in questions],'fixed complete ordered CAL100 census required')
        preparation_succeeded=True
    except Exception as error:
        record['failure_class']=type(error).__name__;raise
    finally:
        closed=client.close();record['native_session_close']=closed
        record['elapsed_seconds']=time.monotonic()-started
        try:
            require_normal_session_close(closed)
            record['complete']=preparation_succeeded
        except Exception as error:
            record['complete']=False;record['close_failure_class']=type(error).__name__
            save();raise
        save()
    print(json.dumps({'complete':record['complete'],'questions':len(record['rows']),'sha256':sha(output),'Context_task_NN_invocations':0,'human_semantic_gold_count':0}))
if __name__=='__main__':main()
