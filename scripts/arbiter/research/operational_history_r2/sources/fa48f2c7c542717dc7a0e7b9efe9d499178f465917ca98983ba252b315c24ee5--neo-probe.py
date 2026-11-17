# SPDX-License-Identifier: Apache-2.0
"""Read-only, bounded Neo prerequisite probe; never builds or prints SSH config."""
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
connection = json.loads((ROOT / 'artifacts/v190/neo-connection.local.json').read_text(encoding='utf-8-sig'))
ssh = ['ssh', '-i', connection['identity_path'], *connection['ssh_options'], connection['user'] + '@' + connection['host']]
remote = r'''
import hashlib,json,os,platform,shutil,subprocess
from pathlib import Path
root=Path.home()/'airk-builds'
paths=[root/'timing-1.6.4.PutU1P/tools/python3.13',root/'timing-1.6.4.PutU1P/tools/cargo-nextest',
 root/'one-airk-static-ort-1814-1794087059/static-environment-recovery.json',
 root/'cargo-clean-69d305e3933f/inputs/local-vector-model/minilm-l6-v2-s8-v1/model.json']
rows=[]
for path in paths:
 row={'path':str(path),'file':path.is_file(),'symlink':path.is_symlink()}
 if path.is_file():
  row.update(bytes=path.stat().st_size,sha256=hashlib.sha256(path.read_bytes()).hexdigest())
  if path.name=='static-environment-recovery.json':
   data=json.loads(path.read_text()); location=Path(data['environment']['ORT_LIB_LOCATION'])
   row['static_libraries_directory_exists']=location.is_dir()
 rows.append(row)
cargo=Path.home()/'.cargo/bin/cargo'
print(json.dumps({'platform':platform.platform(),'machine':platform.machine(),'uid':os.getuid(),
 'free_bytes':shutil.disk_usage(root).free,'paths':rows,
 'cargo_version':subprocess.check_output([str(cargo),'--version'],text=True).strip()}))
'''
result = subprocess.run([*ssh, "python3 - <<'AIRK_READ_ONLY_PROBE'\n" + remote + '\nAIRK_READ_ONLY_PROBE'],
                        capture_output=True, text=True, timeout=60)
if result.returncode:
    raise SystemExit('Read-only Neo probe failed; exit ' + str(result.returncode))
observed = json.loads(result.stdout)
destination = Path(__file__).with_name('neo-prerequisite-probe.json')
with destination.open('x', encoding='utf-8') as output:
    json.dump(observed, output, indent=2)
    output.write('\n')
print(json.dumps({'probe_only': True, 'receipt': str(destination), 'machine': observed['machine'],
                  'free_bytes': observed['free_bytes'], 'prerequisites': [{k: row[k] for k in ('file', 'symlink')} for row in observed['paths']]}))
