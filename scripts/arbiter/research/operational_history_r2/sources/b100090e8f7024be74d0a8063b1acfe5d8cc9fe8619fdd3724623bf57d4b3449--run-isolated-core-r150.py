# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,shutil,subprocess,sys
from pathlib import Path
r=Path.cwd();checkout=r/'artifacts/advanced-split/core-contract-gate-r149';out=r/'artifacts/advanced-split/receipts/core-contract-r150';assert not out.exists();out.mkdir()
m=r/'artifacts/advanced-split/model-download-origin-preparation-r1/core-contract-r1/manifest.json'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
assert sha(m)=='9ab6150bc84cfbc01aa00f33694f79b8bfdf6b4524e9705e1783fc3bc94cef95'
rows=json.loads(m.read_bytes())['rows'];main_pins={}
for row in rows:
    main=r/row['target'];proposal=Path(row['proposal']);target=checkout/row['target']
    assert (sha(main)if main.exists()else None)==row['base_sha256'];assert sha(proposal)==row['proposal_sha256']
    main_pins[str(main)]=row['base_sha256'];target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(proposal,target)
env=dict(os.environ);env.update(GIT_CONFIG_COUNT='1',GIT_CONFIG_KEY_0='safe.directory',GIT_CONFIG_VALUE_0=str(checkout),CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='8',CARGO_INCREMENTAL='1',CARGO_TARGET_DIR=str(r/'artifacts/v190/p10-target'))
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe'
base=[sys.executable,'scripts/dev/receipt_runner.py','--groups','native,fixtures','--out',str(out/'gates'),'--target-dir',env['CARGO_TARGET_DIR']]
commands=[('clippy',None,[*base,'--allow-no-tests','clippy',cargo,'clippy','-p','airk-core','--all-targets','--all-features','--locked','--timings','--','-D','warnings']),('focused',6,[*base,'focused',cargo,'nextest','run','--profile','ci','-p','airk-core','--all-features','--locked','--timings','-E','test(arbiter::downloads::tests::) | test(retained_profile_bytes_have_no_stage_insertion_or_digest_reidentification) | test(compiled_asset_download_closure_is_bounded_official_and_preserves_legacy_omission)'])]
for name,count,cmd in commands:
    result=subprocess.run(cmd,cwd=checkout,env=env)
    if result.returncode:raise SystemExit(result.returncode)
    if count is not None:assert json.loads((out/'gates'/(name+'.json')).read_bytes())['selected_tests']==count
emitted=out/'native-emitted';emitted.mkdir()
env['AIRK_WRITE_ARBITER_DOWNLOAD_ORIGIN_CASES']=str(emitted/'download_origin_cases.json');env['AIRK_WRITE_ARBITER_SCHEMA']=str(emitted/'arbiter.schema.json')
cmd=[*base,'emitter',cargo,'nextest','run','--profile','ci','-p','airk-core','--all-features','--locked','--timings','-E','test(arbiter::downloads::tests::native_download_origin_semantic_fixture_emitter)']
result=subprocess.run(cmd,cwd=checkout,env=env)
if result.returncode:raise SystemExit(result.returncode)
assert json.loads((out/'gates/emitter.json').read_bytes())['selected_tests']==1
for name in ('AIRK_WRITE_ARBITER_DOWNLOAD_ORIGIN_CASES','AIRK_WRITE_ARBITER_SCHEMA'):env.pop(name)
schema=checkout/'packages/contracts-gen/arbiter.schema.json';before_schema=sha(schema)
shutil.copyfile(emitted/'arbiter.schema.json',schema);shutil.copyfile(emitted/'download_origin_cases.json',checkout/'fixtures/split/arbiter/download_origin_cases.json')
cmd=[*base,'schema',cargo,'nextest','run','--profile','ci','-p','airk-core','--all-features','--locked','--timings','-E','test(arbiter::schema::tests::schema_matches_reviewed_capture)']
result=subprocess.run(cmd,cwd=checkout,env=env)
if result.returncode:raise SystemExit(result.returncode)
assert json.loads((out/'gates/schema.json').read_bytes())['selected_tests']==1
for flag in ('--write','--check'):
    with (out/('contract-types-'+flag[2:]+'.log')).open('w',encoding='utf-8')as log:
        result=subprocess.run([sys.executable,'scripts/dev/contract_types.py','arbiter',flag],cwd=checkout,env=env,stdout=log,stderr=subprocess.STDOUT)
    if result.returncode:raise SystemExit(result.returncode)
for path,digest in main_pins.items():assert (sha(Path(path))if Path(path).exists()else None)==digest
record={'complete':True,'scope':'Isolated Core owning contract only; current model-capture public source remains unchanged. Sole Root Cargo owner reuses persistent target. Not release qualification.','manifest_sha256':sha(m),'checkout':str(checkout),'target_directory':env['CARGO_TARGET_DIR'],'incremental':'1','focused':6,'emitter':1,'schema':1,'main_core_pins_unchanged':main_pins,'old_schema_sha256':before_schema,'native_emitted':{str(p):sha(p)for p in emitted.iterdir()}}
(out/'completion.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8');print(json.dumps(record))
