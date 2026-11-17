# SPDX-License-Identifier: Apache-2.0
"""Additive R3: validate retained R2 passes, run ONLY static Help and final seals."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

sys.dont_write_bytecode=True
HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('executed_help_r2_owner',HERE/'resume_final_help.py')
r2owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(r2owner)
original=r2owner.original;ROOT=original.ROOT
R2=ROOT/'artifacts/advanced-split/receipts/final-help-resume-r2'
AUDIT=HERE/'help-cardinality-audit.json'

def reuse():
    plan=json.loads((R2/'plan.json').read_bytes());results=json.loads((R2/'results.json').read_bytes())
    assert [row['name'] for row in results]==['admin-build','admin-check','admin-seal','shared-help-tests']
    expected=dict(plan['source_files']);pins={name:original.digest(R2/name) for name in ['plan.json','results.json','admin-assets.json']}
    for row in results:
        assert row['exit_code']==0 and not row['unexpected_changes'] and row['usage_input_unchanged']
        assert not row['compiled_out_changed'] and row['skipped_tests']==0
        for name,change in row['changed_identities'].items():
            assert expected.get(name)==change['before']
            if change['after'] is None:expected.pop(name,None)
            else:expected[name]=change['after']
        for suffix in ['.json','.log']:pins[row['name']+suffix]=original.digest(R2/(row['name']+suffix))
    shared=results[-1]
    assert shared['selected_tests']==20 and shared['expected_selected']==21
    audit=json.loads(AUDIT.read_bytes())
    assert audit['authored_count']==20 and audit['all_authored_names_passed_in_r2'] and audit['exact_r1_r2_command_same']
    assert not audit['filters_or_subset'] and audit['r2_skipped']==0 and audit['r2_failures']==0
    assert audit['evidence']['r2_log_sha256']==pins['shared-help-tests.log']
    assert audit['evidence']['r2_receipt_sha256']==pins['shared-help-tests.json']
    assert sum(row['count'] for row in audit['files'])==20 and len(audit['files'])==7
    for row in audit['files']:assert original.digest(ROOT/row['file'])==row['sha256']
    current=r2owner.source_inputs()
    if current!=expected:
        changed=sorted(name for name in current.keys()|expected.keys() if current.get(name)!=expected.get(name))
        raise ValueError('Source differs from actual R2 passed inputs: '+repr(changed))
    out=original.inventory(ROOT/'packages/vscode/out')
    assert out==plan['reuse']['current_compiled_out']
    return {'r2_receipt_pins':pins,'source_files_after_passed_r2_stages':expected,'compiled_out':out,
        'admin_assets':original.inventory(ROOT/'crates/airk-cli/admin-assets'),
        'cardinality_audit_sha256':original.digest(AUDIT),'cardinality_diagnosis':audit['diagnosis'],
        'original_failed_expectation_preserved':True,'r1_reuse_identity':plan['reuse'],
        'shared_help_status':'actual authored20/20 passed; R2 helper failed only its incorrect21count assertion'}

def inputs():
    result=r2owner.source_inputs()
    result['private:r3-helper']=original.digest(Path(__file__))
    result['private:authored-cardinality-audit']=original.digest(AUDIT)
    result['private:authored-cardinality-audit-source']=original.digest(HERE/'audit_help_cardinality.cjs')
    return result

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--execute-reviewed',action='store_true')
    parser.add_argument('--expected-source-sha256')
    parser.add_argument('--out',type=Path,default=ROOT/'artifacts/advanced-split/receipts/final-help-complete-r3')
    args=parser.parse_args();out=args.out.resolve()
    if not out.is_relative_to(ROOT/'artifacts') or out==ROOT/'artifacts':raise ValueError('Private fresh output required')
    retained=reuse();before=inputs()
    extra={'private:r3-helper','private:authored-cardinality-audit','private:authored-cardinality-audit-source'}
    if {key:value for key,value in before.items() if key not in extra}!=retained['source_files_after_passed_r2_stages']:
        raise ValueError('Source changed while freezing R3 reuse proof')
    command=[sys.executable,'-B','-m','unittest','scripts.tests.test_help_site','-v']
    plan={'schema':'airk.private-final-help-completion-plan/1','source_files':before,'reused':retained,
        'remaining_command':command,'out':str(out),'normal_windows_sid':r2owner.NORMAL_SID,
        'native_admin_node_compile_passes_reexecuted':False}
    plan['source_sha256']=original.seal(plan)
    if not args.execute_reviewed:
        print(json.dumps(plan,indent=2));return 0
    if args.expected_source_sha256!=plan['source_sha256']:raise ValueError('Reviewed R3 source/evidence seal differs')
    if r2owner.normal_identity()!=r2owner.NORMAL_SID:raise ValueError('Reviewed normal identity required')
    if out.exists():raise ValueError('Fresh additive receipt directory required')
    out.mkdir(parents=True);r2owner.write(out/'plan.json',plan)
    private=out/'private-temp';private.mkdir()
    env={key:value for key,value in os.environ.items() if not key.startswith('AIRK_WRITE_')}
    env.update(TEMP=str(private),TMP=str(private),TMPDIR=str(private),PYTHONIOENCODING='utf-8')
    if inputs()!=before:
        r2owner.write(out/'refusal.json',{'reason':'source changed before static tests','command_executed':False});return 3
    started=time.monotonic()
    with (out/'static-help-tests.log').open('x',encoding='utf-8') as log:
        child=subprocess.run(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
    final=inputs();text=(out/'static-help-tests.log').read_text(encoding='utf-8',errors='replace')
    count=original.owner.selected_tests(text)
    skips=sum(int(value) for value in re.findall(r'OK \(skipped=(\d+)\)',text))
    receipt={'command':command,'exit_code':child.returncode,'elapsed_seconds':time.monotonic()-started,
        'selected_tests':count,'skipped_tests':skips,'source_unchanged':before==final,
        'compiled_out_unchanged':original.inventory(ROOT/'packages/vscode/out')==retained['compiled_out'],
        'admin_assets_unchanged':original.inventory(ROOT/'crates/airk-cli/admin-assets')==retained['admin_assets']}
    r2owner.write(out/'static-help-tests.json',receipt);print(json.dumps(receipt),flush=True)
    if child.returncode or not count or skips or not all(receipt[key] for key in ['source_unchanged','compiled_out_unchanged','admin_assets_unchanged']):
        return child.returncode or 3
    if inputs()!=final:
        r2owner.write(out/'refusal.json',{'reason':'source changed before final seal'});return 3
    r2owner.write(out/'result.json',{'schema':'airk.private-final-help-completion-result/1','result':'help_generation_and_exercised_consumers_complete',
        'reused_evidence':retained,'static_help_receipt':receipt,'final_source_files':final,'final_source_sha256':original.seal(final),
        'extension_out':retained['compiled_out'],'admin_assets':retained['admin_assets'],
        'original_failures_and_wrong_count_expectation_preserved':True,
        'qualified_scope':'original R1 native/compile passes; R2 Admin/build/seal and all20authoredHelp cases; R3 static Help/final hashes. Not installed/platform/model/publication qualification.'})
    return 0

if __name__=='__main__':raise SystemExit(main())
