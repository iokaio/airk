# SPDX-License-Identifier: Apache-2.0
"""Prepare or consume an opaque grade seal only after six fixed FINAL families naturally complete."""
from pathlib import Path
import argparse,hashlib,json,types,time
HERE=Path(__file__).resolve().parent
REPO=HERE.parents[3]
def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def require(value,message):
    if not value:raise ValueError(message)
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--packet-sha256',required=True);parser.add_argument('--runner-sha256',required=True)
    parser.add_argument('--prepare-grade-seal',action='store_true');parser.add_argument('--grade-seal-sha256');parser.add_argument('--execute-reviewed-opaque-grade',action='store_true');args=parser.parse_args()
    packet_path=HERE/'final-execution-packet.json';require(sha(packet_path)==args.packet_sha256 and sha(__file__)==args.runner_sha256,'reviewed opaque grading identity differs')
    p=json.loads(packet_path.read_bytes());require(p['status']=='FROZEN_BEFORE_ANY_FRESH_FINAL_MODEL_OUTCOME','fixed final packet required')
    owner_path=HERE/'run_final.py';require(sha(owner_path)==p['pins'][str(owner_path)],'reviewed FINAL wrapper changed')
    native=types.ModuleType('_grade_native_closure');native.__file__=str(owner_path);exec(compile(owner_path.read_bytes(),str(owner_path),'exec'),native.__dict__)
    proof=[]
    for stage in p['stages']:
        execution=HERE/('execution-FINAL-'+stage['family'])/'execution.json';request=execution.parent/'request.json';capture=Path(stage['environment']['AIRK_NATIVE_FINAL205_OUTPUT'])
        e=json.loads(execution.read_bytes());q=json.loads(request.read_bytes())
        require(e['complete'] is True and native.natural(e['owned']) and e['post_input_drift']==[],'all six native families must complete unchanged naturally before grade')
        require(e['packet_sha256']==q['packet_sha256']==args.packet_sha256 and e['candidate_freeze_sha256']==q['candidate_freeze_sha256']==p['candidate_freeze_sha256'],'native execution/freeze identity differs')
        require(e['capture_sha256']==sha(capture),'canonical capture bytes differ')
        proof.append({'role':stage['role'],'family':stage['family'],'execution':{'path':str(execution),'sha256':sha(execution)},'request':{'path':str(request),'sha256':sha(request)},'capture':{'path':str(capture),'sha256':e['capture_sha256']}})
    require(len(proof)==6 and len({r['role'] for r in proof})==6,'exact six final roles required')
    grade_seal_path=HERE/'grade-input-seal.json'
    if args.prepare_grade_seal:
        require(not args.execute_reviewed_opaque_grade and not grade_seal_path.exists(),'fresh grade preparation only')
        runtime_source=REPO/'artifacts/advanced-split/models/a1-fresh-final-evaluation-r1/prepare-input-seal.proposed.json'
        require(sha(runtime_source)==p['pins'][str(runtime_source)],'reviewed opaque runtime authority changed')
        old=json.loads(runtime_source.read_bytes());require(old['runtime_pin_authority']['inherited_runtime_files']==137,'original opaque runtime authority differs')
        runtime_rows=old['files'][:137]
        input_pins=dict(p['pins']);input_pins.update({str(packet_path):args.packet_sha256,str(Path(__file__).resolve()):args.runner_sha256})
        for row in runtime_rows:input_pins[row['path']]=row['sha256']
        for row in proof:
            for k in ['execution','request','capture']:input_pins[row[k]['path']]=row[k]['sha256']
        helper=HERE/'retained-opaque-job-helper.py'
        require(sha(helper)==p['pins'][str(helper)],'reviewed opaque process helper changed')
        input_pins[str(helper)]=p['pins'][str(helper)]
        data={'schema':'airk.private-fresh-FINAL-opaque-grade-input-seal/1','packet_sha256':args.packet_sha256,
            'candidate_freeze_sha256':p['candidate_freeze_sha256'],'captures':proof,'pins':input_pins,
            'original_opaque_runtime_authority':{'path':str(runtime_source),'sha256':sha(runtime_source),'runtime_files':137},
            'job_helper':{'path':str(helper),'sha256':sha(helper)},'limits':p['grade_limits'],'FINAL_grade_executed':False}
        grade_seal_path.write_text(json.dumps(data,indent=2)+'\n',encoding='utf-8');print(json.dumps({'grade_input_seal_sha256':sha(grade_seal_path),'capture_count':6,'FINAL_grade_executed':False}));return
    require(args.execute_reviewed_opaque_grade and args.grade_seal_sha256 and sha(grade_seal_path)==args.grade_seal_sha256,'separate reviewed immutable opaque grade release required')
    seal=json.loads(grade_seal_path.read_bytes());require(seal['packet_sha256']==args.packet_sha256 and seal['captures']==proof,'actual six-role outputs differ from grade freeze')
    for path,expected in seal['pins'].items():require(sha(native.regular(path))==expected,'opaque grade source/runtime/input differs')
    output=HERE/'grade-execution-r1.json';log=HERE/'grade-execution-r1.log';require(not output.exists() and not log.exists(),'preserve previous grade attempt')
    helper_path=Path(seal['job_helper']['path']);helper=types.ModuleType('_opaque_final_grade_owner');helper.__file__=str(REPO/'scripts/provision-static-onnx-runtime.py');exec(compile(helper_path.read_bytes(),helper.__file__,'exec'),helper.__dict__)
    argv=[p['grade_runtime'],'-I',p['grade_observer'],'grade','--protocol',p['grading_protocol'],
        '--protocol-sha256','1feb3036243249ccfd18c7d75af0adae4b9b908301ed17a12639ac36fc8927f5',
        '--execute-reviewed-opaque-operation','--candidate-freeze',p['candidate_freeze'],'--candidate-freeze-sha256',p['candidate_freeze_sha256'],
        '--fixture',p['frozen_source_fixture'],'--manifest-sha256',p['grading_manifest_sha256'],'--rubric-sha256',p['grading_rubric_sha256'],
        '--captures',*[r['capture']['path'] for r in proof],'--output',p['grade_output']]
    started=time.monotonic();owned=helper.run_owned(argv,REPO,log,60)
    stable=True
    for path,expected in seal['pins'].items():
        try:
            if sha(native.regular(path))!=expected:stable=False
        except OSError:stable=False
    strict=helper.successful(owned) and owned.get('owned_job_forced_termination') is False
    summaries=[]
    if log.exists():
        for line in log.read_text(encoding='utf-8',errors='replace').splitlines():
            if line.startswith('{'):
                try:row=json.loads(line)
                except json.JSONDecodeError:continue
                if row.get('complete') is True and row.get('quality_denominator')==205:summaries.append(row)
    receipt={'schema':'airk.private-fresh-FINAL-opaque-grade-execution/1','grade_input_seal_sha256':args.grade_seal_sha256,'packet_sha256':args.packet_sha256,
        'command':argv,'owned':owned,'strict_normal_close':strict,'inputs_unchanged':stable,'elapsed_seconds':time.monotonic()-started,
        'complete':strict and stable and len(summaries)==1,'summary':summaries[0] if len(summaries)==1 else None,
        'questions_answers_evidence_gold_inspected_by_developer':False,'release_or_full_A1_qualification':False}
    output.write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps(receipt))
    if not receipt['complete']:raise SystemExit(1)
if __name__=='__main__':main()
