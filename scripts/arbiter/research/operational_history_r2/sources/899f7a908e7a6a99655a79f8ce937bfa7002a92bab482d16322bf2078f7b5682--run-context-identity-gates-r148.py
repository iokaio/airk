# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,shutil,subprocess,sys
from pathlib import Path
r=Path.cwd();env=dict(os.environ)
env.update(json.loads((r/'artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json').read_bytes())['environment'])
env.update(CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',AIRK_BUNDLED_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v3'),AIRK_TEST_MODEL_DIR=str(r/'artifacts/advanced-split/bundled-model-v3/minilm-l6-v2-u8-v3'))
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe';out=r/'artifacts/advanced-split/receipts/context-identity-gates-r148';assert not out.exists()
base=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures,harness','--out',str(out),'--target-dir',str(r/'artifacts/v190/p10-target')]
packages=['-p','airk-kernel','-p','airk-mcp','-p','airk-service','-p','airk-cli']
selector='test(native_context_partition_keeps_snapshot_digest_separate_from_installed_selection) | test(native_focus_preserves_context_snapshot_identity_and_checks_singleton_selection) | test(focus_handler_tests::)'
commands=[
 ('workspace-clippy',None,[*base,'--allow-no-tests','workspace-clippy',cargo,'clippy','--workspace','--all-targets','--all-features','--locked','--timings','--','-D','warnings']),
 ('focused',6,[*base,'focused',cargo,'nextest','run','--profile','ci',*packages,'--all-features','--locked','--timings','-E',selector]),
 ('actual-combined',1,[*base,'actual-combined',cargo,'nextest','run','--profile','ci',*packages,'--all-features','--locked','--timings','-E','test(current_answer_many_and_open_keep_complete_cli_pipe_and_https_mcp_packets)']),
 ('actual-images',None,[*base,'--allow-no-tests','actual-images',cargo,'test',*packages,'--all-features','--no-run','--message-format=json','--locked','--timings']),
]
for name,count,command in commands:
    result=subprocess.run(command,env=env)
    if result.returncode:raise SystemExit(result.returncode)
    if count is not None:assert json.loads((out/(name+'.json')).read_bytes())['selected_tests']==count
def sha(p):
    with p.open('rb')as f:return hashlib.file_digest(f,'sha256').hexdigest()
images=[]
for line in (out/'actual-images.log').read_text(encoding='utf-8').splitlines():
    try:row=json.loads(line)
    except json.JSONDecodeError:continue
    if row.get('reason')=='compiler-artifact'and row.get('profile',{}).get('test')is True and row.get('executable')and row['target']['name']in {'airk_kernel','airk_service'}:
        source=Path(row['executable']);dest=out/'images'/source.name;dest.parent.mkdir(exist_ok=True);assert not dest.exists();shutil.copyfile(source,dest);assert sha(source)==sha(dest)
        images.append({'target':row['target']['name'],'path':str(dest),'cargo_artifact_path':str(source),'sha256':sha(dest),'bytes':dest.stat().st_size})
assert len(images)==2
(out/'images.json').write_text(json.dumps({'schema':'airk.private-actual-cargo-native-images/1','images':images,'cargo_json':str(out/'actual-images.log'),'build_receipt':str(out/'actual-images.json')},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'complete':True,'focused':6,'actual':1,'images':images}))
