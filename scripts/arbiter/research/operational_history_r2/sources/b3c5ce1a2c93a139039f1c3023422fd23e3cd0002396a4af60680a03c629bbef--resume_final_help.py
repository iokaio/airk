# SPDX-License-Identifier: Apache-2.0
"""Additive Root-only Help R1 resume. Default plans; never reruns Cargo/compile."""
import argparse
import csv
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

sys.dont_write_bytecode=True
HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('frozen_r1_help_owner',HERE/'run_final_help.py')
original=importlib.util.module_from_spec(spec);spec.loader.exec_module(original)
ROOT=original.ROOT
R1=ROOT/'artifacts/advanced-split/receipts/final-help-r1'
CONTROL=ROOT/'artifacts/advanced-split/receipts/browser-normal-control-r1/result.log'
NORMAL_SID='S-1-5-21-3678370870-3858642417-826968105-1002'
CORE_SHA='01ef1ac60b206135880d540ca464234a148f192398e8be81ba41c3631598cbc4'
MOUNT=ROOT/'artifacts/advanced-split/receipts/source-mount-r94/mount.json'
UNCONSUMED_TEST_CONTRACT_PATHS={
    'crates/airk-kernel/src/inference_owner/evaluation_tests/native_parent_bag.rs',
    'crates/airk-kernel/src/inference_owner/evaluation_tests/development.rs',
    'crates/airk-kernel/src/inference_owner/evaluation_tests/CONTRACT.md',
    'crates/airk-service/src/service_runtime/evaluation_fixture_tests.rs',
    'crates/airk-service/src/service_runtime/CONTRACT.md',
}
PASS_NAMES=['scoped-clippy','cli-catalog','cli-reference','native-content','native-help-cases',
    'i2-cases','i3-cases','native-help-drift','npm-ci-vscode','npm-ci-admin','metadata-codec',
    'host-typescript','shared-help-manifest','extension-compile','extension-help-tests']

def browser_control():
    records=[json.loads(line) for line in CONTROL.read_text(encoding='utf-8').splitlines() if line.strip()]
    opened=next(row for row in records if row['stage']=='actual_browser_started')
    closed=next(row for row in records if row['stage']=='actual_browser_closed')
    assert opened['value']==2 and closed['observation']['browser_close_acknowledged']
    assert closed['observation']['exit_observed'] and closed['observation']['exit_code']==0
    assert closed['observation']['profile_removed']
    executable=Path(opened['observation']['executable'])
    assert executable.is_file()
    return {'receipt_sha256':original.digest(CONTROL),'executable':str(executable),
            'executable_sha256':original.digest(executable)}

def reuse_evidence():
    assert original.digest(HERE/'run_final_help.py')==CORE_SHA,'Original executed helper changed'
    plan=json.loads((R1/'plan.json').read_bytes())
    results=json.loads((R1/'results.json').read_bytes())
    assert [step['name'] for step in results[:-1]]==PASS_NAMES
    expected=dict(plan['source_files']); pins={'plan.json':original.digest(R1/'plan.json'),
        'results.json':original.digest(R1/'results.json')}
    for step in results[:-1]:
        assert step['exit_code']==0 and not step['unexpected_changes'] and step['usage_input_unchanged']
        if step['write_variable']:assert step['selected_tests']==1
        if step['name']=='native-help-drift':assert step['selected_tests']==20
        if step['name']=='extension-help-tests':assert step['selected_tests']==3
        for name,row in step['changed_identities'].items():
            assert expected.get(name)==row['before']
            if row['after'] is None:expected.pop(name,None)
            else:expected[name]=row['after']
        for suffix in ('.json','.log'):
            pins[step['name']+suffix]=original.digest(R1/(step['name']+suffix))
    failure=results[-1]
    assert failure['name']=='shared-help-tests' and failure['exit_code']!=0
    assert not failure['changed_inputs'] and failure['selected_tests']==21
    for name in ['shared-help-tests.json','shared-help-tests.log','failed-browser-owned-tree.json']:
        pins[name]=original.digest(R1/name)
    current=original.inputs()
    def protected(name):
        return name.startswith(('packages/','crates/airk-core/','crates/airkctl/',
            'crates/airk-mcp/','crates/airk-https/','fixtures/split/ui_pilot/','private:')) or name=='docs/cli-reference.md'
    reused={name:value for name,value in expected.items() if protected(name)}
    differences=[name for name in reused if current.get(name)!=reused[name]]
    differences += [name for name in current if protected(name) and name not in expected]
    if differences:raise ValueError('Passed R1 Help/consumer input changed: '+repr(sorted(set(differences))))
    delta={name:{'r1':expected.get(name),'current':current.get(name)} for name in expected.keys()|current.keys()
           if expected.get(name)!=current.get(name)}
    if set(delta)-UNCONSUMED_TEST_CONTRACT_PATHS:
        raise ValueError('Unreviewed non-Help source delta: '+repr(sorted(set(delta)-UNCONSUMED_TEST_CONTRACT_PATHS)))
    mount=json.loads(MOUNT.read_bytes())
    for row in mount['files']:
        name=row['path'].replace('\\','/')
        assert name in UNCONSUMED_TEST_CONTRACT_PATHS and expected[name]==row['before']
        if name in delta:delta[name]['r94_mounted_after']=row['after']
    assert original.digest(ROOT/'packages/vscode/out/help-manifest.json')==reused['packages/ui/generated/help-manifest.json']
    # R1 stopped before its final out inventory. Freeze current compiled bytes;
    # retain the R1 compile pass under its own original source identity.
    return {'r1_receipt_pins':pins,'reused_help_input_hashes':reused,
        'known_unconsumed_test_contract_delta':delta,
        'approved_delta_reference':{'path':str(MOUNT),'sha256':original.digest(MOUNT)},
        'delta_authority':'Root reviewed R94 three test-only mounts and owning contract prose; additional same Service synthetic test append must be reviewed/mounted before the new plan freeze',
        'r1_native_pass_not_reinterpreted_for_changed_kernel_service_sources':True,
        'current_compiled_out':original.inventory(ROOT/'packages/vscode/out'),
        'compiled_out_scope':'current byte hashes frozen for resume; R1 compile pass retains its original input identity',
        'shared_test_command':failure['command'],'r1_failed_outcome_retained':True}

def source_inputs():
    value=original.inputs()
    value['private:resume-helper']=original.digest(Path(__file__))
    value['private:browser-normal-control']=original.digest(CONTROL)
    value['private:browser-executable']=browser_control()['executable_sha256']
    value['private:approved-source-mount-r94']=original.digest(MOUNT)
    return value

def stages(out,reused):
    node=original.NODE;npm=original.NPM
    return [
        {'name':'admin-build','command':list(map(str,[node,npm,'run','build','--prefix','packages/admin'])),
         'allowed_changes':[r'crates/airk-cli/admin-assets/(?:app\.[a-f0-9]{8}\.(?:js|css)|manifest\.json|index\.html|launch\.html)'],'expected_selected':None},
        {'name':'admin-check','command':list(map(str,[node,npm,'run','check','--prefix','packages/admin'])),'allowed_changes':[],'expected_selected':None},
        {'name':'admin-seal','command':[sys.executable,'scripts/build-admin-assets.py','--skip-install','--output',str(out/'admin-assets.json')],'allowed_changes':[],'expected_selected':None},
        {'name':'shared-help-tests','command':reused['shared_test_command'],'allowed_changes':[],'expected_selected':21},
        {'name':'static-help-tests','command':[sys.executable,'-m','unittest','scripts.tests.test_help_site','-v'],'allowed_changes':[],'expected_selected':'nonzero'},
    ]

def normal_identity():
    result=subprocess.run(['whoami','/user','/fo','csv','/nh'],capture_output=True,text=True,check=True)
    return next(csv.reader(io.StringIO(result.stdout)))[-1]

def write(path,value):
    path.write_text(json.dumps(value,indent=2)+'\n',encoding='utf-8')

def failed_step(receipt):
    return bool(receipt['exit_code'] or receipt['unexpected_changes'] or not receipt['usage_input_unchanged']
        or receipt['compiled_out_changed'] or receipt['skipped_tests']
        or receipt['expected_selected']=='nonzero' and not receipt['selected_tests']
        or isinstance(receipt['expected_selected'],int) and receipt['selected_tests']!=receipt['expected_selected'])

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--execute-reviewed',action='store_true')
    parser.add_argument('--expected-source-sha256')
    parser.add_argument('--out',type=Path,default=ROOT/'artifacts/advanced-split/receipts/final-help-resume-r2')
    args=parser.parse_args(); original.versions()
    out=args.out.resolve()
    if not out.is_relative_to(ROOT/'artifacts') or out==ROOT/'artifacts':raise ValueError('Private fresh output required')
    reused=reuse_evidence(); browser=browser_control(); before=source_inputs()
    plan={'schema':'airk.private-final-help-resume-plan/1','source_files':before,'reuse':reused,
        'browser':browser,'required_windows_sid':NORMAL_SID,'steps':stages(out,reused),'out':str(out),
        'no_cargo_native_generation_npm_ci_or_compile_stages':True}
    plan['source_sha256']=original.seal(plan)
    if not args.execute_reviewed:
        print(json.dumps(plan,indent=2));return 0
    if args.expected_source_sha256!=plan['source_sha256']:raise ValueError('Reviewed resume source/evidence seal differs')
    if normal_identity()!=NORMAL_SID:raise ValueError('Resume requires the reviewed normal human account; do not rerun under Sandbox1012')
    if out.exists():raise ValueError('Fresh additive output required; preserve original failures')
    out.mkdir(parents=True);write(out/'plan.json',plan)
    private_temp=out/'private-temp';private_temp.mkdir()
    cache=ROOT/'artifacts/advanced-split/npm-cache'
    env={key:value for key,value in os.environ.items() if not key.startswith('AIRK_WRITE_')
         and key.upper() not in ['NPM_CONFIG_CACHE','NPM_CONFIG_LOGS_DIR','NPM_CONFIG_UPDATE_NOTIFIER','AIRK_BROWSER']}
    env.update(PATH=os.pathsep.join([str(original.NODE.parent),env.get('PATH','')]),
        NPM_CONFIG_CACHE=str(cache),NPM_CONFIG_LOGS_DIR=str(cache/'_logs'),NPM_CONFIG_UPDATE_NOTIFIER='false',
        AIRK_BROWSER=browser['executable'],TEMP=str(private_temp),TMP=str(private_temp),TMPDIR=str(private_temp),PYTHONIOENCODING='utf-8')
    usage_sha=original.digest(ROOT/'fixtures/split/ui_pilot/usage_native.json')
    expected=before;results=[]
    for step in plan['steps']:
        initial=source_inputs()
        if initial!=expected or original.inventory(ROOT/'packages/vscode/out')!=reused['current_compiled_out']:
            refusal={'name':step['name'],'result':'refused_source_or_compiled_artifact_drift_before_command','command_executed':False}
            results.append(refusal)
            write(out/(step['name']+'.json'),refusal);write(out/'refusal.json',refusal);write(out/'results.json',results);return 3
        started=time.monotonic()
        with (out/(step['name']+'.log')).open('x',encoding='utf-8') as log:
            child=subprocess.run(step['command'],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
        final=source_inputs();changed=sorted(name for name in initial.keys()|final.keys() if initial.get(name)!=final.get(name))
        unexpected=[name for name in changed if not any(re.fullmatch(pattern,name) for pattern in step['allowed_changes'])]
        log=(out/(step['name']+'.log')).read_text(encoding='utf-8',errors='replace')
        skipped=[int(value) for value in re.findall(r'(?:ℹ|#)\s*skipped\s+(\d+)',log)]
        skipped += [int(value) for value in re.findall(r'OK \(skipped=(\d+)\)',log)]
        receipt=dict(step,exit_code=child.returncode,elapsed_seconds=time.monotonic()-started,
            selected_tests=original.owner.selected_tests(log),skipped_tests=sum(skipped),
            input_sha256=original.seal(initial),output_sha256=original.seal(final),unexpected_changes=unexpected,
            changed_identities={name:{'before':initial.get(name),'after':final.get(name)} for name in changed},
            usage_input_unchanged=original.digest(ROOT/'fixtures/split/ui_pilot/usage_native.json')==usage_sha,
            compiled_out_changed=original.inventory(ROOT/'packages/vscode/out')!=reused['current_compiled_out'],
            normal_windows_sid=NORMAL_SID)
        for name in changed:
            source=ROOT/name
            if name not in unexpected and source.is_file():
                saved=out/'generated'/step['name']/name;saved.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,saved)
        results.append(receipt);write(out/(step['name']+'.json'),receipt);write(out/'results.json',results)
        print(json.dumps({key:receipt[key] for key in ['name','exit_code','selected_tests','skipped_tests','unexpected_changes']}),flush=True)
        if failed_step(receipt):return child.returncode or 3
        expected=final
    final=source_inputs()
    if final!=expected:write(out/'final-source-refusal.json',{'result':'source_drift_before_seal'});return 3
    write(out/'result.json',{'schema':'airk.private-final-help-resume-result/1','result':'remaining_help_checks_passed',
        'original_r1_receipts':reused['r1_receipt_pins'],'original_failed_outcome_preserved':True,
        'known_unconsumed_test_contract_delta':reused['known_unconsumed_test_contract_delta'],
        'approved_delta_reference':reused['approved_delta_reference'],
        'resume_source_sha256':plan['source_sha256'],'final_source_sha256':original.seal(final),'final_source_files':final,
        'steps':results,'extension_out':reused['current_compiled_out'],'admin_assets':original.inventory(ROOT/'crates/airk-cli/admin-assets'),
        'usage_native_sha256':usage_sha,'not_installed_platform_or_publication_qualification':True})
    return 0

if __name__=='__main__':raise SystemExit(main())
