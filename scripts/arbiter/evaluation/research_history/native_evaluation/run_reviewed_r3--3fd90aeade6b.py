# SPDX-License-Identifier: Apache-2.0
"""Root-only reviewed R43 zero-forward ANN self-vector diagnostic; never builds."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import time
import types

ROOT = Path(__file__).resolve().parents[4]
HERE = Path(__file__).resolve().parent
RETAINED = ROOT / 'artifacts/advanced-split/native-harnesses/context-integrated-r43'
BINARY = RETAINED / 'airk-kernel.exe'
BINARY_SHA = 'cdb2e9d4008d315b4bbe791367c6cc61e172cfe9357b2d4225565760b5864e48'
DEP_SHA = '5f6352c61c71b0792099e6018bbd2bac90b784d32b6de7582bb6531856c4cdcc'
COMPILE_SHA = '8bddab221201e25ef7ef4e37d9fdb7745116130bdbd94270730a6da63a8fac66'
TRANSITIVE_PINS = {
    'ann-transitive-retention-r3.json':'ae16cad60beac03a42e6aa453ec4a2f95cf6702a864774c4345afc0adae6eda0',
    'airk-retrieval-r43.rlib':'67604f7e1f9a279d8ca6a2f6022f18d497ba40c2b8963b69ee03dc7137616cdb',
    'airk-retrieval-r43.rmeta':'71420b4729035d3f349d81c05e589baf12f603103b33b0bbf8c7d097853ef1f1',
    'airk-retrieval-r43.d':'010c5fda98cfe0027b72c91be42942b8b0d8b3088845f8728cc39448c1bc9a86',
    'ann-probe-compile.log':'f58418a973418ced5b4bbc3b4fe2df77feb34050389a495a039bb3622a23ced0',
}
GROUP_SHA = '159bfdec0ceb8e05b7b4c347a6f0e53e50e6bbc32d49c10ed78ba0d1292879d9'
OWNER = ROOT / 'tools/retrieval-train/training_execution.py'
OWNER_SHA = 'd363913b2424321822f9eee189f999eabb807a43c7d0c46fab567531dd1bd975'
CAPTURE = ROOT / 'artifacts/advanced-split/models/candidate-native-r20-r1/visible-prefix-mining-r1-u8/capture.json'
CAPTURE_SHA = '9aec2ccc46b1d278db426da62183078702496879dd826442f5b3fb8f999caeb9'
DATA = ROOT / 'artifacts/advanced-split/models/candidate-native-r20-r1/visible-prefix-mining-r1-u8/data/visible-prefix-mining-r1-u8/index-0'
SOURCES = {
    'crates/airk-kernel/src/inference_owner/evaluation_tests/ann_self.rs': 'bc51700e0d5fbf782742fc6c2b98d56c34159d3044a37d678a0d151aeca88398',
    'crates/airk-retrieval/src/vector/mutable/self_probe.rs': 'b17096bc35d51a0a302fe0de107b2912eda4a25ad87719b7021bbb2189d4b5ac',
}
PREFIX = 'inference_owner::evaluation_tests::ann_self::'
EMIT = PREFIX + 'emit_retained_dev_graph_self_probe_typed_input'
PROBE = PREFIX + 'retained_dev_graph_self_vectors_compare_actual_ann_to_exact_all_live_ids'
SMOKE = PREFIX + 'retained_dev_graph_first_live_reader_query_is_observed_without_new_inference'
NATIVE_INPUT = HERE/'execution-r2/native-input.json'
NATIVE_INPUT_SHA = '850afafe8286a626c47709449da86e12e77052e06f5a65c710ee9b4bad23af19'

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

def verify_compile_owners():
    # rustc dep-info covers ONLY the owning crate, not transitive extern sources.
    for dep,name in [('airk-kernel.d','crates/airk-kernel/src/inference_owner/evaluation_tests/ann_self.rs'),
                     ('airk-retrieval-r43.d','crates/airk-retrieval/src/vector/mutable/self_probe.rs')]:
        if name not in (RETAINED/dep).read_text().replace('\\','/'):
            raise ValueError('owning crate compiler did not consume diagnostic source')
    records=[json.loads(line) for line in (RETAINED/'ann-probe-compile.log').read_text(encoding='utf-8').splitlines() if line.startswith('{')]
    dependencies=[row for row in records if row.get('reason')=='compiler-artifact' and row.get('target',{}).get('name')=='airk_retrieval']
    kernels=[row for row in records if row.get('reason')=='compiler-artifact' and row.get('target',{}).get('name')=='airk_kernel' and row.get('executable')]
    if len(dependencies)!=1 or len(kernels)!=1:
        raise ValueError('actual same-command compiler artifact identity ambiguous')
    dependency=dependencies[0]
    if dependency['features']!=['local-inference','test-support'] or sorted(Path(name).name for name in dependency['filenames'])!=['libairk_retrieval-d4713b2dc80ddac7.rlib','libairk_retrieval-d4713b2dc80ddac7.rmeta']:
        raise ValueError('actual R43 Retrieval artifact/feature graph differs')
    if Path(kernels[0]['executable']).name!='airk_kernel-57b62dce9e600d19.exe' or kernels[0]['profile']['test'] is not True:
        raise ValueError('actual R43 Kernel test artifact differs')

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--execute-reviewed', action='store_true', required=True)
    parser.add_argument('--reviewed-runner-sha256', required=True)
    parser.add_argument('--mode', choices=['smoke','full'], required=True)
    args = parser.parse_args()
    if os.name != 'nt':
        raise ValueError('reviewed Windows owned runner required')
    pins = {BINARY:BINARY_SHA, RETAINED/'airk-kernel.d':DEP_SHA,
            RETAINED/'ann-probe-compile.json':COMPILE_SHA, CAPTURE:CAPTURE_SHA,
            OWNER:OWNER_SHA, NATIVE_INPUT:NATIVE_INPUT_SHA, Path(__file__).resolve():args.reviewed_runner_sha256}
    pins.update({ROOT/name:pin for name,pin in SOURCES.items()})
    pins.update({RETAINED/name:pin for name,pin in TRANSITIVE_PINS.items()})
    for path,pin in pins.items(): require_file(path,pin)
    compiled = json.loads((RETAINED/'ann-probe-compile.json').read_bytes())
    group = compiled['groups']['native-libraries']
    if compiled['exit_code'] != 0 or group['sha256'] != GROUP_SHA or any(group['files'].get(name) != pin for name,pin in SOURCES.items()):
        raise ValueError('retained actual R43 compilation/source binding differs')
    verify_compile_owners()
    if not DATA.is_dir(): raise ValueError('retained existing data root absent')
    owner_raw = OWNER.read_bytes()
    if hashlib.sha256(owner_raw).hexdigest() != OWNER_SHA: raise ValueError('captured owner changed')
    owner = types.ModuleType('_reviewed_ann_owned_process')
    owner.__file__ = str(OWNER)
    exec(compile(owner_raw,str(OWNER),'exec'),owner.__dict__)
    work = HERE/('execution-r3-'+args.mode)
    if work.exists(): raise ValueError('fresh output required; never retry or overwrite')
    work.mkdir()
    stdin_path=work/'stdin.txt';stdin_path.write_bytes(b'')
    # Rust canonicalize on Windows preserves the extended-length prefix.
    canonical = lambda path: '\\\\?\\' + str(path.resolve())
    native_input=NATIVE_INPUT;native_result=work/'native-result.json'
    stages=[]
    input_sha=NATIVE_INPUT_SHA
    timeout=5 if args.mode=='smoke' else 60
    for label,selector in [(args.mode,SMOKE if args.mode=='smoke' else PROBE)]:
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
                owned=owner.run_windows_owned_training(argv,ROOT,log,timeout,environment,stdin,1073741824,before_resume=suspended)
            drift=[str(path) for path,pin in pins.items() if sha(path)!=pin]
            if input_sha is not None and sha(native_input)!=input_sha: drift.append('native typed input')
            stage={'stage':label,'argv':argv,'owned':owned,'elapsed_seconds':time.monotonic()-started,'post_input_drift':drift,
                   'limits':{'whole_seconds':timeout,'aggregate_host_bytes':1073741824,'max_job_slots':2,'expected_native_processes':1}}
            stages.append(stage)
            (work/(label+'-receipt.json')).write_text(json.dumps(stage,indent=2)+'\n')
            if not natural(owned) or drift: raise ValueError('stage failed; original evidence retained; no retry')
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
