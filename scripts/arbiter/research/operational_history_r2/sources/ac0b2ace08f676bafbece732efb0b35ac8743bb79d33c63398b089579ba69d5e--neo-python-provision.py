# SPDX-License-Identifier: Apache-2.0
import hashlib,io,json,subprocess,tarfile
from pathlib import Path
c=json.loads(Path('artifacts/v190/neo-connection.local.json').read_text());ssh=['ssh',*c['ssh_options'],'-i',c['identity_path'],'-p',str(c['port']),c['user']+'@'+c['host']]
archive=next(Path('artifacts/advanced-split/tools/neo-python-source').glob('*.gz'));names={str(archive).replace('\\','/'):'artifacts/tools/neo-python-3.13.18.tar.gz','artifacts/advanced-split/neo-python-isolated-tool.py':'artifacts/advanced-split/neo-python-isolated-tool.py'}
package=io.BytesIO()
with tarfile.open(fileobj=package,mode='w:gz') as t:
 for source,name in names.items():
  data=Path(source).read_bytes();item=tarfile.TarInfo(name);item.size=len(data);item.mode=0o644;t.addfile(item,io.BytesIO(data))
r=subprocess.run(ssh+['sh','-c',"'cd \"$HOME/code/airk\" && test ! -e artifacts/tools/neo-python-3.13.18 && test ! -e artifacts/tools/neo-python-3.13.18.tar.gz && mkdir -p artifacts/tools && tar -xzf -'"],input=package.getvalue(),capture_output=True,timeout=60)
if r.returncode:raise RuntimeError(r.stderr.decode())
remote='''set -eu
cd "$HOME/code/airk"
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 artifacts/advanced-split/neo-python-isolated-tool.py artifacts/tools/neo-python-3.13.18.tar.gz artifacts/tools/neo-python-3.13.18
PYTHONDONTWRITEBYTECODE=1 /usr/bin/python3 - <<'PY'
import importlib.util,json,os
from pathlib import Path
s=importlib.util.spec_from_file_location('core',Path('scripts/provision-static-onnx-runtime.py'));c=importlib.util.module_from_spec(s);s.loader.exec_module(c)
root=Path('artifacts/tools/neo-python-3.13.18').resolve();exe=root/'bin/python3.13'
script="""import sys,ssl,tomllib,hashlib,io,json,urllib.request
assert sys.version_info[:3]==(3,13,18)
assert tomllib.loads('answer=42')['answer']==42
assert hashlib.file_digest(io.BytesIO(b'abc'),'sha256').hexdigest()=='ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'
context=ssl.create_default_context()
assert context.verify_mode==ssl.CERT_REQUIRED and context.check_hostname
assert context.cert_store_stats()['x509_ca']>0
with urllib.request.urlopen('https://www.python.org/',context=context,timeout=10) as response:
 assert response.status==200
 response.read(1024)
print(json.dumps({'version':sys.version,'executable':sys.executable,'openssl':ssl.OPENSSL_VERSION,'certificate_roots':context.cert_store_stats(),'default_verify_paths':ssl.get_default_verify_paths()._asdict(),'https_certificate_verified':True,'tomllib':True,'file_digest':True}))
"""
environment={key:value for key,value in os.environ.items() if key in {'HOME','USER','LOGNAME','TMPDIR','LANG','LC_ALL'}};environment['PATH']='/usr/bin:/bin:/usr/sbin:/sbin'
log=Path('artifacts/advanced-split/receipts/neo-python-3.13.18-smoke.log').resolve();result=c.run_owned([str(exe),'-I','-B','-c',script],root,log,30,environment)
c.write_json(log.with_suffix('.json'),result)
print('SMOKE_RESULT',json.dumps(result));print('SMOKE_OUTPUT',log.read_text())
if not c.successful(result):raise SystemExit(1)
PY
'''
r=subprocess.run(ssh+['sh','-s'],input=remote.encode(),capture_output=True,timeout=90)
Path('artifacts/advanced-split/receipts/delivery-04-neo-python-provision.log').write_bytes(r.stdout+r.stderr)
Path('artifacts/advanced-split/receipts/delivery-04-neo-python-provision.json').write_text(json.dumps({'source_sha256':hashlib.sha256(Path('artifacts/advanced-split/neo-python-isolated-tool.py').read_bytes()).hexdigest(),'archive_sha256':'7a2384641584ab9984221fcb0fc9265df465922af71be71410529e2a9c3f0540','command':remote,'exit_code':r.returncode,'stdout':r.stdout.decode(),'stderr':r.stderr.decode()},indent=2),encoding='utf-8')
print(r.stdout.decode());print(r.stderr.decode());raise SystemExit(r.returncode)
