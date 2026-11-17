# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,importlib.util,json,os,subprocess,time
root=Path.cwd();a=root/'artifacts/optimizations';env=os.environ.copy();env.update(CARGO_BUILD_JOBS='4',CARGO_NET_OFFLINE='true',AIRK_BUNDLED_MODEL_DIR=str(root/'target/local-vector-model'),AIRK_BUNDLED_RUNTIME_DIR=str(root/'target/local-onnx-runtime'))
spec=importlib.util.spec_from_file_location('f10_builder',root/'scripts/build-service-runtime.py');builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
cargo='C:/Users/tdotn/.cargo/bin/cargo.exe';base=[cargo,'test','-p','airk-store','--lib','--locked','--offline'];commands=[('standalone-check',[cargo,'check','-p','airk-store','--lib','--no-default-features','--locked','--offline'])]
for kind,selector in [('integrity','integrity_'),('enrollment','native_metadata_enrollment_requires_exact_writer'),('private-storage','private_storage'),('windows-census','service_census_windows'),('relocation','installed_data_root_rename_preserves_publication_settings_pause_and_receipts'),('manifest-death','manifest_process_death_selects_complete_old_or_new_and_pins_retirement'),('full-removal','full_removal_preserves_sources_refuses_unknown_files_and_aliases_and_resumes_after_restart')]:commands.append((kind,base+[selector,'--','--nocapture']))
for kind,command in commands:
 record={'command':command,'source_before':builder.source_identity(),'environment':{k:env[k] for k in ['CARGO_BUILD_JOBS','CARGO_NET_OFFLINE','AIRK_BUNDLED_MODEL_DIR','AIRK_BUNDLED_RUNTIME_DIR']},'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()};start=time.monotonic()
 with (a/f'f10a-focused-{kind}.log').open('x') as out:r=subprocess.run(command,env=env,stdout=out,stderr=subprocess.STDOUT)
 record.update(exit_code=r.returncode,elapsed_seconds=time.monotonic()-start,source_after=builder.source_identity(),ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat());record['source_unchanged']=record['source_before']==record['source_after'];(a/f'f10a-focused-{kind}-execution.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps({'gate':kind,'exit_code':r.returncode,'elapsed':record['elapsed_seconds'],'source_unchanged':record['source_unchanged'],'source':record['source_before']}),flush=True)
 if r.returncode or not record['source_unchanged']:raise SystemExit(r.returncode or 1)
