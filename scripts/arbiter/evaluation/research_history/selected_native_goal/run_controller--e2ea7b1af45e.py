# SPDX-License-Identifier: Apache-2.0
"""Root-reviewed13 phase ownership; existing run_owned primitive, no retries."""
import argparse,importlib.util,json,os,time
from pathlib import Path
from chunk_support import SIZES,need,pin,verified_json,partitions,global_deadline,check_inputs,merge_seals
ROOT=Path(__file__).resolve().parents[6]
def load_owner(reference):
    verified=pin(reference['path']);need(verified['sha256']==reference['sha256'],'owner source changed')
    spec=importlib.util.spec_from_file_location('existing_owned_process',reference['path']);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

def validate_controller(packet,decision):
    need(packet['schema']=='airk.private-selected-cal-chunk-controller-packet/1','controller packet schema differs')
    need(decision.get('schema')=='airk.private-selected-cal-chunk-controller-decision/1' and decision.get('approved') is True,'explicit Root controller decision required')
    protocol=verified_json(packet['protocol']);release=verified_json(packet['release']);bound=verified_json(packet['fixture_bound'])
    scopes=verified_json(protocol['frozen_question_scopes']);expected=partitions([q['id'] for q in scopes['questions']])
    need(protocol['partitions']==[dict(part,questions=len(part['questions'])) for part in expected],'original fixed13phase partition differs')
    need(release.get('chunk_protocol_sha256')==packet['protocol']['sha256'] and release.get('original_fixture_bound')==packet['fixture_bound'],'common live release protocol/bound differs')
    need(len(packet['phase_packets'])==13,'all13 new phase packets required')
    outputs=set()
    for index,ref in enumerate(packet['phase_packets']):
        phase=verified_json(ref);part=expected[index]
        need(ref['chunk_index']==phase['chunk_index']==index and phase['question_start']==part['question_start'] and phase['question_end']==part['question_end'] and phase['questions']==part['questions'],'ordered original chunk range differs')
        need(phase['release']==packet['release'] and phase['protocol']==packet['protocol'] and phase['fixture_bound']==packet['fixture_bound'] and phase['owner_seconds']==1800,'phase common release/budgets differ')
        for name in ('capture_output','owner_output'):
            target=Path(phase[name]).resolve();need(not os.path.lexists(target) and target.is_relative_to(Path(packet['output']).resolve()) and str(target) not in outputs,'fresh isolated phase output required');outputs.add(str(target))
        for name in ('runtime_python','capture_driver','owner','node'):need(pin(phase[name]['path'])==phase[name],'phase execution input differs')
    return protocol,release,global_deadline(packet['fixture_bound'])

def walk_phases(references,run_phase):
    """Pure stop-on-first-failure policy, injected only in cheap controls."""
    for reference in references:
        result=run_phase(reference);yield result
        if result.get('complete') is not True or result.get('stopped') is True:break

def run(packet_ref,decision_ref):
    entry_mono=time.monotonic();entry_wall=time.time();packet=verified_json(packet_ref);decision=verified_json(decision_ref)
    need(decision.get('controller_packet_sha256')==packet_ref['sha256'],'Root decision does not approve exact controller packet')
    protocol,release,absolute=validate_controller(packet,decision)
    original_monotonic_deadline=entry_mono+max(0,absolute/1000-entry_wall)
    output=Path(packet['output']);output.mkdir(exist_ok=False)
    overall={'schema':'airk.private-selected-cal-chunk-controller-execution/1','complete':False,'stopped':False,'controller_packet':packet_ref,'decision':decision_ref,'protocol_sha256':packet['protocol']['sha256'],'fixture_absolute_deadline_unix_ms':absolute,'started_unix_ms':int(entry_wall*1000),'phases':[]}
    def save():
        overall['finished_unix_ms']=time.time_ns()//1000000;overall['elapsed_seconds']=time.monotonic()-entry_mono
        (output/'execution.json').write_text(json.dumps(overall,indent=2)+'\n',encoding='utf-8')
    def remaining():return min(absolute/1000-time.time(),original_monotonic_deadline-time.monotonic())
    def phase_run(reference):
        phase_started=time.monotonic();phase_wall=time.time();phase=verified_json(reference)
        owner_output=Path(phase['owner_output']);owner_output.mkdir(parents=True,exist_ok=False)
        receipt={'schema':'airk.private-selected-cal-chunk-owned-execution/1','complete':False,'stopped':False,'chunk_index':phase['chunk_index'],'protocol_sha256':packet['protocol']['sha256'],'controller_packet':packet_ref,'decision':decision_ref,'packet':reference,'fixture_absolute_deadline_unix_ms':absolute,'started_unix_ms':int(phase_wall*1000),'post_input_drift':[],'input_seals':merge_seals(release['input_pins'],phase['input_pins'],{phase[name]['path']:phase[name]['sha256'] for name in ('runtime_python','capture_driver','owner','node')}),'absent_build_inputs':release.get('absent_build_inputs',{})}
        def retain():
            receipt['finished_unix_ms']=time.time_ns()//1000000;receipt['elapsed_seconds']=time.monotonic()-phase_started
            (owner_output/'execution.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
        try:
            check_inputs(release)
            for path,digest in phase['input_pins'].items():need(pin(path)['sha256']==digest,'phase source changed before launch')
            need(remaining()>60,'original fixture expiry cannot admit another phase')
            owner=load_owner(phase['owner'])
            argv=[phase['runtime_python']['path'],'-B','-X','utf8',phase['capture_driver']['path']]
            for name,path in release['paths'].items():argv.extend(['--'+name.replace('_','-'),path])
            argv.extend(['--release',packet['release']['path'],'--release-sha256',packet['release']['sha256'],'--phase-packet',reference['path'],'--phase-packet-sha256',reference['sha256'],'--output',phase['capture_output']])
            env=dict(os.environ);env['PATH']=str(Path(phase['node']['path']).parent)+os.pathsep+env.get('PATH','')
            env['PYTHONDONTWRITEBYTECODE']='1';env['PYTHONNOUSERSITE']='1';env['CUDA_VISIBLE_DEVICES']=''
            env['AIRK_CAL_ORIGINAL_GLOBAL_MONOTONIC_DEADLINE_NS']=str(int(original_monotonic_deadline*1e9))
            for name in ('OMP_NUM_THREADS','MKL_NUM_THREADS','OPENBLAS_NUM_THREADS','NUMEXPR_NUM_THREADS'):env[name]='1'
            receipt['argv']=argv;receipt['original_owner_seconds']=1800
            budget=min(1800-(time.monotonic()-phase_started),remaining());need(budget>0,'original phase deadline expired before launch')
            result=owner.run_owned(argv,ROOT,owner_output/'capture.log',budget,environment=env);receipt['owned']=result
            need(owner.successful(result),'actual phase child failed or did not close naturally; no retry')
            capture_path=Path(phase['capture_output'])/'capture.private.json';report=json.loads(capture_path.read_bytes());receipt['capture']=pin(capture_path)
            receipt['stopped']=report.get('stopped') is True
            need(report['schema']=='airk.private-selected-Context-FocusON-CAL-native-chunk/1' and report['chunk_index']==phase['chunk_index'] and report['phase_packet_sha256']==reference['sha256'] and report['chunk_protocol_sha256']==packet['protocol']['sha256'] and report['Context_task_NN_invocations']==0,'foreign/incomplete phase capture')
            from native_goal_client import require_normal_session_close
            require_normal_session_close(report['native_session_close'])
            check_inputs(release)
            for path,digest in phase['input_pins'].items():
                if pin(path)['sha256']!=digest:receipt['post_input_drift'].append(path)
            need(not receipt['post_input_drift'],'phase source drift')
            need(remaining()>0 and time.monotonic()-phase_started<1800,'original phase/global bound expired during postchecks')
            for key in ('stage_before','stage_after'):need(report[key]['full_native_response']['configuration']==release['frozen_configuration'],'phase original configuration differs')
            if not receipt['stopped']:
                part=protocol['partitions'][phase['chunk_index']]
                need(report.get('complete') is True and [row['id'] for row in report['rows']]==phase['questions'],'fixed complete original phase census required')
                need(sum(len(row['counterfactual_choices'])+len(row['counterfactual_refusals']) for row in report['rows'])<=5*SIZES[phase['chunk_index']],'original phase probe count exceeded')
                receipt['complete']=True
        except Exception as error:
            receipt['failure_class']=type(error).__name__;receipt['failure']=str(error)
        finally:retain()
        return receipt
    try:
        for row in walk_phases(packet['phase_packets'],phase_run):
            phase=verified_json(row['packet']);overall['phases'].append(pin(Path(phase['owner_output'])/'execution.json'));save()
            if not row['complete'] or row['stopped']:overall['stopped']=row['stopped'];break
        overall['complete']=len(overall['phases'])==13 and all(verified_json(ref)['complete'] is True for ref in overall['phases'])
    finally:save()
    return overall
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--packet',required=True);p.add_argument('--packet-sha256',required=True);p.add_argument('--decision',required=True);p.add_argument('--decision-sha256',required=True);a=p.parse_args()
    result=run({'path':a.packet,'sha256':a.packet_sha256},{'path':a.decision,'sha256':a.decision_sha256})
    if not result['complete']:raise SystemExit('chunk batch stopped or failed; no partial reuse or retry')
