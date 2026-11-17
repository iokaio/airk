# SPDX-License-Identifier: Apache-2.0
"""Retain authenticated canonical budget refusals as UNKNOWN; never delivery evidence."""
def continuable_budget_refusal(call):
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
        and isinstance(error,dict)and error.get('code')=='verification_reply_budget'
        and isinstance(error.get('message'),str)and error.get('retryable')is False
        and error.get('verification_performed')is False)

def refusal_row(question,call,orphans):
    if not continuable_budget_refusal(call):raise ValueError('Only authenticated nonretryable unverified verification-budget refusal may continue')
    return {'id':question['id'],'split':'calibration','question':question['question'],'answer_call':call,
        'request':None,'admission':None,'counterfactual_choices':[],'counterfactual_refusals':[],
        'native_capture_status':'verification_budget_refusal_no_delivered_assessment',
        'undelivered_native_observations':orphans,'semantic_negative':False,'human_semantic_gold':False,
        'delivery':'No Context2 delivered; private admission observations cannot establish a scored request.'}
