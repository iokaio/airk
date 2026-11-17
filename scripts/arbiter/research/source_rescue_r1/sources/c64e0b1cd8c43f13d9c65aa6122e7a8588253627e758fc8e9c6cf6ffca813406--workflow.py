# SPDX-License-Identifier: Apache-2.0
"""Actual owned SCORE/probe continuity, pure checks plus bounded retained reads."""
import json,os
from pathlib import Path
from worker_logic import digest,stable_json

def continuity_key(plan):
    limits=plan['limits'];environment={k:v for k,v in plan['environment'].items()if k not in('TORCH_HOME','CUDA_CACHE_PATH')}
    record={key:plan[key]for key in('schema','human_sid','repository_root','native_census','native_owned_receipt','cohort','replay','versions','base_python','reference_module','reference_worker','ownership_module','human_identity_helper','worker','optimizer','immutable_input_seals')}
    record['environment']=environment
    record['gpu']={k:plan['gpu'][k]for k in('uuid','name','driver_version','compute_capability')}
    record['limits']={k:limits[k]for k in('host_committed_bytes','cuda_allocator_bytes','minimum_cuda_free_bytes')}
    record['general_replay_instruction']=plan['general_replay_instruction']
    return digest(stable_json(record))

def validate_owned(execution,start,plan,worker,plan_sha,mode):
    if execution['schema']!='airk.private.F1-F2-owned-execution/1'or plan['schema']!='airk.private.F1-F2-owned-plan/1'or plan['mode']!=mode or worker['mode']!=mode:raise ValueError('wrong actual owned workflow stage')
    if start['plan']['sha256']!=plan_sha or execution['plan_sha256']!=plan_sha or worker['plan_sha256']!=plan_sha:raise ValueError('actual stage plan binding differs')
    key=continuity_key(plan)
    if plan['continuity_key']!=key:raise ValueError('actual immutable execution identity differs')
    if execution['identity']['sid']!=plan['human_sid']or start['identity']['sid']!=plan['human_sid']:raise ValueError('actual human stage identity differs')
    expected=[plan['base_python']['path'],'-B',plan['worker']['path'],'--plan',start['plan']['path'],'--plan-sha256',plan_sha]
    if start['argv']!=expected or execution['argv']!=expected:raise ValueError('actual command differs from stage plan')
    owned=execution['owned'];state=execution['disposition']
    if execution['failure']is not None or not state['passed']or not state['naturally_settled']or state['ownership_unknown_or_unsettled']or not worker['complete']or worker.get('failure'):raise ValueError('actual stage did not complete naturally')
    if owned['exit_code']!=0 or owned['timed_out']or owned['log_oversized']or not owned['owned_descendants_settled']or owned['owned_job_forced_termination']or owned['owned_job_final_active_processes']!=0 or owned['owned_job_final_member_pids']or owned['owned_total_processes']!=1:raise ValueError('actual original process tree not successful')
    if not owned['child_assigned_before_resume']or owned['parent_assigned_to_job']or not owned['whole_process_deadline_not_refreshed']:raise ValueError('actual suspended Job/original clock absent')
    exits=owned['retained_member_exits']
    if len(exits)!=1 or not exits[0]['exit_observed']or exits[0]['exit_code']!=0:raise ValueError('original worker HANDLE exit not observed')
    pid=owned['owned_leader_pid'];suspended=owned['suspended_assignment'];gate=owned['before_GO']
    if suspended['single_suspended_worker']!=pid or suspended['parent_in_child_job']or gate['same_actual_leader_gate']!=pid or gate['members']!=[pid]:raise ValueError('actual same worker suspension/GO evidence differs')
    for image in(suspended['image'],gate['image']):
        if os.path.normcase(image['path'])!=os.path.normcase(plan['base_python']['path'])or image['sha256']!=plan['base_python']['sha256']:raise ValueError('actual base interpreter image differs')
    if worker['native_census_sha256']!=plan['native_census']['sha256']:raise ValueError('actual stage used another native census')
    limits=plan['limits']
    if execution['elapsed_seconds']>limits['wall_seconds']or owned['peak_host_committed_bytes']>limits['host_committed_bytes']or worker['peak_cuda_allocated_bytes']>limits['cuda_allocator_bytes']or worker['peak_cuda_reserved_bytes']>limits['cuda_allocator_bytes']:raise ValueError('actual stage exceeded original caps')
    if worker['forward_attempts']!=worker['forward_calls']or worker['optimizer_attempts']!=worker['gradient_steps']or worker['backward_attempts']!=worker['backwards_completed']:raise ValueError('actual unknown submitted work remains')
    if mode=='score':
        if worker['gradient_steps']!=0 or worker['backwards_completed']!=0:raise ValueError('SCORE unexpectedly trained')
    elif mode=='probe':
        if worker['gradient_steps']!=1 or worker['forward_calls']!=32 or worker['backwards_completed']!=32 or not worker['probe_adapter_discarded']or plan['max_optimizer_steps']!=1 or limits['wall_seconds']>120:raise ValueError('not actual bounded one-update probe')
    return key

def load_owned(receipt_pin,mode):
    # The caller's independently reviewed pin roots these local retained bytes.
    # No success JSON manufactures native runtime/query authority.
    from focus_worker import pinned,digest as file_digest
    def read(path,max_bytes):
        path=Path(path)
        if path.stat().st_size>max_bytes:raise ValueError('bounded retained workflow JSON required')
        with path.open('rb')as stream:raw=stream.read(max_bytes+1)
        if len(raw)>max_bytes:raise ValueError('retained workflow JSON grew beyond bound')
        return json.loads(raw)
    if Path(receipt_pin['path']).stat().st_size>4*1024**2:raise ValueError('bounded actual receipt required')
    receipt_path=pinned(receipt_pin);execution=read(receipt_path,4*1024**2);start=read(receipt_path.parent/'start.json',4*1024**2);plan_pin=start['plan']
    if Path(plan_pin['path']).stat().st_size>16*1024**2:raise ValueError('bounded actual execution plan required')
    plan=read(pinned(plan_pin),16*1024**2)
    worker_path=Path(execution['worker_result_path'])
    if worker_path.resolve()!=Path(plan['worker_output']).resolve()/'result.json'or file_digest(worker_path)!=execution['worker_result_sha256']:raise ValueError('actual retained worker result differs')
    worker=read(worker_path,4*1024**2);key=validate_owned(execution,start,plan,worker,plan_pin['sha256'],mode)
    return{'execution':execution,'start':start,'plan':plan,'worker':worker,'continuity_key':key,'receipt':receipt_pin}
