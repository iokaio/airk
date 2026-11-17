# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import subprocess,sys,json,time,importlib.util
root=Path.cwd();a=root/'artifacts/optimizations';command=[sys.executable,'-m','unittest','discover','-s','scripts/tests','-p','test_service_*.py'];start=time.monotonic()
with (a/'f12a-pool-harness-python.log').open('x') as out:r=subprocess.run(command,stdout=out,stderr=subprocess.STDOUT)
spec=importlib.util.spec_from_file_location('f12_py_builder',root/'scripts/build-service-runtime.py');b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b)
record={'command':command,'exit_code':r.returncode,'elapsed_seconds':time.monotonic()-start,'native_source':b.source_identity()};(a/'f12a-pool-harness-python-execution.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record));raise SystemExit(r.returncode)
