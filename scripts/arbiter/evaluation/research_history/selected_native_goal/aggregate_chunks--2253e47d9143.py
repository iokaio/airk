# SPDX-License-Identifier: Apache-2.0
"""Aggregate actual thirteen owned chunks; no native execution or DTO reconstruction."""
import argparse,json,os
from pathlib import Path
from chunk_contract import validate_members,validate_aggregate,sha,need,CAPTURE,AGGREGATE,HEADER

def pin(path):return {'path':str(Path(path).resolve()),'sha256':sha(path)}
def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('protocol','controller-packet','decision'):
        p.add_argument('--'+name,type=Path,required=True);p.add_argument('--'+name+'-sha256',required=True)
    p.add_argument('--phase-execution',type=Path,action='append',required=True);p.add_argument('--phase-execution-sha256',action='append',required=True)
    p.add_argument('--output-capture',type=Path,required=True);p.add_argument('--output-execution',type=Path,required=True)
    a=p.parse_args();need(len(a.phase_execution)==len(a.phase_execution_sha256)==13,'exact13 member proofs required')
    protocol={'path':str(a.protocol.resolve()),'sha256':a.protocol_sha256};controller={'path':str(a.controller_packet.resolve()),'sha256':a.controller_packet_sha256};decision={'path':str(a.decision.resolve()),'sha256':a.decision_sha256}
    members=[{'path':str(path.resolve()),'sha256':digest}for path,digest in zip(a.phase_execution,a.phase_execution_sha256)]
    checked=validate_members(protocol,controller,decision,members);release=checked['release'];reports=checked['reports'];root=Path(__file__).resolve().parents[6]
    for output in (a.output_capture,a.output_execution):need(not output.exists()and output.resolve().is_relative_to(root/'artifacts'),'fresh private aggregate output required')
    need(a.output_capture.resolve()!=a.output_execution.resolve(),'distinct aggregate artifacts required')
    # Reconcile the unchanged original source/runtime/assets ONCE, never13duplicate rehashes.
    for path,digest in release['input_pins'].items():need(sha(path)==digest,'aggregate original input closure drift')
    for path,sentinel in release.get('absent_build_inputs',{}).items():need(sentinel=='missing'and not os.path.lexists(path),'aggregate consumed-source absence changed')
    rows=[row for report in reports for row in report['rows']]
    capture={key:release[key]for key in HEADER};capture.update(schema=CAPTURE,complete=True,phase_count=13,chunk_protocol_sha256=a.protocol_sha256,
        release_sha256=checked['controller']['release']['sha256'],Context_task_NN_invocations=0,human_semantic_gold_count=0,
        stage_before=reports[0]['stage_before'],stage_after=reports[-1]['stage_after'],rows=rows,
        phase_capture_refs=[receipt['capture']for receipt in [json.loads(Path(row['path']).read_bytes())for row in members]],
        scope='Genuine13separateownedchunks; native per-question source/scope/nonces/fingerprints/InputDigests preserved unchanged. No single1800second execution proof or partialoldrow reuse.')
    execution={'schema':AGGREGATE,'complete':True,'qualification_granted':False,'protocol':protocol,'controller_packet':controller,'decision':decision,
        'capture':None,'phases':members,'instance':checked['instance'],'native_execution_phases':13,
        'post_input_drift':[],'absent_build_inputs':release.get('absent_build_inputs',{}),'aggregate_script':pin(__file__),
        'native_work_executed_by_aggregator':False,'no_synthetic_single_phase':True}
    try:
        a.output_capture.write_text(json.dumps(capture,ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8')
        execution['capture']=pin(a.output_capture)
        a.output_execution.write_text(json.dumps(execution,indent=2)+'\n',encoding='utf-8')
        validate_aggregate(pin(a.output_execution),pin(a.output_capture),checked['controller']['release'])
    except BaseException as error:
        capture['complete']=False;execution['complete']=False;execution['failure']={'type':type(error).__name__,'message':str(error)}
        if a.output_capture.exists():
            a.output_capture.write_text(json.dumps(capture,ensure_ascii=False,separators=(',',':'))+'\n',encoding='utf-8');execution['capture']=pin(a.output_capture)
        a.output_execution.write_text(json.dumps(execution,indent=2)+'\n',encoding='utf-8')
        raise
    print(json.dumps({'complete':True,'questions':100,'phases':13,'total_probes':checked['total_probes'],'capture':pin(a.output_capture),'execution':pin(a.output_execution),'NN_executed':False}))
if __name__=='__main__':main()
