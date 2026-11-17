# SPDX-License-Identifier: Apache-2.0
set -eu
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
