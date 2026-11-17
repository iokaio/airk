# SPDX-License-Identifier: Apache-2.0
"""Read-only R17 Health/same-file census after its recorded idle interval."""
import datetime,hashlib,importlib.util,json,pathlib,subprocess,time
base=pathlib.Path(__file__).resolve().parent
case=next(c for c in json.loads((base/'windows-debug-pre14-adoption-result-r17.json').read_text(encoding='utf-8-sig'))['cases'] if c.get('preserved_owner'))
fixture=pathlib.Path(case['preserved_owner']).resolve(strict=True);install=fixture/'install'
start=json.loads((fixture/'managed-start.json').read_text())
journal_path=install/'legacy-maintenance.json';original_journal=journal_path.read_bytes();journal=json.loads(original_journal)
assert journal['phase']=='restored' and journal['original_explicit_user_stop'] is False
assert time.time()*1000-journal['restoration']['observed_unix_ms']>=185000,'Recorded idle interval has not elapsed'
active=start['health']['runtime_sha256'];expected=start['health']['executable_sha256']
assert json.loads((install/'selected.json').read_text())['active']==active
exe=install/'runtimes'/active/'airk.exe'
def sha(path):
    with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
assert sha(exe)==expected and sha(exe.parent/'airk-install.json')==active
cache=json.loads((install/'legacy-cache-retirement.json').read_text())
assert cache['withdrawal_operation']==journal['operation']
original=pathlib.Path(case['inspection']['caches'][0]['executable'])
held=original.parent/('.airk-withheld-'+journal['operation'])
assert not original.exists() and sha(held)==case['original_helper_sha256']
out=base/'preserved-adoption-r17-after-idle.json';assert not out.exists()
before=time.monotonic()
p=subprocess.run([str(exe),'service','rpc','--runtime-dir',str(fixture/'state'/'control'),'--stdin'],input=json.dumps('health').encode(),capture_output=True,timeout=90,creationflags=subprocess.CREATE_NO_WINDOW)
assert len(p.stdout)+len(p.stderr)<=2*1024*1024
spec=importlib.util.spec_from_file_location('fixture_native_census',base/'native_process_image_census.py');mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod)
census=mod.census([str(exe),str(original),str(held)])
assert original_journal==journal_path.read_bytes(),'Maintenance journal changed during observation'
record={'schema':1,'fixture':str(fixture),'observed_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'maintenance_phase':journal['phase'],'operation':journal['operation'],'restoration':journal['restoration'],
        'active':active,'executable_sha256':expected,'health':{'exit_code':p.returncode,'elapsed_seconds':time.monotonic()-before,
        'stdout':p.stdout.decode('utf8',errors='replace'),'stderr':p.stderr.decode('utf8',errors='replace')},'census':census,
        'cache_pending':True,'source_archive_data_untouched':True}
out.write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record))
