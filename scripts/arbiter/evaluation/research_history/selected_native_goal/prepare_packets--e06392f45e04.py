# SPDX-License-Identifier: Apache-2.0
"""Pure exact13 phase metadata producer. No processes, model calls or native DTOs."""
import argparse,json,os
from pathlib import Path
from chunk_support import pin,verified_json,need,partitions,global_deadline
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[5]
def prepare(release_ref,protocol_ref,bound_ref,runtime_python,node,owner,output,execution):
    release=verified_json(release_ref);protocol=verified_json(protocol_ref)
    need(protocol['schema']=='airk.private-cal-chunked-census/1','original protocol required')
    need(release.get('chunk_protocol_sha256')==protocol_ref['sha256'] and release.get('original_fixture_bound')==bound_ref,'actual common live release must bind original protocol and fixture expiry')
    scopes=verified_json(protocol['frozen_question_scopes']);ids=[q['id'] for q in scopes['questions']]
    need(protocol['partitions']==[dict(part,questions=len(part['questions'])) for part in partitions(ids)],'original100orderedIDs/ranges differ')
    global_deadline(bound_ref)
    output=Path(output).resolve();execution=Path(execution).resolve()
    need(not os.path.lexists(output) and not os.path.lexists(execution) and output.is_relative_to(ROOT/'artifacts') and execution.is_relative_to(ROOT/'artifacts'),'fresh private plan and execution roots required')
    need(not output.is_relative_to(execution) and not execution.is_relative_to(output),'immutable metadata and live outputs must be separate')
    output.mkdir()
    sources=[HERE/name for name in ('capture_chunk.py','native_goal_client.py','native_eval_session.cjs','goal_sources.py','chunk_support.py','run_controller.py','prepare_packets.py')]
    source_pins={str(path):pin(path)['sha256'] for path in sources}
    phases=[];stop=str(Path(release['paths']['fixture_ready']).parent/'chunked-cal-stop.requested')
    for part in protocol['partitions']:
        index=part['chunk_index'];root=execution/f'phase-{index:02d}'
        phase={'schema':'airk.private-selected-cal-chunk-packet/1','chunk_index':index,'question_start':part['question_start'],'question_end':part['question_end'],'questions':ids[part['question_start']:part['question_end']],'protocol':protocol_ref,'release':release_ref,'fixture_bound':bound_ref,'owner_seconds':1800,'runtime_python':pin(runtime_python),'node':pin(node),'owner':pin(owner),'capture_driver':pin(HERE/'capture_chunk.py'),'input_pins':source_pins,'capture_output':str(root/'capture'),'owner_output':str(root/'owner'),'stop_file':stop}
        path=output/f'phase-{index:02d}.json';path.write_text(json.dumps(phase,indent=2)+'\n',encoding='utf-8');phases.append(dict(pin(path),chunk_index=index))
    packet={'schema':'airk.private-selected-cal-chunk-controller-packet/1','protocol':protocol_ref,'release':release_ref,'fixture_bound':bound_ref,'phase_packets':phases,'output':str(execution),'input_pins':source_pins}
    path=output/'controller-packet.json';path.write_text(json.dumps(packet,indent=2)+'\n',encoding='utf-8');return pin(path)
if __name__=='__main__':
    p=argparse.ArgumentParser()
    for name in ('release','protocol','fixture-bound','runtime-python','node','owner','output','execution'):p.add_argument('--'+name,required=True)
    a=p.parse_args();print(json.dumps(prepare(pin(a.release),pin(a.protocol),pin(a.fixture_bound),a.runtime_python,a.node,a.owner,a.output,a.execution)))
