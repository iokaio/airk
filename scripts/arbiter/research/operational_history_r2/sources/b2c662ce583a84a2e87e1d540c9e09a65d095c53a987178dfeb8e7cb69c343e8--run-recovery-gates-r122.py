# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,shutil,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ)
env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_bytes())['environment'])
env.update(CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v3'),AIRK_TEST_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v3/minilm-l6-v2-u8-v3'))
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
out=r/'artifacts/advanced-split/receipts/native-recovery-gates-r122'
common=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures,harness','--out',str(out),'--target-dir',str(r/'artifacts/v190/p10-target')]
packages=['-p','airk-service','-p','airk-supervisor','-p','airk-kernel']
commands=[
 [*common,'--allow-no-tests','scoped-clippy',cargo,'clippy',*packages,'--all-targets','--all-features','--locked','--timings','--','-D','warnings'],
 [*common,'bridge-focused',cargo,'nextest','run','--profile','ci',*packages,'--all-features','--locked','--timings','-E','test(runtime_metadata::arbiter::tests::sync_control_)'],
 [*common,'--allow-no-tests','actual-images',cargo,'test',*packages,'--all-features','--no-run','--message-format=json','--locked','--timings'],
]
for command in commands:
    result=subprocess.run(command,env=env)
    if result.returncode:raise SystemExit(result.returncode)
images=[]
for line in (out/'actual-images.log').read_text(encoding='utf-8').splitlines():
    try:row=json.loads(line)
    except json.JSONDecodeError:continue
    if row.get('reason')=='compiler-artifact' and row.get('profile',{}).get('test') is True and row.get('executable') and row['target']['name'] in ('airk_service','airk_kernel','airk_supervisor'):
        source=Path(row['executable']);destination=out/'images'/source.name
        destination.parent.mkdir(exist_ok=True)
        assert not destination.exists()
        shutil.copyfile(source,destination)
        def digest(path):
            with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
        assert digest(source)==digest(destination)
        images.append({'target':row['target']['name'],'path':str(destination),'cargo_artifact_path':str(source),'sha256':digest(destination),'bytes':destination.stat().st_size})
assert {row['target'] for row in images}=={'airk_service','airk_kernel','airk_supervisor'} and len(images)==3
(out/'images.json').write_text(json.dumps({'schema':'airk.private-actual-cargo-native-images/1','images':images,'cargo_json':str(out/'actual-images.log'),'build_receipt':str(out/'actual-images.json'),'prior_warm_and_accounting_tests':'R121 actual10 passes retained under source44e; only four Windows blocking-peer cases failed, corrected before R122sixcase rerun.'},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'complete':True,'receipt_directory':str(out),'expected_bridge_tests':6,'images':images}))
