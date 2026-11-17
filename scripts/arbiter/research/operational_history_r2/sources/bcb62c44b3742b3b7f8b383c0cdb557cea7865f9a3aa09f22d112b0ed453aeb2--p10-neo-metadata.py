# SPDX-License-Identifier: Apache-2.0
"""Explicit adapter for the targeted P10 metadata correction Neo batch.

preview writes only an inert plan; prepare freezes public source locally.
Only explicit launch/collect delegate to the already reviewed SSH helper.
"""
import argparse
import ast
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

BASE = Path(__file__).resolve().parent
LABEL = 'p10-native-20261106-r3'
EXPECTED = {'sha256': 'c713b469d7d4a646a9ceab52aa7ef28ecd2b1af19a787312d2b0b9e843d4321b', 'files': 945}

def read(path):
    return json.loads(path.read_text(encoding='utf-8', errors='strict'))

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def retain_json(path, value):
    content=(json.dumps(value, indent=2)+'\n').encode('utf-8', errors='strict')
    if path.exists():
        assert path.read_bytes()==content, 'Different immutable evidence: '+str(path)
    else:
        with path.open('xb') as output: output.write(content)

def plan():
    receipt=read(BASE/'p10-scoped-metadata-full-module-r1.receipt.json')
    assert receipt['source_before']==receipt['source_after']==EXPECTED
    assert receipt['source_unchanged'] and receipt['actual_exit_code']==0 and receipt['passing_tests']==10
    prior=read(BASE/'p10-neo-r2-primary-verification.json')
    assert prior['native_source']['sha256'].startswith('a7458f86')
    assert prior['steps']==16 and prior['passing_tests']==66 and prior['cleanup']['target_removed']
    value=read(BASE/'p10-native-20261105-r2-plan.json')
    value.update(phase='P10', draft=False, pending=[], steps=[
        {'name':'fmt','command':['cargo','fmt','--all','--check']},
        {'name':'kernel-scoped-metadata-lightweight','expected_passing_tests':10,
         'command':['cargo','test','-p','airk-kernel','--lib','--locked','--offline','--timings',
                    '--no-default-features','scoped_reads::tests::','--','--test-threads=1']},
        {'name':'kernel-scoped-metadata-helpers','expected_passing_tests':10,
         'command':['cargo','test','-p','airk-kernel','--lib','--locked','--offline','--timings',
                    '--all-features','scoped_reads::tests::','--','--test-threads=1']}
    ], correction_source=EXPECTED,
       prior_qualification={'path':'p10-neo-r2-primary-verification.json','native_source':prior['native_source'],
                            'steps':16,'passing_tests':66,'target_removed':True,
                            'scope':'Historical broad16-step evidence only; do not relabel as c713 source.'},
       not_run=['Unchanged broad16-step Mac batch is not repeated; historical a745/945 evidence remains separately identified.',
                'This targeted source proves only formatting and Kernel scoped metadata tests in minimal/all-feature modes.',
                'Full Mac workspace/runtime/performance, native CLI transport suffix, editor, signing, Intel/Linux and publication are not this batch.',
                'Windows HANDLE-specific tests remain separately qualified on Windows.'])
    value['environment'].update(CARGO_BUILD_JOBS='1', CARGO_NET_OFFLINE='true', CARGO_INCREMENTAL='0',
                                TMPDIR='Canonical existing macOS default temporary directory, resolved strictly by runner',
                                RLIMIT_NOFILE=4096)
    assert '--fresh' in value['runner_arguments'] and '--cache' not in value['runner_arguments']
    assert len(value['steps'])==3
    return value

def check_prepared(label):
    prepared=read(BASE/(label+'-metadata-prepared.json'))
    assert prepared['native_source']==EXPECTED
    assert digest(BASE/(label+'-plan.json'))==prepared['plan_sha256']
    assert read(BASE/(label+'-plan.json'))==plan()
    assert digest(BASE/(label+'-runner.py'))==prepared['runner_sha256']
    assert digest(BASE/(label+'.tar.gz'))==prepared['archive_sha256']
    assert read(BASE/(label+'-snapshot.json'))['native_source']==EXPECTED
    return prepared

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['preview','prepare','launch','collect'])
    parser.add_argument('label', nargs='?', default=LABEL)
    args=parser.parse_args()
    assert args.label==LABEL and re.fullmatch(r'p10-native-[a-z0-9-]+',args.label)
    value=plan()
    if args.action=='preview':
        retain_json(BASE/(args.label+'-metadata-preview.json'),value)
        print(json.dumps({'prepared_only':False,'preview_only':True,'archive_created':False,
                          'label':args.label,'steps':3,'expected_passing_tests':20,'native_source':EXPECTED}))
    elif args.action=='prepare':
        # Root runs this only after final source gates. A correction needs a new
        # reviewed source/label; never silently freeze changed production bytes.
        for suffix in ['.tar.gz','-snapshot.json','-plan.json','-runner.py','-metadata-prepared.json','-active.json']:
            assert not (BASE/(args.label+suffix)).exists(), 'Fresh label artifact already exists: '+suffix
        import importlib.util
        source=BASE/'p10-worktree'
        spec=importlib.util.spec_from_file_location('metadata_builder',source/'scripts/build-service-runtime.py')
        builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
        assert builder.source_identity()==EXPECTED, 'Source changed after reviewed metadata plan; review a new source/label'
        subprocess.run([sys.executable,str(BASE/'p10-neo-prepare.py'),args.label],check=True)
        snapshot=read(BASE/(args.label+'-snapshot.json'))
        assert snapshot['native_source']==EXPECTED and builder.source_identity()==EXPECTED
        # No process can launch during this local preparation; replace the
        # inherited broad plan before producing the required prepared receipt.
        (BASE/(args.label+'-plan.json')).write_text(json.dumps(value,indent=2)+'\n',encoding='utf-8',errors='strict')
        runner_path=BASE/(args.label+'-runner.py')
        runner=runner_path.read_text(encoding='utf-8',errors='strict')
        anchor="        steps.append({'actual_exit_code': actual_exit_code, 'passing_tests': passing_tests"
        assert runner.count(anchor)==1
        guard="""        expected_passing_tests = row.get('expected_passing_tests')
        if result.returncode == 0 and expected_passing_tests is not None and passing_tests != expected_passing_tests:
            validation_error = f'Expected {expected_passing_tests} passing tests, observed {passing_tests}'
            result.returncode = 1
"""
        runner=runner.replace(anchor,guard+anchor)
        ast.parse(runner)
        assert "environment['TMPDIR'] = str(Path(tempfile.gettempdir()).resolve(strict=True))" in runner
        assert "CARGO_BUILD_JOBS='1'" in runner and "CARGO_INCREMENTAL='0'" in runner
        assert "resource.setrlimit(resource.RLIMIT_NOFILE, (4096, hard))" in runner
        runner_path.write_text(runner,encoding='utf-8',errors='strict')
        retain_json(BASE/(args.label+'-metadata-prepared.json'),{
            'native_source':EXPECTED,'steps':3,'expected_passing_tests':20,
            'plan_sha256':digest(BASE/(args.label+'-plan.json')),'runner_sha256':digest(runner_path),
            'archive_sha256':snapshot['archive_sha256'],
            'owner':'p10-neo-owner.py, unconditional finally cargo clean + exact target absence verification; timings retained first',
            'status':'local source frozen and targeted plan prepared; no SSH or tests launched'})
        check_prepared(args.label)
        print('prepared',args.label)
    else:
        check_prepared(args.label)
        subprocess.run([sys.executable,str(BASE/'p10-neo-launch.py'),args.action,args.label],check=True)
        if args.action=='collect':
            qualification=read(BASE/(args.label+'-qualification.json'))
            steps=read(BASE/(args.label+'-steps.json'))
            cleanup=read(BASE/(args.label+'-cleanup.json'))
            assert qualification['native_source']==EXPECTED and qualification['status']=='passed'
            assert [row['name'] for row in steps]==[row['name'] for row in value['steps']]
            assert all(row['actual_exit_code']==row['exit_code']==0 for row in steps)
            assert [row['passing_tests'] for row in steps]==[None,10,10]
            assert cleanup['cargo_clean_exit_code']==0 and cleanup['target_removed'] and cleanup['source_preserved']
            assert cleanup['timing_copy_error'] is None
            retain_json(BASE/(args.label+'-metadata-verification.json'),{
                'status':'passed_targeted_mac_metadata_correction','native_source':EXPECTED,'steps':3,'passing_tests':20,
                'qualification_sha256':digest(BASE/(args.label+'-qualification.json')),
                'cleanup_sha256':digest(BASE/(args.label+'-cleanup.json')),
                'timings_sha256':digest(BASE/(args.label+'-timings.json')),
                'prior_broad_evidence':'a7458f86/945,16steps/66tests; not relabeled as current source'})

if __name__=='__main__':
    main()
