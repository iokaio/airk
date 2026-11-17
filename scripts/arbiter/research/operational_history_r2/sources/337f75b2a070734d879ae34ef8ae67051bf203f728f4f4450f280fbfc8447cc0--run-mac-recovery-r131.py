# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess,time
from pathlib import Path
root=Path.cwd()
script=root/'artifacts/advanced-split/local-delivery-preparation-r1/root_only_mac_r4_batch.sh'
raw=script.read_bytes()
assert hashlib.sha256(raw).hexdigest()=='aede7b287825d64c030de22346c1994a440a99c8fa713df8ebe879ff8418427d'
connection=json.loads((root/'artifacts/v190/neo-connection.local.json').read_bytes())
command=['ssh','-p',str(connection['port']),'-i',connection['identity_path'],*connection['ssh_options'],connection['user']+'@'+connection['host'],'sh','-s']
out=root/'artifacts/advanced-split/receipts/mac-recovery-r131';out.mkdir(exist_ok=False)
started=time.time();tick=time.monotonic()
(out/'started.json').write_text(json.dumps({'schema':'airk.private-root-mac-recovery-launch/1','source':'Independent immutable MacR2 recovery draft, reviewed nativeeac7, original checkout preserved','script_sha256':hashlib.sha256(raw).hexdigest(),'started_unix':started,'command':'Existing authorized SSH -> POSIX sh -s; connection details remain private','scope':'Root sole Cargo owner on separate host; no competing local Cargo or model/resource load; scoped Clippy, six actual bridge tests and matched pair/model smoke, then retain evidence and clean exact isolated target. Not full platform qualification or publication.'},indent=2)+'\n',encoding='utf-8')
with (out/'remote.stdout.log').open('xb') as stdout,(out/'remote.stderr.log').open('xb') as stderr:
    result=subprocess.run(command,input=raw,stdout=stdout,stderr=stderr)
record={'schema':'airk.private-root-mac-recovery-ssh-result/1','exit_code':result.returncode,'started_unix':started,'elapsed_seconds':time.monotonic()-tick,'script_sha256':hashlib.sha256(raw).hexdigest(),'credentials_printed':False,'stdout':'remote.stdout.log','stderr':'remote.stderr.log'}
(out/'result.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
print(json.dumps(record))
raise SystemExit(result.returncode)
