# SPDX-License-Identifier: Apache-2.0
"""Root-owned opaque alignment launch; observer never opens emitted task corpus."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import types
HERE=Path(__file__).resolve().parent

def sha(p):
    with Path(p).open('rb')as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('protocol-sha256','reservation-sha256','root-decision-sha256','metadata-execution-sha256'):parser.add_argument('--'+name,required=True)
    for name in ('reservation','root-decision','metadata-execution'):parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--execute-reviewed-opaque-alignment',action='store_true',required=True)
    args=parser.parse_args();protocol=HERE/'materialization-protocol.proposed.json';receipt=HERE/'execution-alignment-r1.json'
    expected={protocol:args.protocol_sha256,args.reservation:args.reservation_sha256,args.root_decision:args.root_decision_sha256,args.metadata_execution:args.metadata_execution_sha256}
    if receipt.exists()or any(sha(path)!=value for path,value in expected.items()):raise ValueError('reviewed fresh launch identity required')
    p=json.loads(protocol.read_bytes());metadata=json.loads(args.metadata_execution.read_bytes())
    if metadata.get('complete')is not True or metadata.get('strict_normal_close')is not True or metadata.get('inputs_unchanged')is not True or metadata.get('reservation_sha256')!=args.reservation_sha256 or metadata.get('protocol_sha256')!=p['metadata_protocol']['sha256'] or metadata.get('task_question_gold_corpus_opened')is not False:raise ValueError('metadata admission execution not closed')
    seal=json.loads(Path(p['runtime_seal']['path']).read_bytes())
    if sha(p['runtime_seal']['path'])!=p['runtime_seal']['sha256']:raise ValueError('runtime seal drift')
    rows=list(p['inputs'].values())+seal['files'];before={row['path']:sha(row['path'])for row in rows}
    if any(before[row['path']]!=row['sha256']for row in rows):raise ValueError('source/runtime/asset drift')
    helper_pin=p['inputs']['job_helper'];raw=Path(helper_pin['path']).read_bytes()
    helper=types.ModuleType('sealed_selected_alignment_job_owner');helper.__file__=helper_pin['path'];exec(compile(raw,helper.__file__,'exec'),helper.__dict__)
    argv=[seal['runtime_executable'],'-I',str(HERE/'materialize.py'),'--protocol',str(protocol),'--protocol-sha256',args.protocol_sha256,'--reservation',str(args.reservation),'--reservation-sha256',args.reservation_sha256,'--root-decision',str(args.root_decision),'--root-decision-sha256',args.root_decision_sha256,'--execute-reviewed-opaque-alignment']
    result=helper.run_owned(argv,Path.cwd(),HERE/'execution-alignment-r1.log',60)
    unchanged=all(sha(path)==value for path,value in before.items())and all(sha(path)==value for path,value in expected.items())
    settled=helper.successful(result)and result.get('owned_job_forced_termination')is False
    counts=Path(p['output_anchor'])/'materialized/opaque-counts.json';opaque=json.loads(counts.read_bytes())if counts.exists()else None
    report={'schema':'airk.private-selected-Context-opaque-alignment-owned/1','owned_result':result,'strict_normal_close':settled,'inputs_unchanged':unchanged,'opaque':opaque,'observer_parsed_question_gold_corpus':False,'complete':bool(settled and unchanged and opaque and opaque.get('complete'))}
    with receipt.open('x',encoding='utf-8')as stream:json.dump(report,stream,indent=2);stream.write('\n')
    print(json.dumps(report));return 0 if report['complete']else 1
if __name__=='__main__':sys.exit(main())
