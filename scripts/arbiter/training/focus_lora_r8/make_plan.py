# SPDX-License-Identifier: Apache-2.0
"""Seal a future Root-reviewed F1/F2 execution. Never dispatches a model."""
import argparse,hashlib,json,os,sys
from pathlib import Path
from focus_worker import digest,pinned
import worker_logic as logic
import source_epoch
from workflow import continuity_key,load_owned

def require_private_root(value):
    if not value:
        raise ValueError('explicit private root required')
    root = next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file())
    private = Path(value).resolve()
    if not private.is_relative_to(root/'artifacts'):
        raise ValueError('private root must remain beneath repository artifacts; tracked or external roots refused')
    return private


def pin(p):p=Path(p).resolve();return{'path':str(p),'bytes':p.stat().st_size,'sha256':digest(p)}
def main():
    p=argparse.ArgumentParser();p.add_argument('--source-epoch',type=Path,required=True);p.add_argument('--source-epoch-sha256',required=True);p.add_argument('--native-census',type=Path,required=True);p.add_argument('--native-census-sha256',required=True);p.add_argument('--native-owned-receipt',type=Path,required=True);p.add_argument('--gpu-inventory',type=Path,required=True);p.add_argument('--mode',choices=['score','probe','train'],required=True);p.add_argument('--selection',type=Path);p.add_argument('--measured-probe',type=Path);p.add_argument('--log-dir',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--execution-root',type=Path,required=True);args=p.parse_args()
    O=Path(__file__).resolve().parent;E=require_private_root(args.execution_root);root=next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file());cohort=E/'f2-existing-pairs-closure-r1/all-original1955-roster.json';replay=E/'f2-existing-replay-candidates-r1/qasper-fixed45-draws.json';original=root/'artifacts/advanced-split/models/weights-v2/software-v2-independent-numeric-r1/plans-native-r2/focus-original_fp32.json';reference=json.loads(original.read_bytes())
    identity_path=O/'support/candidate_overlay.py'
    import importlib.util
    spec=importlib.util.spec_from_file_location('custody',identity_path);h=importlib.util.module_from_spec(spec);sys.modules[spec.name]=h;spec.loader.exec_module(h);identity=h.identity()
    if args.output.exists()or args.log_dir.exists()or not args.output.resolve().is_relative_to(E)or not args.log_dir.resolve().is_relative_to(E):raise ValueError('fresh approved plan/output roots required')
    if digest(args.native_census)!=args.native_census_sha256:raise ValueError('exact actual native census required')
    emission=json.loads(args.native_census.read_bytes());rows=logic.native_rows(emission,json.loads(cohort.read_bytes()),json.loads(replay.read_bytes()))
    epoch_pin={'path':str(args.source_epoch.resolve()),'sha256':args.source_epoch_sha256};epoch=json.loads(pinned(epoch_pin).read_bytes())
    held=json.loads(pinned(epoch['held_failed_plan']).read_bytes());original_parent=json.loads(pinned(epoch['original_reference_plan']).read_bytes())
    for name in('native_build_receipt','native_census','native_image','native_owned_receipt','held_failed_execution'):pinned(epoch[name])
    current_build=json.loads(pinned(epoch['native_build_receipt']).read_bytes())
    operational_seals=source_epoch.advance(held,original_parent,epoch,current_build,emission)
    if reference!=original_parent or args.native_census_sha256!=epoch['native_census']['sha256']:raise ValueError('immutable original reference/census parent differs')
    # Root independently reviews the actual native child receipt and image. It
    # remains bound in this future plan, never inferred from a success boolean.
    native_receipt=pin(args.native_owned_receipt);gpu_document=json.loads(args.gpu_inventory.read_bytes());gpu=gpu_document.get('baseline_gpu',gpu_document.get('gpu',gpu_document))
    for key in('uuid','name','driver_version','compute_capability'):assert key in gpu
    selected='publisher'
    if args.mode!='score':
        if args.selection is None:raise ValueError('actual fixedDEV selection required')
        selection=json.loads(args.selection.read_bytes());scores=json.loads(pinned(selection['score_report']).read_bytes());choice=logic.choose_instruction(scores,rows)
        if scores['native_census_sha256']!=args.native_census_sha256 or selection['choice']!=choice or selection['native_census_sha256']!=args.native_census_sha256:raise ValueError('actual selection/census differs')
        selected=choice['selected'];logic.training_schedule(rows,selected)
    measured=None
    if args.mode=='train':
        if args.measured_probe is None:raise ValueError('naturally successful measured probe required')
        probe=json.loads(args.measured_probe.read_bytes())
        if probe['schema']!='airk.private.F1-F2-owned-execution/1'or not probe['disposition']['passed']or not probe['owned']['owned_descendants_settled']or probe['owned']['owned_total_processes']!=1:raise ValueError('probe was not naturally successful')
        measured=pin(args.measured_probe)
        actual_probe=load_owned(measured,'probe')
        if actual_probe['plan']['selected_instruction']!=selected:raise ValueError('actual probe used another selected instruction')
    reference_source=root/'scripts/arbiter/export/qwen/qwen3-reranker06-experiment-r1/run_focus_export.py'
    reference['reference_module']=pin(reference_source)
    wall=120 if args.mode=='probe'else 600;logdir=args.log_dir.resolve();cache=logdir/'cache'
    environment={'PYTHONPYCACHEPREFIX':str(logdir/'unwritten-pycache'),'PYTHONDONTWRITEBYTECODE':'1','PYTHONIOENCODING':'utf-8','PYTHONPATH':os.pathsep.join([str(O),str(Path(reference['reference_module']['path']).parent),reference['site_packages']]),'CUBLAS_WORKSPACE_CONFIG':':4096:8','CUDA_MODULE_LOADING':'LAZY','HF_HUB_OFFLINE':'1','TRANSFORMERS_OFFLINE':'1','HF_HUB_DISABLE_TELEMETRY':'1','TORCH_HOME':str(cache/'torch'),'CUDA_CACHE_PATH':str(cache/'cuda'),'TOKENIZERS_PARALLELISM':'false'}
    sources={str(path):pin(path)for path in O.glob('*.py')}
    sources[str(identity_path)]=pin(identity_path)
    for path in reference_source.parent.glob('*.py'):sources[str(path.resolve())]=pin(path)
    owner_path=root/'scripts/cuda-native-execution/helpers/reference_owned.py';reference_worker=root/'scripts/cuda-native-execution/helpers/reference_worker.py'
    for path in(owner_path,reference_worker,original,cohort,replay,args.native_census,args.native_owned_receipt,args.gpu_inventory):sources[str(path.resolve())]=pin(path)
    sources[str(args.source_epoch.resolve())]=pin(args.source_epoch)
    for name in('held_failed_plan','held_failed_execution','original_reference_plan','native_build_receipt','native_image'):
        item=epoch[name];sources[item['path']]=item
    for value in sources.values():
        if value['path']in operational_seals and operational_seals[value['path']]!=value['sha256']:raise ValueError('new source must not overwrite immutable inherited seal')
    immutable_seals=dict(operational_seals);immutable_seals.update({v['path']:v['sha256']for v in sources.values()if v['path']!=str(args.gpu_inventory.resolve())})
    if args.selection:sources[str(args.selection.resolve())]=pin(args.selection)
    if args.measured_probe:sources[str(args.measured_probe.resolve())]=pin(args.measured_probe)
    seals=dict(operational_seals);seals.update({v['path']:v['sha256']for v in sources.values()})
    # Added worker/owner source identity is explicit, not a retag of old runs.
    reference['input_seals']=seals
    plan={'schema':'airk.private.F1-F2-owned-plan/1','mode':args.mode,'human_sid':identity['sid'],'repository_root':str(root),'native_census':pin(args.native_census),'native_owned_receipt':native_receipt,'cohort':pin(cohort),'replay':pin(replay),'selection':pin(args.selection)if args.selection else None,'selected_instruction':selected,'general_replay_instruction':'publisher','measured_probe':measured,'max_optimizer_steps':1 if args.mode=='probe'else 14,'gpu':gpu,'nvidia_smi':str(Path(os.environ.get('SystemRoot',r'C:\Windows'))/'System32/nvidia-smi.exe'),'log_dir':str(logdir),'worker_output':str(logdir/'worker'),'environment':environment,'versions':reference['versions'],'base_python':reference['base_python'],'reference_module':reference['reference_module'],'reference_worker':pin(reference_worker),'reference_plan':reference,'ownership_module':pin(owner_path),'human_identity_helper':pin(identity_path),'worker':pin(O/'focus_worker.py'),'execution_sources':sources,'input_seals':seals,'immutable_input_seals':immutable_seals,'limits':{'wall_seconds':wall,'host_committed_bytes':16*1024**3,'cuda_allocator_bytes':8*1024**3,'minimum_cuda_free_bytes':8*1024**3},'qualification':False,'promotion':False,'original_seed':1908,'optimizer':{'rank':16,'alpha':16,'lr':5e-5,'effective_batch':32,'microbatch':1,'epochs':1,'dropout':0}}
    plan['continuity_key']=continuity_key(plan)
    if args.mode!='score':
        actual_score=load_owned(selection['owned_score_receipt'],'score')
        if selection['schema']!='airk.private.F1-fixedDEV-choice/2'or selection['continuity_key']!=actual_score['continuity_key']or actual_score['continuity_key']!=plan['continuity_key']or selection['score_report']['sha256']!=actual_score['execution']['worker_result_sha256']:raise ValueError('actual SCORE model/runtime/data/source/owner/caps continuity differs')
        if logic.choose_instruction(actual_score['worker'],rows)!=choice:raise ValueError('actual owned SCORE choice differs')
    if args.mode=='train'and actual_probe['continuity_key']!=plan['continuity_key']:raise ValueError('actual probe model/runtime/data/source/owner/caps continuity differs')
    args.output.write_text(json.dumps(plan,indent=2)+'\n',encoding='utf8');print(json.dumps(pin(args.output)))
if __name__=='__main__':main()
