# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import datetime,importlib.util,json,subprocess,sys,time
root=Path.cwd();a=root/'artifacts/optimizations';spec=importlib.util.spec_from_file_location('f12_full_py_builder',root/'scripts/build-service-runtime.py');b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b)
command=[sys.executable,'-m','unittest','discover','-s','scripts/tests','-v'];record={'command':command,'source_before':b.source_identity(),'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()};start=time.monotonic()
with (a/'f12a-full-python.log').open('x') as out:r=subprocess.run(command,stdout=out,stderr=subprocess.STDOUT)
record.update(exit_code=r.returncode,elapsed_seconds=time.monotonic()-start,source_after=b.source_identity(),ended_utc=datetime.datetime.now(datetime.timezone.utc).isoformat());record['source_unchanged']=record['source_before']==record['source_after'];(a/'f12a-full-python-execution.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record));raise SystemExit(r.returncode)
