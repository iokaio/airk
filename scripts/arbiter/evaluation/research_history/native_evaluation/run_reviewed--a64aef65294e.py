# SPDX-License-Identifier: Apache-2.0
"""Root-only reviewed R40 zero-forward ANN self-vector diagnostic; never builds."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import time
import types

ROOT = Path(__file__).resolve().parents[4]
HERE = Path(__file__).resolve().parent
RETAINED = ROOT / 'artifacts/advanced-split/native-harnesses/context-integrated-r40'
BINARY = RETAINED / 'airk-kernel.exe'
BINARY_SHA = '2ba55f9690b765e18fc7856407db3f6a453957d98986347de632d84b5c969454'
DEP_SHA = '5f6352c61c71b0792099e6018bbd2bac90b784d32b6de7582bb6531856c4cdcc'
COMPILE_SHA = '3be86087e9d20e26c74b82c8cee7cefb5262898189099d58fb20a1e7ae3d865c'
GROUP_SHA = 'e4fa5e113c602e9f95b724b5600073729413c891295dd26aaa89373adb668b8e'
OWNER = ROOT / 'tools/retrieval-train/training_execution.py'
OWNER_SHA = 'd363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975'
CAPTURE = ROOT / 'artifacts/advanced-split/models/candidate-native-r20-r1/visible-prefix-mining-r1-u8/capture.json'
CAPTURE_SHA = '9aec2ccc46b1d278db426da62183078702496879dd826442f5b3fb8f999caeb9'
DATA = ROOT / 'artifacts/advanced-split/models/candidate-native-r20-r1/visible-prefix-mining-r1-u8/data/visible-prefix-mining-r1-u8/index-0'
SOURCES = {
    'crates/airk-kernel/src/inference_owner/evaluation_tests/ann_self.rs': '4e8af664c66700db9b27aac2a1a20324e30b936883f781aee3e6c5854d057952',
    'crates/airk-retrieval/src/vector/mutable/self_probe.rs': '24201bbb76f9611034d4583bd7d0c757895b052cf15b08b6bf31005216963a9a',
}
PREFIX = 'inference_owner::evaluation_tests::ann_self::'
EMIT = PREFIX + 'emit_retained_dev_graph_self_probe_typed_input'
PROBE = PREFIX + 'retained_dev_graph_self_vectors_compare_actual_ann_to_exact_all_live_ids'

def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def require_file(path, expected):
    path = Path(path)
    state = path.stat(follow_symlinks=False)
    if not path.is_file() or state.st_nlink != 1 or path.is_symlink() or getattr(state, 'st_file_attributes', 0) & 0x400 or sha(path) != expected:
        raise ValueError('reviewed input file identity refused')

def natural(result):
    return (result.get('exit_code') == 0 and result.get('timed_out') is False
            and result.get('log_oversized') is False
            and result.get('owned_job_forced_termination') is False
            and result.get('owned_descendants_settled') is True
            and result.get('owned_job_final_active_processes') == 0
            and result.get('owned_job_final_member_pids') == []
            and result.get('owned_total_processes') == 1
            and not result.get('unexpected_third_process')
            and not result.get('cleanup_refusal'))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--execute-reviewed', action='store_true', required=True)
    parser.add_argument('--reviewed-runner-sha256', required=True)
    args = parser.parse_args()
    if os.name != 'nt':
        raise ValueError('reviewed Windows owned runner required')
    pins = {BINARY:BINARY_SHA, RETAINED/'airk-kernel.d':DEP_SHA,
            RETAINED/'ann-probe-compile.json':COMPILE_SHA, CAPTURE:CAPTURE_SHA,
            OWNER:OWNER_SHA, Path(__file__).resolve():args.reviewed_runner_sha256}
    pins.update({ROOT/name:pin for name,pin in SOURCES.items()})
    for path,pin in pins.items(): require_file(path,pin)
    compiled = json.loads((RETAINED/'ann-probe-compile.json').read_bytes())
    group = compiled['groups']['native-libraries']
    if compiled['exit_code'] != 0 or group['sha256'] != GROUP_SHA or any(group['files'].get(name) != pin for name,pin in SOURCES.items()):
        raise ValueError('retained actual R40 compilation/source binding differs')
    dep = (RETAINED/'airk-kernel.d').read_text().replace('\\','/')
    if any(name not in dep for name in SOURCES):
        raise ValueError('retained actual compiler did not consume diagnostic owners')
    if not DATA.is_dir(): raise ValueError('retained existing data root absent')
    owner_raw = OWNER.read_bytes()
    if hashlib.sha256(owner_raw).hexdigest() != OWNER_SHA: raise ValueError('captured owner changed')
    owner = types.ModuleType('_reviewed_ann_owned_process')
    owner.__file__ = str(OWNER)
    exec(compile(owner_raw,str(OWNER),'exec'),owner.__dict__)
    work = HERE/'execution-r1'
    if work.exists(): raise ValueError('fresh output required; never retry or overwrite')
    work.mkdir()
    stdin_path=work/'stdin.txt';stdin_path.write_bytes(b'')
    # Rust canonicalize on Windows preserves the extended-length prefix.
    canonical = lambda path: '\\\\?\\' + str(path.resolve())
    native_input=work/'native-input.json';native_result=work/'native-result.json'
    stages=[]
    input_sha=None
    for label,selector in [('emit',EMIT),('probe',PROBE)]:
        for path,pin in pins.items(): require_file(path,pin)
        environment=dict(os.environ)
        environment.update({'AIRK_ANN_SELF_CAPTURE':str(CAPTURE),
                            'AIRK_ANN_SELF_DATA_ROOT':canonical(DATA),
                            'AIRK_ANN_SELF_INPUT':canonical(native_input),
                            'AIRK_ANN_SELF_OUTPUT':canonical(native_result),
                            'OMP_NUM_THREADS':'1','MKL_NUM_THREADS':'1','OPENBLAS_NUM_THREADS':'1'})
        if input_sha is not None:
            require_file(native_input,input_sha)
            environment['AIRK_ANN_SELF_INPUT_SHA256']=input_sha
        argv=[str(BINARY),'--exact',selector,'--ignored','--nocapture','--test-threads=1']
        def suspended(api,job,info,deadline):
            members,active,total=owner.job_snapshot(api,job)
            if members != [int(info.pid)] or active != 1 or total != 1:
                raise ValueError('exact single suspended native image required')
            image=owner.image_proof(api,info.process,{'path':str(BINARY),'sha256':BINARY_SHA})
            if time.monotonic() >= deadline: raise ValueError('original stage deadline expired before resume')
            return {'members':members,'active':active,'total':total,'image':image}
        started=time.monotonic()
        try:
            with (work/(label+'.log')).open('xb') as log,stdin_path.open('rb') as stdin:
                owned=owner.run_windows_owned_training(argv,ROOT,log,60,environment,stdin,1073741824,before_resume=suspended)
            drift=[str(path) for path,pin in pins.items() if sha(path)!=pin]
            if input_sha is not None and sha(native_input)!=input_sha: drift.append('native typed input')
            stage={'stage':label,'argv':argv,'owned':owned,'elapsed_seconds':time.monotonic()-started,'post_input_drift':drift,
                   'limits':{'whole_seconds':60,'aggregate_host_bytes':1073741824,'max_job_slots':2,'expected_native_processes':1}}
            stages.append(stage)
            (work/(label+'-receipt.json')).write_text(json.dumps(stage,indent=2)+'\n')
            if not natural(owned) or drift: raise ValueError('stage failed; original evidence retained; no retry')
            if label=='emit':
                input_sha=sha(native_input)
            else:
                result=json.loads(native_result.read_bytes())
                if result.get('complete') is not True or result.get('input_sha256') != input_sha or result.get('binary_sha256') != BINARY_SHA or result['result']['embedding_actual_forwards'] != 0 or result['result']['embedding_inferences'] != 0:
                    raise ValueError('native diagnostic completion identity differs')
        except BaseException as error:
            failure={'complete':False,'stage':label,'exception_class':type(error).__name__,'stages':stages}
            cleanup=getattr(error,'owned_cleanup',None)
            if cleanup is not None: failure['owned_cleanup']=cleanup
            (work/'failure.json').write_text(json.dumps(failure,indent=2)+'\n')
            raise
    final={'schema':'airk.private-ann-self-execution/1','complete':True,'evaluation':True,'release_qualified':False,
           'retained_binary_sha256':BINARY_SHA,'retained_dep_info_sha256':DEP_SHA,'retained_compile_receipt_sha256':COMPILE_SHA,
           'native_source_group':GROUP_SHA,'native_input_sha256':input_sha,'native_result_sha256':sha(native_result),
           'input_pins':{str(path):pin for path,pin in pins.items()},'stages':stages,
           'scope':'Self-vector graph diagnosis only; no human-query recall, new NN, quality qualification or repair claim.'}
    (work/'execution.json').write_text(json.dumps(final,indent=2)+'\n')
    print(json.dumps({'complete':True,'native_input_sha256':input_sha,'native_result_sha256':final['native_result_sha256'],'receipt':str(work/'execution.json')}),flush=True)

if __name__=='__main__': main()
