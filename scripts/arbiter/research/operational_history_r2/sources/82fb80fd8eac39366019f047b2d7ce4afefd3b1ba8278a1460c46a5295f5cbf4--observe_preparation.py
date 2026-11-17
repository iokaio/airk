# SPDX-License-Identifier: Apache-2.0
"""Owned one-shot60s observer for already-reviewed private data preparation."""
import hashlib,importlib.util,json,os,sys,time
from pathlib import Path
sys.dont_write_bytecode=True
ROOT=Path.cwd();WORK=ROOT/'artifacts/advanced-split/context-action-facts-source-r1'
protocol_path=WORK/'protocol.private.json';protocol=json.loads(protocol_path.read_bytes())
paths=[protocol_path,WORK/'context_action_facts.py',WORK/'memory_runner.py',Path(__file__),ROOT/'tools/retrieval-train/training_recipe_audit.py',ROOT/'tools/retrieval-train/context_action_facts.py',ROOT/'scripts/provision-static-onnx-runtime.py',ROOT/'scripts/static-onnx-runtime-lock.json',ROOT/'artifacts/advanced-split/models/a1-next-experiment-blind-metadata-r3/audit.py',Path(sys.executable)]
paths.extend(ROOT/value['path'] for value in protocol['inputs'].values())
def digest(path):
    h=hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda:stream.read(262144),b''):h.update(block)
    return h.hexdigest()
before={str(path):digest(path) for path in paths}
for value in protocol['inputs'].values():
    path=ROOT/value['path'];assert path.stat().st_size==value['bytes'] and before[str(path)]==value['sha256']
assert before[str(WORK/'context_action_facts.py')]==protocol['producer_sha256']=='0911be58cda856e741bec6dec8d1b5763337096b889765060c9acaa82ee40a24'
assert before[str(protocol_path)]=='f7c20e6de1519fb05d6d06444b97a7911fc0b4aa5f31abd563fde8d21e26bc14'
owner_path=ROOT/'scripts/provision-static-onnx-runtime.py';spec=importlib.util.spec_from_file_location('existing_owned_data_observer',owner_path);owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
argv=[sys.executable,'-B',str(WORK/'memory_runner.py')];started=time.monotonic();result=None;failure=None
try:
    result=owner.run_owned(argv,ROOT,WORK/'preparation-owned-r1.log',60,{**os.environ,'PYTHONDONTWRITEBYTECODE':'1'})
except Exception as error:failure={'class':type(error).__name__,'message':str(error)}
after={str(path):digest(path) for path in paths}
receipt={'schema':'airk.private-context-action-data-run/1','argv':argv,'observer_seconds':60,'hard_process_committed_bytes':268435456,'source_and_inputs_before_sha256':before,'source_and_inputs_after_sha256':after,'source_and_inputs_unchanged':before==after,'owned_process':result,'failure':failure,'elapsed_seconds':time.monotonic()-started,'model_execution':False,'native_execution':False,'qualified_profiles':[]}
with (WORK/'preparation-owned-r1.json').open('x',encoding='utf-8') as stream:json.dump(receipt,stream,indent=2);stream.write('\n')
print(json.dumps(receipt,indent=2))
raise SystemExit(0 if result and owner.successful(result) and before==after and not failure else 1)
