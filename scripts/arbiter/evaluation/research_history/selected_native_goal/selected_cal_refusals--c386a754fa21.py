# SPDX-License-Identifier: Apache-2.0
"""Retain authenticated canonical budget refusals as UNKNOWN; never delivery evidence."""
def canonical_refusal(call):
    value=call.get('full_canonical_value');envelope=call.get('envelope');exchange=call.get('session_exchange')or {}
    error=value.get('error')if isinstance(value,dict)else None
    return bool(call.get('schema')=='airk.development-native-goal-call/1'
        and call.get('identity_error')is None and call.get('parse_error')is None
        and call.get('timed_out')is False and call.get('returncode')==1
        and call.get('validated')is False and call.get('actual_service_identity')
        and call.get('native_binding_sha256')and len(call.get('actual_health_observations',[]))==2
        and exchange.get('kind')=='invoked'and exchange.get('launch_count')==1
        and exchange.get('arguments_sha256')==call.get('arguments_sha256')
        and exchange.get('result',{}).get('exitCode')==1
        and isinstance(envelope,dict)and envelope.get('isError')is True
        and isinstance(error,dict)and isinstance(error.get('code'),str)and error['code']
        and isinstance(error.get('message'),str)and type(error.get('retryable'))is bool)

def canonical_budget_refusal(call):
    # Historical prefix preparation only; active generic handling uses canonical_refusal.
    return bool(canonical_refusal(call)and call['full_canonical_value']['error']['code']=='verification_reply_budget'
        and call['full_canonical_value']['error'].get('verification_performed')is False
        and call['full_canonical_value']['error']['retryable']is False)

def refusal_row(question,call,orphans):
    if not canonical_refusal(call):raise ValueError('Only authenticated canonical refusal may continue')
    return {'id':question['id'],'split':'calibration','question':question['question'],'answer_call':call,
        'request':None,'admission':None,'counterfactual_choices':[],'counterfactual_refusals':[],
        'native_capture_status':'canonical_refusal_no_delivered_assessment',
        'undelivered_native_observations':orphans,'semantic_negative':False,'human_semantic_gold':False,
        'delivery':'No Context2 delivered; private admission observations cannot establish a scored request.'}
