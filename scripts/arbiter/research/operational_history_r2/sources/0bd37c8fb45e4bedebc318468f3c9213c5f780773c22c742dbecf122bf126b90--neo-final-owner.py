# SPDX-License-Identifier: Apache-2.0
"""One final frozen-source Neo ARM batch. Launched only after root approval."""
import argparse,hashlib,importlib.util,json,os,platform,resource,shutil,socket,sys,time,tomllib
from pathlib import Path

def load(source,name,filename):
 spec=importlib.util.spec_from_file_location(name,source/'scripts'/filename);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

def main():
 parser=argparse.ArgumentParser(description=__doc__)
 parser.add_argument('--campaign',type=Path,required=True)
 parser.add_argument('--native-sha256',required=True)
 parser.add_argument('--previous-runtime',type=Path,required=True)
 args=parser.parse_args()
 assert platform.system()=='Darwin' and platform.machine()=='arm64' and os.getuid()==501 and socket.gethostname()=='Tylers-MacBook-Neo.local'
 campaign=args.campaign.resolve(strict=True)
 assert campaign.parent==Path('/Users/tylerjensen/airk-builds') and campaign.name.startswith('one-airk-1814-final-')
 source=(campaign/'source').resolve(strict=True);target=campaign/'cargo-target';fixtures=campaign/'fixtures';evidence=campaign/'evidence'
 assert target.parent==campaign and target.name=='cargo-target' and target.absolute()!=(source/'target').absolute()
 assert not target.exists() and not target.is_symlink() and not (source/'target').is_symlink()
 evidence.mkdir(mode=0o700,exist_ok=False);fixtures.mkdir(mode=0o700,exist_ok=False)
 state={'schema':1,'purpose':'user-requested full frozen-source ARM workspace/Python/TypeScript qualification; no Intel/publication claim','host':'192.168.68.61','campaign':str(campaign),'source':str(source),'compiler_target':str(target),'fixture_targets_preserved':[str(fixtures),str(source/'target')],'commands':[],'result':'running','no_speculative_benchmarks':True,'qualification_complete':False,'mandatory_remaining':['Developer ID exact-leaf signing and notarization','final signed native and sealed VSIX packaging/verification','same-UID Aqua public SMAppService login install/status/stop/start/uninstall acceptance','actual installed-editor native HTTPS lifecycle and upgrade','OS user-CA trust acceptance if authorized; private-CA fixtures do not prove it'],'native_extension_smoke':'Current reviewed native HTTPS smoke enabled; actual installed-editor qualification remains mandatory'}
 tick=time.monotonic();active=None
 def save():
  state['elapsed_seconds']=round(time.monotonic()-tick,3);(evidence/'batch-receipt.json').write_text(json.dumps(state,indent=2)+'\n')
 def sha(path):
  with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
 try:
  owner=load(source,'neo_owned_command','provision-static-onnx-runtime.py')
  builder=load(source,'neo_runtime_builder','build-service-runtime.py')
  manifest=json.loads((campaign/'source-manifest.json').read_text())
  assert manifest.get('status')=='FROZEN_PUBLIC_SOURCE' and manifest.get('final_freeze_required') is False
  assert manifest['source_roster_sha256']==hashlib.sha256(json.dumps(manifest['files'],sort_keys=True,separators=(',',':')).encode()).hexdigest()
  assert manifest['native_source']==builder.source_identity() and manifest['native_source']['sha256']==args.native_sha256
  for row in manifest['files']:
   relative=Path(row['path']);assert not relative.is_absolute() and '..' not in relative.parts
   path=source/relative;assert path.is_file() and not path.is_symlink() and path.stat().st_size==row['bytes'] and sha(path)==row['sha256']
  state['runner_sha256']=sha(Path(__file__).resolve(strict=True));state['restored_long_campaigns']=['two_hundred_current_edits_reach_object_and_database_plateau','compact_measurements_compare_small_reads_from_large_catalog','manual_backlog_larger_than_one_page_advances_without_unprotected_deletion','measure_small_and_larger_sparse_catalog_edits'];state['source_identity']=builder.source_identity();state['source_manifest_sha256']=sha(campaign/'source-manifest.json');state['git_producer']=manifest['head'];state['source_roster_sha256']=manifest['source_roster_sha256'];save()
  tools=Path('/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P')
  base=dict(os.environ);base['PATH']=str(Path.home()/'.cargo/bin')+':'+str(tools/'tools/node-v24.15.0-darwin-arm64/bin')+':'+str(tools/'tools')+':'+str(Path(sys.executable).parent)+':'+base.get('PATH','')
  base.update(CARGO_TARGET_DIR=str(target),CARGO_BUILD_JOBS='2',MACOSX_DEPLOYMENT_TARGET='13.3',AIRK_BUNDLED_MODEL_DIR=str(campaign/'offline-model'),AIRK_TEST_MODEL_DIR=str(campaign/'offline-model/minilm-l6-v2-s8-v1'),AIRK_PRIVATE_CENSUS_BASE=str(fixtures),TMPDIR=str(fixtures),TEMP=str(fixtures),TMP=str(fixtures),PYTHONNOUSERSITE='1',PYTHONDONTWRITEBYTECODE='1')
  nextest=tools/'tools/cargo-nextest'
  assert nextest.is_file() and not nextest.is_symlink() and sha(nextest)=='aac4a0d9109efcfb4d83ec729c6e3204d5c15bf16ff554b5ee4476f6c5f60b37'
  assert Path(shutil.which('cargo-nextest',path=base['PATH'])).resolve(strict=True)==nextest
  state['nextest']={'path':str(nextest),'sha256':sha(nextest),'version':'0.9.148','profile':'ci','test_threads':2}
  repository_nextest=source/'.config/nextest.toml'
  nextest_bytes=repository_nextest.read_bytes();nextest_policy=tomllib.loads(nextest_bytes.decode())
  assert 'store' not in nextest_policy, 'Explicit source store configuration requires owner review before derivation'
  junit_relative=Path(nextest_policy['profile']['ci']['junit']['path'])
  assert str(junit_relative) and not junit_relative.is_absolute() and '..' not in junit_relative.parts
  nextest_gates={}
  for label,stage in [('checkpoint','debug-native-checkpointed-owner-drain-regression'),('workspace','full-workspace-all-feature-tests-including-four-long-campaigns')]:
   store=evidence/('nextest-'+label+'-store');config=evidence/('nextest-'+label+'.toml')
   derived=('[store]\ndir = '+json.dumps(str(store))+'\n\n').encode()+nextest_bytes
   assert tomllib.loads(derived.decode())=={**nextest_policy,'store':{'dir':str(store)}}
   with config.open('xb') as stream:stream.write(derived)
   config.chmod(0o600)
   nextest_gates[stage]={'config':str(config),'config_sha256':sha(config),'source_config_sha256':sha(repository_nextest),'store':str(store),'junit':str(store/'ci'/junit_relative),'policy_changes':['store.dir only'],'user_config':'none'}
  state['nextest_evidence']=nextest_gates;save()
  checkpoint_config=Path(nextest_gates['debug-native-checkpointed-owner-drain-regression']['config'])
  workspace_config=Path(nextest_gates['full-workspace-all-feature-tests-including-four-long-campaigns']['config'])
  checkpoint_junit=Path(nextest_gates['debug-native-checkpointed-owner-drain-regression']['junit'])
  workspace_junit=Path(nextest_gates['full-workspace-all-feature-tests-including-four-long-campaigns']['junit'])
  state['macos_deployment_target']='13.3'
  environment=load(source,'neo_static_environment','static-runtime-environment.py').load_environment_file(Path('/Users/tylerjensen/airk-builds/one-airk-static-ort-1814-1794087059/static-environment-recovery.json'),target='aarch64-apple-darwin')
  base.update(environment);state['static_environment']=environment
  soft,hard=resource.getrlimit(resource.RLIMIT_NOFILE);assert hard>=4096;resource.setrlimit(resource.RLIMIT_NOFILE,(4096,hard));state['nofile']={'before':[soft,hard],'after':list(resource.getrlimit(resource.RLIMIT_NOFILE))};save()
  development={**base,'CARGO_INCREMENTAL':'0'};release=dict(base);release.pop('CARGO_INCREMENTAL',None)
  runtime_parent=source/'artifacts/neo-final';runtime_parent.mkdir(parents=True,exist_ok=False)
  debug=runtime_parent/'debug-runtime';runtime=runtime_parent/'runtime'
  def run(name,argv,timeout=1200,env=None):
   nonlocal active
   assert active is None
   (campaign/'phase.txt').write_text(name);active=name
   row={'stage':name,'argv':list(map(str,argv)),'scope':name,'cwd':str(source),'compiler_target':str(target),'incremental':(env or development).get('CARGO_INCREMENTAL','profile-default'),'started_unix':time.time(),'result':'running'}
   state['commands'].append(row);save()
   try:
    result=owner.run_owned(row['argv'],source,evidence/(name+'.log'),timeout,env or development);row.update(result)
    row['result']='passed' if owner.successful(result) else 'failed'
    if name in nextest_gates:
     junit=Path(nextest_gates[name]['junit']);row['nextest_config']=nextest_gates[name]
     if junit.is_file() and not junit.is_symlink():
      row['junit_evidence']={'path':str(junit),'bytes':junit.stat().st_size,'sha256':sha(junit),'retained_outside_compiler_target':True}
     else:
      row['junit_missing']=True
      row['junit_missing_reason']='Nextest returned before producing its explicitly pinned report; complete command log retained'
      if row['result']=='passed':row['result']='failed';row['evidence_error']='Successful Nextest command omitted its pinned JUnit report'
    save()
    if row['result']!='passed':raise RuntimeError(name+' failed; collect complete batch before correction')
    return result
   except BaseException as error:
    row['result']='failed';row['error']=str(error);save();raise
   finally:active=None
  run('verify-public-offline-models',[sys.executable,source/'scripts/provision-local-model.py','--destination',campaign/'offline-model'],120)
  run('fetch-locked-rust-dependencies',['cargo','fetch','--locked','--target','aarch64-apple-darwin'],900)
  run('extension-npm-ci',['npm','ci','--prefer-offline','--ignore-scripts','--no-audit','--no-fund','--prefix','packages/vscode'],600)
  run('extension-initial-compile',['npm','run','compile','--prefix','packages/vscode'],600)
  run('full-Python-contract-discovery',[sys.executable,'-m','unittest','discover','-s','scripts/tests','-v'],1800)
  run('rust-format',['cargo','fmt','--all','--','--check'],120)
  run('workspace-all-feature-clippy',[sys.executable,source/'scripts/dev-check.py','--workspace','--all-features','--target-dir',target,'--incremental','off','--skip-fmt'],10800)
  run('lightweight-main-components',['cargo','check','-p','airk-cli','--no-default-features','--all-targets','--locked','--timings','--target-dir',target],3600)
  run('debug-static-runtime',[sys.executable,source/'scripts/build-service-runtime.py','--profile','debug','--destination',debug],10800)
  assert builder.source_identity()==state['source_identity']
  checkpoint_test='runtime_install::native_connection_tests::native_checkpointed_owner_drains_without_reopening_admission'
  checkpoint_package=load(source,'neo_checkpoint_runtime_package','package-native.py')
  checkpoint_files,_=checkpoint_package.validate_runtime(debug,'1.8.14','aarch64-apple-darwin',state['source_identity'])
  checkpoint_install_sha=sha(debug/'airk-install.json');checkpoint_build_sha=sha(debug/'airk-build.json')
  checkpoint_build=json.loads((debug/'airk-build.json').read_text())
  assert checkpoint_build['source']==state['source_identity']
  state['native_checkpoint_regression_inputs']={'test':checkpoint_test,'runtime':str(debug),'install_manifest_sha256':checkpoint_install_sha,'build_receipt_sha256':checkpoint_build_sha,'compiled_source':checkpoint_build['source'],'test_source':state['source_identity'],'sealed_payload_files':len(checkpoint_files),'scope':'only this explicitly selected ignored native regression; ordinary ignored helper-child/manual platform/performance gates remain unselected and unqualified'};save()
  checkpoint_environment={**development,'AIRK_F03_RUNTIME':str(debug),'AIRK_F03_APPROVED_SHA256':checkpoint_install_sha}
  run('debug-native-checkpointed-owner-drain-regression',['cargo','nextest','run','-p','airk-cli','--lib','--all-features','--profile','ci','--config-file',checkpoint_config,'--user-config-file','none','--run-ignored','only','-E','test(='+checkpoint_test+')','--test-threads','1','--locked','--timings','--no-fail-fast'],10800,checkpoint_environment)
  assert checkpoint_junit.is_file() and not checkpoint_junit.is_symlink()
  shutil.copy2(checkpoint_junit,evidence/'checkpoint-regression-junit.xml')
  import xml.etree.ElementTree as ET
  executed=[case for case in ET.parse(checkpoint_junit).iter('testcase') if case.find('skipped') is None]
  assert len(executed)==1 and executed[0].get('name')==checkpoint_test and executed[0].find('failure') is None and executed[0].find('error') is None
  assert sha(debug/'airk-install.json')==checkpoint_install_sha and sha(debug/'airk-build.json')==checkpoint_build_sha
  assert checkpoint_package.inventory(debug)==checkpoint_files
  state['native_checkpoint_regression_inputs']['result']='one_exact_native_test_passed'
  state['native_checkpoint_regression_inputs']['junit']={'path':str(evidence/'checkpoint-regression-junit.xml'),'sha256':sha(evidence/'checkpoint-regression-junit.xml')};save()
  run('debug-affected-install-https-preflight',[sys.executable,source/'scripts/check-service-install.py','--runtime',debug],1800)
  run('debug-actual-storage-lifecycle-preflight',[sys.executable,source/'scripts/check-storage-lifecycle.py','--binary',debug/'AIRK.app/Contents/MacOS/airk'],1800)
  previous=args.previous_runtime.resolve(strict=True)
  assert sha(previous/'airk-install.json')=='47f0b790645a707077c00015e5e38cc6256b57e0510f6da97c6c2db25002e038'
  run('debug-genuine-schema1-upgrade-preflight',[sys.executable,source/'scripts/check-runtime-upgrade.py','--runtime',debug,'--previous-runtime',previous,'--source-state','indexed','--fixture-setup','reviewed','--semantic-old','--output',evidence/'debug-schema1-upgrade.json'],1800)
  historical=Path('/Users/tylerjensen/airk-builds/old-standalone-fixtures-20261107-1814/campaign.json')
  assert historical.is_file() and not historical.is_symlink() and historical.stat().st_uid==os.getuid()
  assert sha(historical)=='3d6d38223febd6ff9df74e8041c70d309e5fb9320bcbbe087bda9ec578a3505c'
  adoption_gate=source/'scripts/check-standalone-adoption.py'
  assert sha(adoption_gate)=='6b9382a3feddb4e535792b84f9766927de1bd7ec99e25c1a33f9892cb4ee9fea'
  adoption_failure_gate=source/'scripts/check-standalone-adoption-failure.py'
  assert sha(adoption_failure_gate)=='990e38aa8a3b34aca293e2adb1e696f781336abfa016449e8de9d055722d6d94'
  shutil.copy2(historical,evidence/'original-pre14-fixture-report.json')
  state['authentic_pre14_fixture_inputs']={'report':str(historical),'sha256':sha(historical),'preserved_receipt':str(evidence/'original-pre14-fixture-report.json'),'harness_sha256':sha(adoption_gate),'tags':['v1.3.1','v1.0.0-build.7'],'original_modification_authorized':False,'execution_scope':'one debug sealed-runtime preflight, each genuine old version with two separately claimed original client configurations; owned copies only'};save()
  run('debug-genuine-pre14-two-client-adoption',[sys.executable,adoption_gate,'--runtime',debug,'--fixture-report',historical,'--model-js',source/'packages/vscode/out/model.js','--tag','v1.3.1','--tag','v1.0.0-build.7','--output',evidence/'debug-pre14-adoption.json'],1800)
  assert sha(historical)==state['authentic_pre14_fixture_inputs']['sha256']
  state['authentic_pre14_negative_inputs']={'harness_sha256':sha(adoption_failure_gate),'positive_helper_sha256':sha(adoption_gate),'fixture_report_sha256':sha(historical),'tag':'v1.0.0-build.7','scope':'deliberately corrupt only an owned copied historical object; require native failure/restoration, unchanged originals, no adoption and exact canonical final Stop; retain pending copied fixture even on successful negative qualification'};save()
  run('debug-genuine-pre14-corrupt-copy-restoration',[sys.executable,adoption_failure_gate,'--runtime',debug,'--fixture-report',historical,'--tag','v1.0.0-build.7','--output',evidence/'debug-pre14-corrupt-copy-restoration.json'],1800)
  negative_receipt=evidence/'debug-pre14-corrupt-copy-restoration.json'
  negative=json.loads(negative_receipt.read_text())
  assert negative.get('passed') is True and negative.get('adoption_succeeded') is False and negative.get('fixture_preserved') is True
  assert negative.get('original_fixture_unchanged') is True and negative.get('pending_cache_preserved') is True
  assert negative.get('harness_sha256')==sha(adoption_failure_gate) and negative.get('positive_helper_sha256')==sha(adoption_gate)
  assert negative.get('fixture_report_sha256')==sha(historical) and negative.get('runtime_manifest_sha256')==sha(debug/'airk-install.json')
  assert negative.get('canonical_final_stop',{}).get('status')=='lifecycle' and negative['canonical_final_stop']['record']['phase']=='complete'
  assert sha(historical)==state['authentic_pre14_fixture_inputs']['sha256']
  state['authentic_pre14_negative_inputs']['receipt']={'path':str(negative_receipt),'sha256':sha(negative_receipt),'outcome':'expected native adoption refusal with actual owner restoration and canonical final Stop; not a successful adoption'};save()
  debugenv=dict(development)
  state['native_test_execution']='Rust integration fixtures execute their matched CARGO_BIN_EXE_airk; staged debug runtime separately qualifies Python installed HTTPS preflight, not an AIRK_TEST_BINARY override';save()
  command=['cargo','nextest','run','--workspace','--all-features','--profile','ci','--config-file',workspace_config,'--user-config-file','none','--ignore-default-filter','--test-threads','2','--locked','--timings','--no-fail-fast']
  run('full-workspace-all-feature-tests-including-four-long-campaigns',command,10800,debugenv)
  run('full-workspace-all-feature-doctests',['cargo','test','--doc','--workspace','--all-features','--locked','--timings','--','--test-threads=1'],10800,debugenv)
  run('fresh-release-static-runtime',[sys.executable,source/'scripts/build-service-runtime.py','--profile','release','--destination',runtime],10800,release)
  assert builder.source_identity()==state['source_identity']
  run('release-actual-model-clients',[sys.executable,source/'scripts/check-service-clients.py','--runtime-dir',runtime,'--files','16','--model','--report',evidence/'release-model-clients.json'],1800,release)
  state['performance_observation_limits']=['Authenticated1/10/64-client semantic campaign only; no project-count matrix','CPU/private-RSS/cold-filesystem/affinity facts retain the authored benchmark exact observed/unmeasured qualifiers; no inferred measurements'];save()
  run('release-semantic-client-counts',[sys.executable,source/'scripts/benchmark-service.py','--runtime-dir',runtime,'--clients','1','10','64','--projects','1','--rounds','5','--semantic','--output',evidence/'release-semantic-client-counts.json'],3600,release)
  client_report=evidence/'release-semantic-client-counts.json'
  client_counts=json.loads(client_report.read_text())
  assert client_counts.get('schema')=='airk-service-load/2' and client_counts.get('semantic') is True and client_counts.get('passed') is True
  assert client_counts.get('runtime_manifest_sha256')==sha(runtime/'airk-install.json')
  assert sorted((c['clients'],c['projects'],c['rounds']) for c in client_counts['cases'])==[(1,1,5),(10,1,5),(64,1,5)]
  state['semantic_client_count_observation']={'receipt':str(client_report),'sha256':sha(client_report),'limitations':client_counts['limitations'],'metrics_not_measured':['CPU','private physical memory','controlled cold filesystem/service latency','effective affinity','global process completeness']}
  state['mandatory_remaining'].append('Performance qualification remains incomplete for CPU, private physical memory, controlled cold-cache latency and affinity; the 1/10/64-client campaign records its actual native telemetry and limitations only')
  save()
  run('release-real-retrieval-quality',[sys.executable,source/'scripts/check-retrieval-quality.py','--binary',runtime/'AIRK.app/Contents/MacOS/airk','--output',evidence/'release-retrieval.json'],1800,release)
  run('release-native-install-https-lifecycle',[sys.executable,source/'scripts/check-service-install.py','--runtime',runtime],1800,release)
  run('release-read-only-managed-payload',[sys.executable,source/'scripts/check-managed-runtime.py','--runtime',runtime],1800,release)
  run('release-private-issuer-rotation',[sys.executable,source/'scripts/check-issuer-rotation.py','--runtime-dir',runtime,'--output',evidence/'issuer-rotation-private'],1800,release)
  smokeenv={**release,'AIRK_TEST_BINARY':str(runtime/'AIRK.app/Contents/MacOS/airk'),'AIRK_NATIVE_TEST_BINARY':str(runtime/'AIRK.app/Contents/MacOS/airk'),'AIRK_TEST_F09_QUALIFICATION':'1'}
  testfiles=sorted((source/'packages/vscode/out/test').glob('*.test.js'))
  assert testfiles and len(testfiles)<=256
  state['TypeScript_native_test_inputs']={'runtime':str(runtime),'install_manifest_sha256':sha(runtime/'airk-install.json'),'F09_qualification':True,'previous_standalone_helper':'not supplied: authenticated1.8.12sealedapp cannot be copied outsidebundle or relabeled as legacy standalone1.4 helper; historicalfixture requirement remains explicit'};save()
  run('full-TypeScript-contracts-and-enabled-native-HTTPS',['node','--test','--test-concurrency=2',*testfiles],10800,smokeenv)
  run('unsigned-native-package',[sys.executable,source/'scripts/package-native.py','--target','aarch64-apple-darwin','--runtime-dir',runtime,'--output',runtime_parent/'native-unsigned'],600,release)
  run('unsigned-arm-vsix',[sys.executable,source/'scripts/package-extension.py','--target','aarch64-apple-darwin','--native-dir',runtime_parent/'native-unsigned/publish','--output',runtime_parent/'vscode-unsigned'],600,release)
  state['result']='passed';state['runtime']={'path':str(runtime),'install_manifest_sha256':sha(runtime/'airk-install.json'),'build_receipt_sha256':sha(runtime/'airk-build.json')}
 except BaseException as error:
  state['result']='failed';state['error']=str(error)
 finally:
  save()
  timings=target/'cargo-timings'
  try:
   if timings.is_dir():shutil.copytree(timings,evidence/'cargo-timings',symlinks=False)
   junit=workspace_junit if 'workspace_junit' in locals() else None
   if junit is not None and junit.exists():
    assert junit.is_file() and not junit.is_symlink()
    shutil.copy2(junit,evidence/'nextest-junit.xml');state['nextest_junit']={'path':str(evidence/'nextest-junit.xml'),'bytes':junit.stat().st_size,'sha256':sha(junit)}
   state['evidence_preserved_before_clean']=True
  except BaseException as error:
   state['evidence_preserved_before_clean']=False;state['timing_preservation_error']=str(error);state['result']='failed'
  save()
  assert target.parent.resolve(strict=True)==campaign and target.name=='cargo-target' and not target.is_symlink()
  assert target.absolute()!=(source/'target').absolute() and target!=fixtures and target!=(campaign/'offline-model')
  # Cargo clean targets the compiler tree only. Failed native fixtures, source,
  # original runtime/archive producers and model prerequisites stay in place.
  descendants_settled=all(row.get('owned_descendants_settled') is True for row in state['commands'])
  state['all_owned_command_descendants_settled']=descendants_settled
  if target.exists() and (not descendants_settled or not state['evidence_preserved_before_clean']):
   state['cargo_cleanup']='retained_requires_settlement_or_evidence_preservation';state['result']='failed'
  elif target.exists():
   try:
    run('required-neo-cargo-clean',['cargo','clean','--target-dir',target],600,development)
    state['cargo_cleanup']='completed'
   except BaseException as error:state['cargo_cleanup']='failed';state['cleanup_error']=str(error);state['result']='failed'
  else:state['cargo_cleanup']='no-compiler-target-created'
  state['compiler_tree_absent_or_empty']=not target.exists() or not any(target.iterdir())
  state['source_fixture_tree_preserved']=(source/'target').exists();save()
  (campaign/'phase.txt').write_text('complete-'+state['result']);print(json.dumps({'result':state['result'],'receipt':str(evidence/'batch-receipt.json'),'cargo_cleanup':state['cargo_cleanup']}),flush=True)
 return 0 if state['result']=='passed' else 1

if __name__=='__main__':raise SystemExit(main())
