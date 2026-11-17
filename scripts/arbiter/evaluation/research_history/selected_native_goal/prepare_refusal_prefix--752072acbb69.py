# SPDX-License-Identifier: Apache-2.0
"""Propose one consumed genuine refusal row from immutable failed receipts; no calls."""
import argparse,hashlib,json,sys,os
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE.parent/'source'))
from native_goal_client import require_normal_session_close
from selected_cal_refusals import refusal_row

def sha(path):
    with Path(path).open('rb')as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('failed-capture','call-receipt','owned-execution','arguments','scopes'):
        p.add_argument('--'+name,type=Path,required=True);p.add_argument('--'+name+'-sha256',required=True)
    p.add_argument('--orphan-admission',type=Path,required=True);p.add_argument('--orphan-admission-sha256',required=True);p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();pins={};data={}
    for name in ('failed_capture','call_receipt','owned_execution','arguments','scopes','orphan_admission'):
        path=getattr(a,name);expected=getattr(a,name+'_sha256')
        if sha(path)!=expected:raise ValueError('Exact consumed prefix evidence changed: '+name)
        pins[str(path.resolve())]=expected
        if name!='orphan_admission':data[name]=json.loads(path.read_bytes())
    failed=data['failed_capture'];owned=data['owned_execution'];call=data['call_receipt'];arguments=data['arguments'];scopes=data['scopes'];members=owned.get('owned')or {}
    if not(failed['schema']=='airk.private-selected-Context-FocusON-CAL-native-capture/1'and failed['complete']is False and failed['rows']==[]and failed['Context_task_NN_invocations']==0 and owned['schema']=='airk.private-selected-cal-capture-owned-execution/1'and owned['complete']is False and members.get('exit_code')==1 and members.get('timed_out')is False and members.get('log_oversized')is False and members.get('owned_descendants_settled')is True and members.get('owned_job_forced_termination')is False):raise ValueError('Exact retained original failed call/owned phase required')
    require_normal_session_close(failed['native_session_close'])
    packet_pin=owned['packet']
    if sha(packet_pin['path'])!=packet_pin['sha256']:raise ValueError('Original owner packet changed')
    pins[str(Path(packet_pin['path']).resolve())]=packet_pin['sha256'];packet=json.loads(Path(packet_pin['path']).read_bytes());release_pin=packet['release']
    if sha(release_pin['path'])!=release_pin['sha256']or failed['release_sha256']!=release_pin['sha256']:raise ValueError('Original failed capture release differs')
    pins[str(Path(release_pin['path']).resolve())]=release_pin['sha256'];release=json.loads(Path(release_pin['path']).read_bytes())
    if release['input_pins'].get(str(a.scopes.resolve()))!=a.scopes_sha256:raise ValueError('Original frozen question ordering differs')
    questions=scopes['questions']
    if len(questions)!=100 or len({q['id']for q in questions})!=100 or arguments['question']!=questions[0]['question']or call['arguments_sha256']!=a.arguments_sha256:raise ValueError('Actual attempted first original question differs')
    # No source evidence is claimed for this row. Reconcile retained immutable inputs now; never invent an old postcheck.
    for path,digest in release['input_pins'].items():
        if sha(path)!=digest:raise ValueError('Original inputs no longer match failed-phase pins')
        pins[path]=digest
    for path,sentinel in release.get('absent_build_inputs',{}).items():
        if sentinel!='missing'or os.path.lexists(path):raise ValueError('Consumed deletion is not absent')
    row=refusal_row(questions[0],call,[{'path':str(a.orphan_admission.resolve()),'sha256':a.orphan_admission_sha256,'association':'UNDELIVERED_NOT_A_REQUEST_OR_LABEL'}])
    out=a.output.resolve()
    if out.exists()or not out.is_relative_to(HERE.parents[4]/'artifacts'):raise ValueError('Fresh private prefix output required')
    value={'schema':'airk.private-selected-cal-retained-refusal-prefix/1','approved':None,'scope_sha256':a.scopes_sha256,'rows':[row],'next_original_question_ordinal':1,'original_Focus_profile':failed['Focus_profile'],'original_MiniLM_pipeline':failed['MiniLM_pipeline'],'original_task_policy':failed['task_policy'],'original_failed_phase':{'owned_execution':{'path':str(a.owned_execution.resolve()),'sha256':a.owned_execution_sha256},'release':release_pin,'complete':False},'input_pins':pins,'absent_build_inputs':release.get('absent_build_inputs',{}),'prior_failure_is_success':False,'first_question_replay_allowed':False,'neural_execution':False,'human_semantic_gold_count':0,'source_verification':'None performed for typed canonical refusal. Current immutable-input reconciliation is NOT manufactured historical postcheck or evidence delivery.'}
    out.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8');print(json.dumps({'output':str(out),'sha256':sha(out),'retained_refusals':1,'new_calls':0,'approved':None}))
if __name__=='__main__':main()
