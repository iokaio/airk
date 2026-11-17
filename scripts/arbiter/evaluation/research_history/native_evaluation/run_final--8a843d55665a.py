# SPDX-License-Identifier: Apache-2.0
"""Closed Root-owned opaque FINAL stage. Requires separately finalized pre-outcome freeze."""
from pathlib import Path
import argparse,hashlib,json,os,subprocess,time,types
HERE=Path(__file__).resolve().parent
REPO=HERE.parents[3]
def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def require(value,message):
    if not value:raise ValueError(message)
def regular(path):
    path=Path(path).absolute()
    for item in (path,*path.parents):
        state=item.stat(follow_symlinks=False)
        require(not item.is_symlink() and not getattr(state,'st_file_attributes',0)&0x400,'input path alias/reparse point refused')
    state=path.stat(follow_symlinks=False)
    require(path.is_file() and state.st_nlink==1,'input is not an ordinary unique file')
    return path
def natural(r):
    return (r.get('exit_code')==0 and r.get('timed_out') is False and r.get('log_oversized') is False and r.get('owned_descendants_settled') is True
        and r.get('owned_job_forced_termination') is False and r.get('owned_job_final_active_processes')==0 and r.get('owned_job_final_member_pids')==[]
        and r.get('owned_total_processes')==1 and not r.get('unexpected_third_process') and not r.get('cleanup_refusal'))
def validate_capture(c,stage,p,freeze):
    require(c.get('schema')=='airk.native-fresh-final-retrieval-family/1' and c.get('complete') is True,'complete canonical FINAL capture required')
    require(c['family']==stage['family'] and c['candidate_freeze_sha256']==p['candidate_freeze_sha256'],'foreign family/freeze')
    for key,value in p['capture_common'].items():require(c[key]==value,'canonical input/runtime/query identity differs')
    model=next(row for row in freeze['families'] if row['family']==stage['family'])
    for key in ['model_json_sha256','pipeline_digest','encoder_input_sha256','runtime']:require(c[key]==model[key],'frozen model/pipeline differs')
    require(c['source_before_sha256']==c['source_after_sha256']==c['source_roster_sha256'],'source roster drift')
    records=c['records'];require(len(records)==205 and len({r['id'] for r in records})==205,'exact205 unique native outcomes required')
    require(c['collections'] and all(row['reused'] is False for row in c['collections']),'fresh native publications required')
    for record in records:
        require('primary_vector' in record and 'native_error' not in record,'partial/failed native record')
        body=record['primary_vector'];require(len(body['hits'])<=10 and any(row['publication']==body['publication'] for row in c['collections']),'native result publication/top10 differs')
    # Do not emit question IDs, evidence text, source paths, answers or gold.

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--packet-sha256',required=True);parser.add_argument('--runner-sha256',required=True)
    parser.add_argument('--family',required=True);parser.add_argument('--execute-reviewed',action='store_true');args=parser.parse_args()
    path=HERE/'final-execution-packet.json';require(sha(regular(path))==args.packet_sha256 and sha(regular(__file__))==args.runner_sha256,'reviewed FINAL execution identity differs')
    p=json.loads(path.read_bytes());require(p['status']=='FROZEN_BEFORE_ANY_FRESH_FINAL_MODEL_OUTCOME','Root freeze required')
    freeze_path=Path(p['candidate_freeze']);require(sha(regular(freeze_path))==p['candidate_freeze_sha256'],'actual frozen barrier differs')
    freeze=json.loads(freeze_path.read_bytes());require(freeze['status']=='FROZEN_BEFORE_ANY_FRESH_FINAL_MODEL_OUTCOME','native freeze status differs')
    found=[s for s in p['stages'] if s['family']==args.family];require(len(found)==1,'one fixed reviewed FINAL family required');stage=found[0]
    pins={Path(k):v for k,v in p['pins'].items()};pins.update({path:args.packet_sha256,Path(__file__).resolve():args.runner_sha256})
    opaque={Path(p[k]) for k in ['opaque_corpus','opaque_manifest','opaque_rubric']}
    for file,pin in pins.items():
        regular(file)
        if file not in opaque or args.execute_reviewed:require(sha(file)==pin,'consumed source/runtime/input hash differs')
    compiled=json.loads(Path(p['compile_receipt']['path']).read_bytes());require(compiled['exit_code']==0,'successful actualR54 compile required')
    dep=Path(p['kernel_depinfo']).read_text().replace('\\','/')
    require('crates/airk-kernel/src/inference_owner/evaluation_tests/fresh_final.rs' in dep,'selected native FINAL owner was not compiled')
    anchor=Path(stage['anchor']);require({x.name for x in anchor.iterdir()}=={'tmp'} and not any((anchor/'tmp').iterdir()),'fresh private native anchor required')
    for root in p['frozen_roots']:
        root=Path(root).resolve();require(not anchor.resolve().is_relative_to(root) and not root.is_relative_to(anchor.resolve()),'FINAL output overlaps frozen input')
    for envkey,child in [('AIRK_NATIVE_FINAL205_DATA','data'),('AIRK_NATIVE_FINAL205_OUTPUT','capture.json')]:
        target=Path(stage['environment'][envkey]);require(target.resolve()==anchor.resolve()/child and not target.exists(),'fresh exact native output geometry required')
    if not args.execute_reviewed:
        print(json.dumps({'static_preflight':True,'family':args.family,'source_pins':len(pins),'opaque_inputs_stat_only':True,'FINAL_neural_execution':False}));return
    require(os.name=='nt','reviewed Windows process owner required')
    for previous in p['stages'][:p['stages'].index(stage)]:
        previous_path=HERE/('execution-FINAL-'+previous['family'])/'execution.json';require(previous_path.is_file(),'previous fixed family must complete first')
        record=json.loads(previous_path.read_bytes());require(record['complete'] is True and natural(record['owned']) and not record['post_input_drift'] and record['packet_sha256']==args.packet_sha256,'stop batch on first failure')
    work=HERE/('execution-FINAL-'+stage['family']);require(not work.exists(),'preserve previous attempt; no replay')
    binary=Path(p['binary']['path']);listed=subprocess.run([str(binary),'--exact',p['selector'],'--ignored','--list'],cwd=REPO,capture_output=True,text=True,timeout=10)
    require(listed.returncode==0 and listed.stdout.count(p['selector']+': test')==1,'exactly one compiled native FINAL entry required')
    source=Path(p['owned_process_source']);owner=types.ModuleType('_opaque_FINAL_owned');owner.__file__=str(source);exec(compile(source.read_bytes(),str(source),'exec'),owner.__dict__)
    environment={k:v for k,v in os.environ.items() if not k.startswith('AIRK_NATIVE_') and k!='AIRK_CANDIDATE_ENCODER_INPUT_SHA256'};environment.update(stage['environment'])
    argv=[str(binary),'--exact',p['selector'],'--ignored','--nocapture','--test-threads=1'];work.mkdir();(work/'stdin.txt').write_bytes(b'')
    request={'argv':argv,'environment':stage['environment'],'limits':p['limits'],'packet_sha256':args.packet_sha256,'runner_sha256':args.runner_sha256,
        'candidate_freeze_sha256':p['candidate_freeze_sha256'],'pre_input_seals':{str(file):pin for file,pin in pins.items()},'selector_list':listed.stdout}
    (work/'request.json').write_text(json.dumps(request,indent=2)+'\n',encoding='utf-8')
    def suspended(api,job,info,deadline):
        members,active,total=owner.job_snapshot(api,job);require(members==[int(info.pid)] and active==total==1,'one expected native process required')
        image=owner.image_proof(api,info.process,p['binary']);require(time.monotonic()<deadline,'original deadline expired before resume');return {'members':members,'active':active,'total':total,'image':image}
    started=time.monotonic()
    with (work/'native.log').open('xb') as log,(work/'stdin.txt').open('rb') as stdin:
        owned=owner.run_windows_owned_training(argv,REPO,log,p['limits']['seconds'],environment,stdin,p['limits']['host_bytes'],before_resume=suspended)
    drift=[]
    for file,pin in pins.items():
        try:
            if sha(file)!=pin:drift.append(str(file))
        except OSError:drift.append(str(file))
    result={'owned':owned,'elapsed_seconds':time.monotonic()-started,'post_input_drift':drift,'packet_sha256':args.packet_sha256,
        'candidate_freeze_sha256':p['candidate_freeze_sha256'],'family':args.family,'complete':False,'quality_or_release_qualified':False,
        'developer_questions_answers_evidence_gold_inspected':False}
    capture=Path(stage['environment']['AIRK_NATIVE_FINAL205_OUTPUT'])
    if capture.is_file():
        result['capture_sha256']=sha(capture)
        try:validate_capture(json.loads(capture.read_bytes()),stage,p,freeze);result['complete']=natural(owned) and not drift
        except (ValueError,KeyError,TypeError,OSError):result['capture_refusal']='Canonical frozen FINAL metadata/count/shape refusal; private evidence retained.'
    (work/'execution.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result))
    if not result['complete']:raise SystemExit(1)
if __name__=='__main__':main()
