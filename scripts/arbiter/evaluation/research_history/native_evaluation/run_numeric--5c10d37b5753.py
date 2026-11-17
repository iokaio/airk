# SPDX-License-Identifier: Apache-2.0
"""One Root-reviewed R54 candidate numeric stage. No FINAL inputs or execution."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import subprocess
import time
import types
HERE=Path(__file__).resolve().parent
REPO=HERE.parents[3]

def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def require(value,message):
    if not value:raise ValueError(message)

def natural(result):
    return (result.get('exit_code')==0 and result.get('timed_out') is False and result.get('log_oversized') is False
        and result.get('owned_descendants_settled') is True and result.get('owned_job_forced_termination') is False
        and result.get('owned_job_final_active_processes')==0 and result.get('owned_job_final_member_pids')==[]
        and result.get('owned_total_processes')==1 and not result.get('unexpected_third_process') and not result.get('cleanup_refusal'))

def validate_capture(value,packet):
    require(value.get('complete') is True and value.get('evaluation') is True,'complete evaluation numeric capture required')
    families=value['families']
    require(len(families)==2 and {f['family']:f['model_json_sha256'] for f in families}==packet['expected_families'], 'exact selectedU8/S8 model population required')
    for family in families:
        require(family['candidate_fp32_floor_passed'] is True and len(family['cases'])==12,'all12 independent cases and original FP32 floor required')
        require(family['mixed_length_execution']['workers']==1,'native B1 caller-order proof required')
        for case in family['cases']:
            require(0<=case['max_component_error']<=1e-5 and case['candidate_vs_fp32_cosine']>=0.995
                    and 0<=case['candidate_vs_fp32_l2']<=0.1,'unchanged original independent numerical floor failed')

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--packet-sha256',required=True);parser.add_argument('--runner-sha256',required=True)
    parser.add_argument('--execute-reviewed',action='store_true');args=parser.parse_args()
    packet_path=HERE/'numeric-packet.json';require(sha(packet_path)==args.packet_sha256 and sha(__file__)==args.runner_sha256,'reviewed numeric packet/runner changed')
    packet=json.loads(packet_path.read_bytes());pins={Path(p):v for p,v in packet['pins'].items()}
    pins.update({packet_path:args.packet_sha256,Path(__file__).resolve():args.runner_sha256})
    for path,expected in pins.items():
        stat=path.stat(follow_symlinks=False)
        require(path.is_file() and not path.is_symlink() and stat.st_nlink==1 and not getattr(stat,'st_file_attributes',0)&0x400
                and sha(path)==expected,'numeric source/artifact/input identity changed: '+str(path))
    compiled=json.loads(Path(packet['compile_receipt']['path']).read_bytes());require(compiled['exit_code']==0,'actual successful R54 compile receipt required')
    for path,expected in packet['compiled_source_seals'].items():require(compiled['groups']['native']['files'][path]==expected,'compiler source relationship differs')
    require('crates/airk-kernel/src/inference_owner/evaluation_tests/candidate.rs' in Path(packet['kernel_depinfo']).read_text().replace('\\','/'),'R54 did not compile numeric owner')
    anchor=Path(packet['anchor']);require({p.name for p in anchor.iterdir()}=={'tmp'} and not any((anchor/'tmp').iterdir()),'fresh exact current-SID numeric anchor required')
    require(Path(packet['output']).resolve()==anchor.resolve()/'capture.json' and not Path(packet['output']).exists(),'fresh exact numeric output required')
    if not args.execute_reviewed:
        print(json.dumps({'static_preflight':True,'sealed_files':len(pins),'stage_started':False,'selector':packet['selector'],'families':2,'independent_cases_each':12}));return
    require(os.name=='nt','reviewed Windows owned-process stage required')
    work=HERE/'numeric-execution-r1';require(not work.exists(),'preserve prior attempt; no overwrite/retry')
    binary=Path(packet['binary']['path']);listed=subprocess.run([str(binary),'--exact',packet['selector'],'--ignored','--list'],cwd=REPO,capture_output=True,text=True,timeout=10)
    require(listed.returncode==0 and listed.stdout.count(packet['selector']+': test')==1,'exactly one actual native test required')
    source=Path(packet['owned_process_source']);owner=types.ModuleType('_R54_numeric_owned');owner.__file__=str(source);exec(compile(source.read_bytes(),str(source),'exec'),owner.__dict__)
    environment={k:v for k,v in os.environ.items() if not k.startswith('AIRK_NATIVE_') and k!='AIRK_CANDIDATE_ENCODER_INPUT_SHA256'};environment.update(packet['environment'])
    argv=[str(binary),'--exact',packet['selector'],'--ignored','--nocapture','--test-threads=1'];work.mkdir();(work/'stdin.txt').write_bytes(b'')
    request={'argv':argv,'environment':packet['environment'],'limits':packet['limits'],'packet_sha256':args.packet_sha256,
        'runner_sha256':args.runner_sha256,'pre_input_seals':{str(p):v for p,v in pins.items()},'selector_list':listed.stdout}
    (work/'request.json').write_text(json.dumps(request,indent=2)+'\n',encoding='utf-8')
    def suspended(api,job,info,deadline):
        members,active,total=owner.job_snapshot(api,job);require(members==[int(info.pid)] and active==total==1,'one expected suspended native process required')
        image=owner.image_proof(api,info.process,packet['binary']);require(time.monotonic()<deadline,'original deadline expired before resume')
        return {'members':members,'active':active,'total':total,'image':image}
    started=time.monotonic()
    with (work/'native.log').open('xb') as log,(work/'stdin.txt').open('rb') as stdin:
        owned=owner.run_windows_owned_training(argv,REPO,log,packet['limits']['seconds'],environment,stdin,packet['limits']['host_bytes'],before_resume=suspended)
    drift=[str(p) for p,expected in pins.items() if sha(p)!=expected]
    result={'owned':owned,'elapsed_seconds':time.monotonic()-started,'post_input_drift':drift,'packet_sha256':args.packet_sha256,
        'runner_sha256':args.runner_sha256,'complete':False,'FINAL_executed':False,'quality_or_release_qualified':False}
    output=Path(packet['output'])
    if output.is_file():
        result['capture_sha256']=sha(output)
        try:validate_capture(json.loads(output.read_bytes()),packet);result['complete']=natural(owned) and not drift
        except (ValueError,KeyError,TypeError) as error:result['numeric_refusal']=str(error)
    (work/'execution.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
    if not result['complete']:raise SystemExit(1)
if __name__=='__main__':main()
