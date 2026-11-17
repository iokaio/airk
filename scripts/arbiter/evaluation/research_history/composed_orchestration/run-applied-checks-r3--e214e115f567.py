# SPDX-License-Identifier: Apache-2.0
"""Exact consolidated private proposal controls; no Cargo/service/model invocation."""
from pathlib import Path
import subprocess
import sys
import os
import time
import json
import hashlib
import importlib.util
B=Path(__file__).resolve().parent
P=next(p for p in B.parents if (p/'Cargo.toml').exists())
OUT=B/'applied-cheap-r3'
OUT.mkdir(exist_ok=False)
ENV={**os.environ,'PYTHONDONTWRITEBYTECODE':'1','TEMP':r'C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\temp',
     'TMP':r'C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\temp',
     'TMPDIR':r'C:\Users\tdotn\AppData\Local\airk-final-exec-20261116T052942Z\temp'}
SELECTORS=[
 'ServiceClientTests.test_split_pair_classification_and_images',
 'ServiceClientTests.test_split_fixture_payload_restages_the_verified_pair_under_a_schema_two_receipt',
 'ServiceClientTests.test_split_pair_routes_user_commands_to_airkctl_and_service_operations_to_airk',
 'ServiceClientTests.test_original_pair_fixture_preserves_all_original_receipts_and_default_adapter',
 'ServiceClientTests.test_original_pair_fixture_opt_in_stages_original_source_without_receipt_projection',
 'ServiceClientTests.test_original_pair_fixture_refuses_unverified_mismatched_or_invented_build_authority',
 'ServiceClientTests.test_original_pair_raw_metadata_refuses_duplicates_before_object_validation',
 'ServiceClientTests.test_original_pair_opt_in_bounds_before_any_metadata_parser_or_hasher']
def sha(p):
    with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
report={'schema':'airk.private-C7-build-proposal-cheap-validation/3','passed':False,'python_selected':8,
        'selectors':SELECTORS,'runs':[],'actual_native_service_or_NN':False,'applied_tracked_source':True,'Cargo_or_compilation':False}
try:
    rustfmt=Path(r'C:\Users\tdotn\.rustup\toolchains\1.98.0-x86_64-pc-windows-msvc\bin\rustfmt.exe')
    native=['crates/airk-supervisor/src/runtime_metadata.rs','crates/airk-supervisor/src/runtime_metadata/split_build.rs',
        'crates/airk-supervisor/src/runtime_metadata/build_tests.rs',
        'crates/airk-supervisor/src/project_supervisor/guarded_qualification_tests/modern_checkpoint_tests.rs']
    commands=[('rust-parser-format',[str(rustfmt),'--edition','2024','--config','skip_children=true','--check',*[str(P/x) for x in native]]),
        ('python-controls',[sys.executable,'-B',str(P/'scripts/tests/test_service_clients.py'),*SELECTORS,'-v'])]
    for label,command in commands:
        begin=time.monotonic()
        result=subprocess.run(command,capture_output=True,env=ENV,timeout=45)
        (OUT/(label+'.stdout')).write_bytes(result.stdout);(OUT/(label+'.stderr')).write_bytes(result.stderr)
        report['runs'].append({'label':label,'argv':command,'exit_code':result.returncode,'elapsed_seconds':time.monotonic()-begin,
            'stdout':{'path':str(OUT/(label+'.stdout')),'sha256':sha(OUT/(label+'.stdout'))},
            'stderr':{'path':str(OUT/(label+'.stderr')),'sha256':sha(OUT/(label+'.stderr'))}})
        if result.returncode:raise RuntimeError('focused correction group failed: '+label)
        if label=='python-controls' and b'Ran 8 tests' not in result.stderr:raise RuntimeError('focused selection count differs')
    # Pure original-source admission only; no Fixture construction or native call.
    spec=importlib.util.spec_from_file_location('C7_r3_preflight',P/'scripts/check-service-clients.py')
    clients=importlib.util.module_from_spec(spec);spec.loader.exec_module(clients)
    repository=next(p for p in B.parents if (p/'Cargo.toml').exists())
    runtime=repository/'artifacts/arbiter-1.8.207/windows-runtime-debug-r1'
    names=('airk-install.json','airk-runtime.json','airk-build.json');before={name:sha(runtime/name) for name in names}
    assert clients.original_split_pair_payload(runtime)==runtime
    assert before=={name:sha(runtime/name) for name in names}
    report['original_pair_preflight']={'passed':True,'original_metadata_SHA_unchanged':before,
        'path':str(runtime),'scope':'Pure bounded original pair verification; no stage/process/NN'}
    report['python_passed']=8;report['passed']=True
finally:
    (OUT/'receipt.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'passed':report['passed'],'receipt':str(OUT/'receipt.json'),'sha256':sha(OUT/'receipt.json'),
        'runs':[{'label':r['label'],'exit_code':r['exit_code'],'elapsed_seconds':r['elapsed_seconds']} for r in report['runs']]},indent=2))