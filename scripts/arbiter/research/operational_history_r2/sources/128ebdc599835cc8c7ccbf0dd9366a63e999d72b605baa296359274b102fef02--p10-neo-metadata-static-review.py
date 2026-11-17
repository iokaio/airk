# SPDX-License-Identifier: Apache-2.0
"""Local parse/command review only; never builds, archives, connects or launches."""
import ast
import hashlib
import importlib.util
import json
from pathlib import Path

base=Path(__file__).resolve().parent
adapter=base/'p10-neo-metadata.py'
spec=importlib.util.spec_from_file_location('metadata_plan',adapter)
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
plan=module.plan()
assert len(plan['steps'])==3
assert plan['steps'][0]['command']==['cargo','fmt','--all','--check']
for row,feature in zip(plan['steps'][1:],['--no-default-features','--all-features']):
    assert row['expected_passing_tests']==10
    assert row['command']==['cargo','test','-p','airk-kernel','--lib','--locked','--offline','--timings',feature,'scoped_reads::tests::','--','--test-threads=1']
runner=(base/'p10-native-20261105-r2-runner.py').read_text(encoding='utf-8',errors='strict')
anchor="        steps.append({'actual_exit_code': actual_exit_code, 'passing_tests': passing_tests"
guard="""        expected_passing_tests = row.get('expected_passing_tests')
        if result.returncode == 0 and expected_passing_tests is not None and passing_tests != expected_passing_tests:
            validation_error = f'Expected {expected_passing_tests} passing tests, observed {passing_tests}'
            result.returncode = 1
"""
assert runner.count(anchor)==1
candidate=runner.replace(anchor,guard+anchor)
ast.parse(candidate)
for needle in ["environment['TMPDIR'] = str(Path(tempfile.gettempdir()).resolve(strict=True))",
               "CARGO_BUILD_JOBS='1'","CARGO_INCREMENTAL='0'",
               "resource.setrlimit(resource.RLIMIT_NOFILE, (4096, hard))"]:
    assert needle in candidate,needle
for name in ['p10-neo-metadata.py','p10-neo-prepare.py','p10-neo-owner.py','p10-neo-launch.py']:
    ast.parse((base/name).read_text(encoding='utf-8',errors='strict'))
owner=(base/'p10-neo-owner.py').read_text(encoding='utf-8',errors='strict')
assert "finally:" in owner and "'cargo','clean'" in owner and "removed=not target.exists()" in owner
assert owner.index("timing_source=target/'cargo-timings'")<owner.index("'cargo','clean'")
launcher=(base/'p10-neo-launch.py').read_text(encoding='utf-8',errors='strict')
assert "destination.read_bytes()==content" in launcher and "destination.open('xb')" in launcher
assert "Remote steps differ from frozen local plan" in launcher
assert "test ! -e '+remote+'/target'" in launcher
for suffix in ['.tar.gz','-snapshot.json','-plan.json','-runner.py','-metadata-prepared.json','-active.json']:
    assert not (base/(module.LABEL+suffix)).exists(),suffix
record={
 'status':'passed_static_local_adapter_review_only', 'label':module.LABEL,'native_source':module.EXPECTED,
 'adapter_sha256':hashlib.sha256(adapter.read_bytes()).hexdigest(),
 'preview_sha256':hashlib.sha256((base/(module.LABEL+'-metadata-preview.json')).read_bytes()).hexdigest(),
 'steps':3,'expected_passing_tests':20,
 'commands':[row['command'] for row in plan['steps']],
 'source_archive_created':False,'source_prepared':False,'ssh_performed':False,'cargo_performed':False,
 'safety':'Reuses reviewed public-only source freezer and unconditional cleanup owner, strict UTF8, jobs1/inc0/offline/default canonicalTMPDIR/FD4096; timing HTML copied before clean including failure; collector validates exact local step catalog and immutable identical-only evidence.',
 'remaining':'Root must run required Windows R6 source gates first, then explicit prepare/launch. Prepare refuses if native source differs from exactc713/945. Actual Mac counts/exit/source/clean target verification remains unrun; historical broad16/66 evidence staysa745.'}
module.retain_json(base/(module.LABEL+'-metadata-static-review.json'),record)
print(json.dumps({'status':record['status'],'archive_created':False,'ssh_performed':False,'steps':3,'expected_passing_tests':20}))
