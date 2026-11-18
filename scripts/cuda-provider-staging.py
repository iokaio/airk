# SPDX-License-Identifier: Apache-2.0
"""Private provider-only staging; incomplete/unapproved until native full closure."""
from pathlib import Path
import hashlib,importlib.util,json,math,shutil
import sys
import time

def load(name):
 p=Path(__file__).with_name(name+'.py');s=importlib.util.spec_from_file_location(name.replace('-','_'),p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m

def stage_providers(destination,build,aggregate,output,continuation,proposal,receipt_sha,repo_root):
 runtime=load('cuda-static-runtime');owner=load('cuda-static-continuation-r12');core=runtime.load(Path(repo_root)/'scripts/provision-static-onnx-runtime.py','cpu_owner_private_stage')
 # Refuse all mutation until both genuine completed producer and aggregate reopen.
 observed=owner.verify_continuation(destination,continuation,proposal,receipt_sha,repo_root);actual_aggregate=load('cuda-static-aggregate').verify_aggregate(Path(destination),observed,core)
 if observed!=build or actual_aggregate!=aggregate:raise ValueError('actual producer/aggregate changed before staging')
 recipe=runtime.load(Path(proposal)/'continue_cuda_runtime.py','r12_stage_helpers');request=core.read_json(Path(proposal)/'continuation-request.json');r8=runtime.load(request['recipe']['path'],'r8_stage_policy');lock=core.read_json(request['recipe_lock']['path']);output=core.plain(output)
 if output!=r8.PARENT/'gpu-provider-stage-r12':raise ValueError('exact fresh protected provider stage required')
 guard=dict(lock);guard['build_path']={'fresh_child':output.name};policy=r8.identity_and_parent(core,output,guard);output.mkdir()
 receipt={'schema':'airk.private.gpu-provider-stage/1','status':'RUNNING','qualification':False,'complete_cuda_runtime':False,'producer_proof_sha256':runtime.canonical(build),'aggregate_proof_sha256':runtime.canonical(aggregate),'output_policy':policy,'files':[]}
 try:
  sources=[(core.child(destination,row['path']),Path(row['path']).name,row) for row in build['provider_dlls']]
  sources.append((Path(build['build_receipt']['path']),'gpu-build-continuation-receipt.json',build['build_receipt']))
  if sorted(name for _,name,_ in sources)!=['gpu-build-continuation-receipt.json','onnxruntime_providers_cuda.dll','onnxruntime_providers_shared.dll']:raise ValueError('exact pair/private receipt roster differs')
  for source,name,row in sources:
   core.verify_file(core.plain(source,regular=True),row);target=output/name
   with source.open('rb') as src,target.open('xb') as dst:shutil.copyfileobj(src,dst,1024*1024)
   core.verify_file(core.plain(target,regular=True),row);receipt['files'].append({'path':name,'bytes':target.stat().st_size,'sha256':core.digest(target)})
  if owner.verify_continuation(destination,continuation,proposal,receipt_sha,repo_root)!=build:raise ValueError('actual producer drifted during staging')
  receipt['status']='PASS_PRIVATE_PROVIDER_COPY_ONLY_INCOMPLETE_UNAPPROVED'
 except BaseException as exc:receipt.update(status='FAILED_PRESERVED_NO_RETRY',error=type(exc).__name__+': '+str(exc));raise
 finally:
  with (output/'staging-receipt.json').open('xb') as f:f.write((json.dumps(receipt,indent=2)+'\n').encode())
 return receipt


def stage_checkpointed_providers(destination,build,aggregate,output,composite_root,proposal,receipt_sha,repo_root):
 runtime=load('cuda-static-runtime');owner=load('cuda-static-checkpointed');core=runtime.load(Path(repo_root)/'scripts/provision-static-onnx-runtime.py','cpu_owner_checkpoint_stage')
 observed=owner.verify_checkpointed(destination,composite_root,proposal,receipt_sha,repo_root);actual=load('cuda-static-aggregate').verify_aggregate(Path(destination),observed,core)
 if observed!=build or actual!=aggregate:raise ValueError('actual checkpoint producer/aggregate changed before staging')
 r12,_=owner.execution_context(build,core);request=core.read_json(r12/'continuation-request.json');r8=runtime.load(request['recipe']['path'],'r8_checkpoint_stage');lock=core.read_json(request['recipe_lock']['path']);output=core.plain(output)
 if output!=r8.PARENT/'gpu-provider-stage-checkpointed-r1':raise ValueError('exact fresh protected checkpoint provider stage required')
 guard=dict(lock);guard['build_path']={'fresh_child':output.name};policy=r8.identity_and_parent(core,output,guard);output.mkdir()
 receipt={'schema':'airk.private.gpu-provider-stage/2','status':'RUNNING','build_mode':'checkpointed-warm','qualification':False,'complete_cuda_runtime':False,'producer_proof_sha256':runtime.canonical(build),'aggregate_proof_sha256':runtime.canonical(aggregate),'output_policy':policy,'files':[]}
 try:
  sources=[(core.child(destination,row['path']),Path(row['path']).name,row) for row in build['provider_dlls']]+[(Path(build['build_receipt']['path']),'gpu-checkpointed-build-receipt.json',build['build_receipt'])]
  if sorted(name for _,name,_ in sources)!=['gpu-checkpointed-build-receipt.json','onnxruntime_providers_cuda.dll','onnxruntime_providers_shared.dll']:raise ValueError('exact checkpoint pair/private receipt roster differs')
  for source,name,row in sources:
   core.verify_file(core.plain(source,regular=True),row);target=output/name
   with source.open('rb') as src,target.open('xb') as dst:shutil.copyfileobj(src,dst,1024*1024)
   core.verify_file(core.plain(target,regular=True),row);receipt['files'].append({'path':name,'bytes':target.stat().st_size,'sha256':core.digest(target)})
  if owner.verify_checkpointed(destination,composite_root,proposal,receipt_sha,repo_root)!=build:raise ValueError('actual checkpoint producer changed during staging')
  receipt['status']='PASS_PRIVATE_PROVIDER_COPY_ONLY_INCOMPLETE_UNAPPROVED'
 except BaseException as exc:receipt.update(status='FAILED_PRESERVED_NO_RETRY',error=type(exc).__name__+': '+str(exc));raise
 finally:
  with (output/'staging-receipt.json').open('xb') as f:f.write((json.dumps(receipt,indent=2)+'\n').encode())
 return receipt


def stage_incremental_providers(destination,build,aggregate,output,incremental_root,proposal,receipt_sha,repo_root):
 import time
 deadline=time.monotonic()+300
 runtime=load('cuda-static-runtime');owner=load('cuda-static-incremental');guards=load('cuda-runtime-environment');core=runtime.load(Path(repo_root)/'scripts/provision-static-onnx-runtime.py','cpu_owner_incremental_stage')
 expected = {'build': build, 'aggregate': aggregate}
 observed = owner.verify_incremental_bundle(
  destination, incremental_root, proposal, receipt_sha, repo_root, deadline=deadline)
 if observed != expected:
  raise ValueError('actual incremental producer/aggregate changed before staging')
 output=core.plain(output);policy=guards.incremental_output_policy(build,'provider_stage',output,core)
 recipe_path=core.plain(build['recipe']['path'],regular=True);core.verify_file(recipe_path,build['recipe'])
 provider_base=core.plain(core.read_json(recipe_path)['parent_context']['destination'])
 if output.exists():raise ValueError('existing incremental provider stage retained')
 def audit(path,created=False):
  remaining=deadline-time.monotonic()
  if remaining<=0:raise TimeoutError('incremental staging original consumer deadline expired')
  guards.audit_incremental_output(policy,path,core,created,min(10,remaining))
  if time.monotonic()>=deadline:raise TimeoutError('incremental staging original consumer deadline expired after guard')
 sources=[(core.child(provider_base,row['path']),Path(row['path']).name,row) for row in build['provider_dlls']]+[(Path(build['build_receipt']['path']),'gpu-incremental-build-receipt.json',build['build_receipt'])]
 if sorted(name for _,name,_ in sources)!=['gpu-incremental-build-receipt.json','onnxruntime_providers_cuda.dll','onnxruntime_providers_shared.dll']:raise ValueError('exact incremental pair/private child receipt roster differs')
 audit(output);output.mkdir();audit(output,True)
 receipt={'schema':'airk.private.gpu-provider-stage/3','status':'RUNNING','build_mode':'incremental-session-collector','qualification':False,'complete_cuda_runtime':False,'producer_proof_sha256':runtime.canonical(build),'aggregate_proof_sha256':runtime.canonical(aggregate),'output_policy':policy,'consumer_deadline_seconds':300,'files':[]}
 primary_error=None
 try:
  for source,name,row in sources:
   core.verify_file(core.plain(source,regular=True),row);target=output/name;audit(target)
   with source.open('rb') as src,target.open('xb') as dst:shutil.copyfileobj(src,dst,1024*1024)
   audit(target,True);core.verify_file(core.plain(target,regular=True),row);core.verify_file(core.plain(source,regular=True),row)
   receipt['files'].append({'path':name,'bytes':target.stat().st_size,'sha256':core.digest(target)})
  observed = owner.verify_incremental_bundle(
   destination, incremental_root, proposal, receipt_sha, repo_root, deadline=deadline)
  if observed != expected:
   raise ValueError('incremental provenance changed during provider staging')
  core.verify_file(core.plain(build['recipe']['path'],regular=True),build['recipe'])
  if time.monotonic()>=deadline:raise TimeoutError('incremental staging original consumer deadline expired')
  receipt['status']='PASS_PRIVATE_PROVIDER_COPY_ONLY_INCOMPLETE_UNAPPROVED'
 except BaseException as exc:
  primary_error=exc;receipt.update(status='FAILED_PRESERVED_NO_RETRY',error=type(exc).__name__+': '+str(exc));raise
 finally:
  try:
   target=output/'staging-receipt.json';audit(target)
   with target.open('xb') as stream:stream.write((json.dumps(receipt,indent=2)+'\n').encode())
   audit(target,True)
   if time.monotonic()>=deadline:raise TimeoutError('incremental staging original consumer deadline expired')
  except BaseException as retention_error:
   if primary_error is None:raise
   primary_error.add_note('Secondary staging receipt retention failure: '+type(retention_error).__name__+': '+str(retention_error))
 return receipt


FRESH_STAGE_LEAF = "gpu-provider-stage"
FRESH_PROVIDER_LEAVES = ("onnxruntime_providers_shared.dll", "onnxruntime_providers_cuda.dll")
FRESH_STAGE_IO_CHUNK = 1 << 20


def _fresh_stage_pin(pin, path, runtime, *, maximum=None):
    if (not isinstance(pin, dict) or set(pin) != {"path", "bytes", "sha256"}
            or not isinstance(pin["path"], str) or Path(pin["path"]) != path
            or type(pin["bytes"]) is not int or pin["bytes"] <= 0
            or (maximum is not None and pin["bytes"] > maximum)
            or not runtime._sha256(pin["sha256"])):
        raise ValueError("exact fresh provider stage FilePin differs: " + str(path))
    return pin


def _fresh_stage_recipe(pin, destination, repo, runtime, core, check):
    path = Path(pin["path"])
    if not path.is_absolute() or core.plain(path, regular=True) != path:
        raise ValueError("absolute unaliased fresh configure recipe required")
    _fresh_stage_pin(pin, path, runtime, maximum=32 << 20)
    check()
    with path.open("rb") as stream:
        raw = stream.read(pin["bytes"] + 1)
    if len(raw) != pin["bytes"] or hashlib.sha256(raw).hexdigest() != pin["sha256"]:
        raise ValueError("authenticated fresh configure recipe bytes differ")
    recipe = json.loads(raw)
    if (recipe["schema"] != "airk.private.cuda-fresh-configure-recipe/1"
            or Path(recipe["repository_root"]) != repo or Path(recipe["fresh_output"]) != destination
            or recipe["execution_authorized"] is not True or recipe["qualification"] is not False):
        raise ValueError("fresh provider stage policy differs from the verified build")
    check()
    return recipe


def _fresh_stage_bytes(path, pin, core, check, copy_to=None):
    check()
    if core.plain(path, regular=True) != path:
        raise ValueError("fresh provider stage path differs: " + str(path))
    remaining = pin["bytes"]
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while remaining:
            check()
            chunk = stream.read(min(FRESH_STAGE_IO_CHUNK, remaining))
            if not chunk:
                raise ValueError("truncated fresh provider stage file: " + str(path))
            check()
            digest.update(chunk)
            if copy_to is not None:
                copy_to.write(chunk)
            remaining -= len(chunk)
        check()
        if stream.read(1) or digest.hexdigest() != pin["sha256"]:
            raise ValueError("fresh provider stage file bytes differ: " + str(path))
    check()


def _fresh_stage_write(target, audit, write):
    audit(target)
    stream = target.open("xb")
    try:
        with stream:
            write(stream)
    except BaseException as primary:
        try:
            audit(target, True)
        except BaseException as secondary:
            primary.add_note("Failed to audit retained stage output: " + str(secondary))
        raise
    audit(target, True)


def stage_fresh_providers(destination, aggregate_receipt, output, repo_root, *, deadline):
    """Stage original current-toolchain provider and producer bytes under one deadline."""
    if type(deadline) not in (int, float) or not math.isfinite(deadline) or time.monotonic() >= deadline:
        raise ValueError("explicit unexpired fresh provider staging deadline required")
    sys.dont_write_bytecode = True
    runtime = load("cuda-static-runtime")
    env = load("cuda-runtime-environment")
    destination, output, repo = Path(destination), Path(output), Path(repo_root)
    if repo != runtime.ROOT or not destination.is_absolute() or output != destination / FRESH_STAGE_LEAF:
        raise ValueError("exact fresh provider stage direct child required")
    core = runtime.load(repo / "scripts/provision-static-onnx-runtime.py", "cpu_owner_fresh_provider_stage")

    def check():
        env._fresh_check(deadline)

    check()
    if core.plain(output) != output or output.exists():
        raise ValueError("existing or aliased fresh provider stage refused")
    expected_aggregate = destination / "fresh-aggregate.json"
    _fresh_stage_pin(aggregate_receipt, expected_aggregate, runtime, maximum=32 << 20)
    observed = env.fresh_handoff_value(destination, aggregate_receipt, repo, deadline=deadline)
    check()
    build, aggregate = observed["build"], observed["aggregate"]
    if (observed["schema"] != env.FRESH_ENV_SCHEMA or observed["activation_pins_available"] is not False
            or observed["request"] != {"destination": str(destination), "aggregate_receipt": aggregate_receipt}
            or build["schema"] != runtime.FRESH_BUILD_SCHEMA or build["qualification"] is not False
            or aggregate["qualification"] is not False or aggregate["receipt_sha256"] != aggregate_receipt["sha256"]):
        raise ValueError("verified fresh unqualified provider staging lineage required")
    recipe_pin = build["configure_recipe"]
    recipe = _fresh_stage_recipe(recipe_pin, destination, repo, runtime, core, check)
    guard_source = repo / "scripts/cuda-component-execution.py"
    owner = env._fresh_owner(guard_source, recipe["owners"]["guards"], "fresh_provider_stage_guard",
                             core, runtime, deadline)

    def audit(path, created=False):
        check()
        owner.guard(recipe, path, created, deadline=deadline)
        check()

    def audit_sources(paths):
        check()
        owner.guard_many(recipe, paths, True, deadline=deadline)
        check()

    provider_paths = [destination / "b/Release" / name for name in FRESH_PROVIDER_LEAVES]
    providers = build["provider_dlls"]
    if not isinstance(providers, list) or len(providers) != len(provider_paths):
        raise ValueError("exact same-build provider pair required")
    producer = destination / "fresh-compile.json"
    sources = [(producer, _fresh_stage_pin(build["build_receipt"], producer, runtime, maximum=32 << 20))]
    sources.extend((path, _fresh_stage_pin(pin, path, runtime)) for path, pin in zip(provider_paths, providers))
    source_paths = [path for path, _ in sources]
    audit_sources([expected_aggregate, *source_paths])
    for source, pin in sources:
        _fresh_stage_bytes(source, pin, core, check)

    receipt = {
        "schema": "airk.private.gpu-provider-stage/4", "status": "RUNNING",
        "build_mode": "fresh-current-toolchain", "qualification": False,
        "complete_cuda_runtime": False, "activation_pins_available": False,
        "build_receipt": build["build_receipt"], "aggregate_receipt": aggregate_receipt,
        "producer_proof_sha256": runtime.canonical(build), "aggregate_proof_sha256": runtime.canonical(aggregate),
        "output_policy": {"path": str(output), "root": str(destination), "expected_sid": owner.SID,
                          "guard": recipe["checked_guard"], "powershell": recipe["powershell"]},
        "files": [],
    }
    audit(output)
    output.mkdir()
    audit(output, True)
    primary_error = None
    try:
        for source, pin in sources:
            target = output / source.name
            _fresh_stage_write(target, audit, lambda stream: _fresh_stage_bytes(source, pin, core, check, stream))
            _fresh_stage_bytes(target, pin, core, check)
            receipt["files"].append({**pin, "path": source.name})
        audit_sources([expected_aggregate, *source_paths])
        for source, pin in [(expected_aggregate, aggregate_receipt), *sources]:
            _fresh_stage_bytes(source, pin, core, check)
        _fresh_stage_recipe(recipe_pin, destination, repo, runtime, core, check)
        receipt["status"] = "PASS_PRIVATE_PROVIDER_COPY_ONLY_INCOMPLETE_UNAPPROVED"
    except BaseException as exc:
        primary_error = exc
        receipt.update(status="FAILED_PRESERVED_NO_RETRY", error=type(exc).__name__ + ": " + str(exc))
        raise
    finally:
        try:
            data = (json.dumps(receipt, indent=2, allow_nan=False) + "\n").encode("utf-8")
            _fresh_stage_write(output / "staging-receipt.json", audit, lambda stream: stream.write(data))
        except BaseException as retention_error:
            if primary_error is None:
                raise
            primary_error.add_note("Secondary staging receipt retention failure: " + str(retention_error))
    return receipt
