# SPDX-License-Identifier: Apache-2.0
"""Finalize exact inputs only after Root records the pre-outcome CAL scope decision. Never executes FINAL."""
from pathlib import Path
import argparse,hashlib,json
HERE=Path(__file__).resolve().parent
SCOPE='embedding_dynamic_quantization_export_provenance_only'
def sha(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def require(value,message):
    if not value:raise ValueError(message)
def validate_scope_record(decision,proposal_sha):
    require(decision['schema']=='airk.private-final-calibration-scope-record/1' and decision['status']=='RECORDED_BEFORE_ANY_FRESH_FINAL_MODEL_OUTCOME', 'Root pre-outcome scope decision required')
    require(decision['scope']==SCOPE and decision['proposal_sha256']==proposal_sha and decision['decision'].strip(),'exact proposed dynamic-embedding scope must be actually reviewed/recorded')
    require(decision['positive_learned_CAL_fit_claimed'] is False and decision['optional_Arbiter_CAL_remains_unsupported'] is True,'do not fabricate positive CAL or qualify optional models')

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prepared-packet-sha256',required=True);parser.add_argument('--prepared-freeze-sha256',required=True)
    parser.add_argument('--finalizer-sha256',required=True);parser.add_argument('--recorded-cal-scope',type=Path,required=True)
    parser.add_argument('--recorded-cal-scope-sha256',required=True);args=parser.parse_args()
    pp=HERE/'final-execution-packet.prepared.json';fp=HERE/'candidate-freeze.prepared.json';proposal=HERE/'calibration-scope-decision.proposed.json'
    require(sha(pp)==args.prepared_packet_sha256 and sha(fp)==args.prepared_freeze_sha256 and sha(__file__)==args.finalizer_sha256,'reviewed preparation identity differs')
    require(sha(args.recorded_cal_scope)==args.recorded_cal_scope_sha256,'actual recorded CAL scope differs')
    decision=json.loads(args.recorded_cal_scope.read_bytes())
    validate_scope_record(decision,sha(proposal))
    packet=json.loads(pp.read_bytes());freeze=json.loads(fp.read_bytes())
    opaque={Path(packet[k]) for k in ['opaque_corpus','opaque_manifest','opaque_rubric']}
    for path,pin in packet['pins'].items():
        path=Path(path)
        if path in opaque:require(path.is_file(),'opaque input absent')
        else:require(sha(path)==pin,'prepared source/model/receipt changed: '+str(path))
    numeric=json.loads((HERE/'numeric-execution-r1/execution.json').read_bytes())
    require(numeric['complete'] is True and numeric['post_input_drift']==[] and numeric['owned']['exit_code']==0
            and numeric['owned']['owned_job_forced_termination'] is False and numeric['owned']['owned_job_final_active_processes']==0,
            'actual current-R54 numeric admission required')
    freeze_path=HERE/'candidate-freeze.json';packet_path=HERE/'final-execution-packet.json'
    require(not freeze_path.exists() and not packet_path.exists(),'do not overwrite an earlier freeze or execution identity')
    freeze['status']='FROZEN_BEFORE_ANY_FRESH_FINAL_MODEL_OUTCOME';freeze['calibration_sha256']=args.recorded_cal_scope_sha256
    freeze.pop('preparation_only',None)
    freeze['scope_record_path']=str(args.recorded_cal_scope.resolve())
    freeze['final_neural_execution_authorized_by_this_file']=False
    freeze_path.write_text(json.dumps(freeze,indent=2)+'\n',encoding='utf-8');freeze_sha=sha(freeze_path)
    packet['status']='FROZEN_BEFORE_ANY_FRESH_FINAL_MODEL_OUTCOME';packet['candidate_freeze_sha256']=freeze_sha
    packet['pins'][str(freeze_path)]=freeze_sha;packet['pins'][str(args.recorded_cal_scope.resolve())]=args.recorded_cal_scope_sha256
    for stage in packet['stages']:stage['environment']['AIRK_NATIVE_FINAL205_CANDIDATE_FREEZE_SHA256']=freeze_sha
    packet_path.write_text(json.dumps(packet,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'candidate_freeze_sha256':freeze_sha,'execution_packet_sha256':sha(packet_path),'six_roles':len(freeze['families']),
        'FINAL_executed':False,'root_explicit_execution_release_still_required':True}))
if __name__=='__main__':main()
