# SPDX-License-Identifier: Apache-2.0
"""Explicit GPU-only static Cargo handoff; no CPU-validator fallback or GPU activation."""
from pathlib import Path
import hashlib
import importlib.util,json,os,subprocess,time,math
import sys

sys.dont_write_bytecode = True
STATIC_KEYS={'LIBONNXRUNTIME_NO_PKG_CONFIG','ORT_SKIP_DOWNLOAD','ORT_PREFER_DYNAMIC_LINK','ORT_LIB_LOCATION','ORT_LIB_PROFILE'}
FRESH_ENV_SCHEMA = "airk.cuda-static-environment/6"
FRESH_REQUEST_FIELDS = {"destination", "aggregate_receipt"}
FRESH_OWNER_LIMIT = 1 << 20
def _load(name):
 p=Path(__file__).with_name(name+'.py');s=importlib.util.spec_from_file_location(name.replace('-','_'),p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m

def environment(destination,build,aggregate):
 runtime=_load('cuda-static-runtime')
 projection=runtime.static_projection(build,aggregate)
 if projection['target']!=runtime.TARGET:raise ValueError('GPU static environment target differs')
 location=Path(destination)/'gpu-aggregate'
 if build.get('schema')=='airk.cuda-static-build-proof/3':
  output=build['new_aggregate'];expected=Path(destination)/'aggregate/onnxruntime.lib'
  if not expected.is_absolute() or Path(output['path'])!=expected or aggregate['output']!={'path':'aggregate/onnxruntime.lib','bytes':output['bytes'],'sha256':output['sha256']} or aggregate.get('receipt_sha256')!=build['build_receipt']['sha256']:raise ValueError('incremental environment requires actual fresh child aggregate')
  location=expected.parent
 if build.get('schema')==runtime.FRESH_BUILD_SCHEMA:
  root=Path(destination)
  if not root.is_absolute() or Path(build["build_receipt"]["path"])!=root/"fresh-compile.json":raise runtime.GpuProofUnavailable('fresh static environment destination differs')
 return {'LIBONNXRUNTIME_NO_PKG_CONFIG':'1','ORT_SKIP_DOWNLOAD':'1','ORT_PREFER_DYNAMIC_LINK':'0','ORT_LIB_LOCATION':str(location),'ORT_LIB_PROFILE':'Release'}

def _fresh_check(deadline):
    if type(deadline) not in (int, float) or not math.isfinite(deadline):
        raise ValueError("explicit finite fresh handoff verification deadline required")
    if time.monotonic() >= deadline:
        raise TimeoutError("original fresh handoff verification deadline expired")


def _fresh_owner(path, pin, name, core, runtime, deadline):
    if (not isinstance(pin, dict) or set(pin) != {"path", "bytes", "sha256"}
            or not isinstance(pin["path"], str) or Path(pin["path"]) != path
            or type(pin["bytes"]) is not int or not 0 < pin["bytes"] <= FRESH_OWNER_LIMIT
            or not runtime._sha256(pin["sha256"]) or core.plain(path, regular=True) != path):
        raise runtime.GpuProofUnavailable("fresh handoff owner pin differs: " + str(path))
    _fresh_check(deadline)
    with path.open("rb") as stream:
        raw = stream.read(pin["bytes"] + 1)
    if len(raw) != pin["bytes"] or hashlib.sha256(raw).hexdigest() != pin["sha256"]:
        raise runtime.GpuProofUnavailable("consumed fresh handoff owner bytes differ: " + str(path))
    _fresh_check(deadline)
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    exec(compile(raw, str(path), "exec"), module.__dict__)
    return module


def _fresh_bundle(destination, aggregate_pin, repo_root, deadline, runtime, core):
    _fresh_check(deadline)
    repo = Path(repo_root)
    if (not destination.is_absolute() or repo != runtime.ROOT or destination != core.plain(destination)
            or not isinstance(aggregate_pin, dict) or set(aggregate_pin) != {"path", "bytes", "sha256"}
            or not isinstance(aggregate_pin["path"], str)
            or Path(aggregate_pin["path"]) != destination / "fresh-aggregate.json"
            or type(aggregate_pin["bytes"]) is not int or not 0 < aggregate_pin["bytes"] <= 32 << 20
            or not runtime._sha256(aggregate_pin["sha256"])):
        raise runtime.GpuProofUnavailable("exact fresh aggregate FilePin and absolute destination required")
    owners_root = repo / "scripts/cuda-static-runtime/build"
    prospective = repo / "artifacts/v190/final-plan/cu18"
    owner_name = "fresh_windows_d.py" if destination == prospective else "fresh_verify.py"
    verifier = runtime.load(owners_root / owner_name, "fresh_handoff_verifier")
    _fresh_check(deadline)
    facts = verifier.verify_receipt(aggregate_pin, deadline)
    _fresh_check(deadline)
    if (facts["aggregate_receipt"] != aggregate_pin
            or Path(facts["configure_recipe"]["fresh_output"]) != destination
            or Path(facts["compile"]["build_root"]) != destination / "b/Release"
            or Path(facts["archive"]["path"]) != destination / "gpu-aggregate/onnxruntime.lib"):
        raise runtime.GpuProofUnavailable("verified fresh receipt or destination differs")
    checked_owners = facts["aggregate_recipe"]["owners"]
    pins = _fresh_owner(repo / "scripts/cuda-static-incremental.py", checked_owners["pin_reader"],
                        "fresh_handoff_pins", core, runtime, deadline)
    projection = _fresh_owner(owners_root / "fresh_projection.py", checked_owners["projection"],
                              "fresh_handoff_projection", core, runtime, deadline)
    _fresh_check(deadline)
    bundle = projection.project_bundle(facts, canonical=pins.canonical)
    _fresh_check(deadline)
    if (not isinstance(bundle, dict) or set(bundle) != {"build", "aggregate"}
            or bundle["build"].get("schema") != runtime.FRESH_BUILD_SCHEMA
            or bundle["build"].get("build_receipt") != facts["aggregate"]["compile_receipt"]
            or bundle["aggregate"].get("receipt_sha256") != aggregate_pin["sha256"]
            or bundle["aggregate"].get("output") != facts["aggregate"]["output"]
            or facts["archive"] != {**bundle["aggregate"]["output"], "path": str(destination / "gpu-aggregate/onnxruntime.lib")}):
        raise runtime.GpuProofUnavailable("verified fresh handoff proof or aggregate differs")
    return bundle


def _verify_fresh_handoff(value, path, expected_sha256, repo_root, deadline, runtime, core):
    _fresh_check(deadline)
    request = value["request"]
    if (not isinstance(request, dict) or set(request) != FRESH_REQUEST_FIELDS
            or not isinstance(request["destination"], str)):
        raise runtime.GpuProofUnavailable("exact fresh handoff request fields required")
    with path.open("rb") as stream:
        raw = stream.read(core.MAX_JSON + 1)
    if len(raw) > core.MAX_JSON or hashlib.sha256(raw).hexdigest() != expected_sha256:
        raise runtime.GpuProofUnavailable("consumed fresh handoff bytes differ")

    def unique_pairs(rows):
        result = {}
        for key, item in rows:
            if key in result:
                raise runtime.GpuProofUnavailable("duplicate fresh handoff field: " + key)
            result[key] = item
        return result

    if json.loads(raw, object_pairs_hook=unique_pairs) != value:
        raise runtime.GpuProofUnavailable("consumed fresh handoff fields differ")
    _fresh_check(deadline)
    destination = Path(request["destination"])
    bundle = _fresh_bundle(destination, request["aggregate_receipt"], repo_root, deadline, runtime, core)
    build, aggregate = bundle["build"], bundle["aggregate"]
    expected = environment(destination, build, aggregate)
    static_ort = runtime.static_projection(build, aggregate)
    _fresh_check(deadline)
    if value["build"] != build or value["aggregate"] != aggregate or value["environment"] != expected or set(expected) != STATIC_KEYS:
        raise runtime.GpuProofUnavailable("fresh GPU static handoff no longer matches actual build and aggregate")
    return {"environment": expected, "static_ort": static_ort, "gpu_build": build,
            "gpu_aggregate": aggregate, "activation_pins_available": False}


def verify_handoff(path,expected_handoff_sha256,repo_root,*,deadline=None):
 if deadline is not None and (type(deadline) not in (int,float) or not math.isfinite(deadline)):raise ValueError("finite original handoff deadline required")
 def check():
  if deadline is not None and time.monotonic()>=deadline:raise TimeoutError("original handoff verification deadline expired")
 check()
 runtime=_load('cuda-static-runtime');core=runtime.load(Path(repo_root)/'scripts/provision-static-onnx-runtime.py','cpu_owner_for_gpu_environment')
 check();path=core.plain(path,regular=True)
 if path.is_file() and path.stat().st_size>core.MAX_JSON:raise runtime.GpuProofUnavailable('oversized GPU environment handoff')
 check()
 if core.digest(path)!=expected_handoff_sha256:raise runtime.GpuProofUnavailable('reviewed GPU environment pin differs')
 check();value=core.read_json(path);check()
 if set(value)!={'schema','request','build','aggregate','environment','activation_pins_available'} or value['schema'] not in ('airk.cuda-static-environment/1','airk.cuda-static-environment/2','airk.cuda-static-environment/3','airk.cuda-static-environment/4','airk.cuda-static-environment/5',FRESH_ENV_SCHEMA) or value['activation_pins_available'] is not False:raise runtime.GpuProofUnavailable('GPU environment format/activation claim differs')
 if value['schema']==FRESH_ENV_SCHEMA:return _verify_fresh_handoff(value,path,expected_handoff_sha256,repo_root,deadline,runtime,core)
 aggregate_owner=_load('cuda-static-aggregate')
 request=value['request']
 destination=core.plain(request['destination'])
 if value['schema']=='airk.cuda-static-environment/1':
  if set(request)!={'destination','producer_file','lock_file','receipt_sha256'}:raise runtime.GpuProofUnavailable('GPU verifier request fields differ')
  build=runtime.verify_build(destination,request['producer_file'],request['lock_file'],request['receipt_sha256'],repo_root)
 elif value['schema']=='airk.cuda-static-environment/5':
  if set(request)!={'destination','incremental_root','incremental_proposal','receipt_sha256'}:raise runtime.GpuProofUnavailable('exact incremental handoff request fields differ')
  check();incremental_owner=_load('cuda-static-incremental');check()
  bundle=incremental_owner.verify_incremental_bundle(destination,request['incremental_root'],request['incremental_proposal'],request['receipt_sha256'],repo_root,**({'deadline':deadline} if deadline is not None else {}));check()
  if not isinstance(bundle,dict) or set(bundle)!={'build','aggregate'}:raise runtime.GpuProofUnavailable('exact incremental verified bundle required')
  build=bundle['build'];aggregate=bundle['aggregate']
  if build.get('schema')!='airk.cuda-static-build-proof/3' or build.get('build_receipt',{}).get('sha256')!=request['receipt_sha256']:raise runtime.GpuProofUnavailable('incremental verifier child identity differs')
 elif value['schema']=='airk.cuda-static-environment/4':
  if set(request)!={'destination','composite_root','composer_proposal','receipt_sha256'}:raise runtime.GpuProofUnavailable('exact checkpoint handoff request fields differ')
  build=_load('cuda-static-checkpointed').verify_checkpointed(destination,request['composite_root'],request['composer_proposal'],request['receipt_sha256'],repo_root)
 else:
  if set(request)!={'destination','continuation_root','continuation_proposal','receipt_sha256'}:raise runtime.GpuProofUnavailable('exact continuation handoff request fields differ')
  owner=_load('cuda-static-continuation-r12' if value['schema']=='airk.cuda-static-environment/3' else 'cuda-static-continuation');build=owner.verify_continuation(destination,request['continuation_root'],request['continuation_proposal'],request['receipt_sha256'],repo_root)
 check()
 if value['schema']!='airk.cuda-static-environment/5':aggregate=aggregate_owner.verify_aggregate(destination,build,core,**({'deadline':deadline} if deadline is not None else {}))
 check()
 expected=environment(destination,build,aggregate);check()
 if value['build']!=build or value['aggregate']!=aggregate or value['environment']!=expected or set(expected)!=STATIC_KEYS:raise runtime.GpuProofUnavailable('GPU static handoff no longer matches actual build/aggregate')
 return {'environment':expected,'static_ort':runtime.static_projection(build,aggregate),'gpu_build':build,'gpu_aggregate':aggregate,'activation_pins_available':False}

def handoff_value(destination,producer_file,lock_file,receipt_sha256,build,aggregate):
 return {'schema':'airk.cuda-static-environment/1','request':{'destination':str(destination),'producer_file':str(producer_file),'lock_file':str(lock_file),'receipt_sha256':receipt_sha256},'build':build,'aggregate':aggregate,'environment':environment(destination,build,aggregate),'activation_pins_available':False}

def continuation_handoff_value(destination,continuation,proposal,receipt_sha256,build,aggregate):
 return {'schema':'airk.cuda-static-environment/2','request':{'destination':str(destination),'continuation_root':str(continuation),'continuation_proposal':str(proposal),'receipt_sha256':receipt_sha256},'build':build,'aggregate':aggregate,'environment':environment(destination,build,aggregate),'activation_pins_available':False}

def r12_handoff_value(destination,continuation,proposal,receipt_sha256,build,aggregate):
 value=continuation_handoff_value(destination,continuation,proposal,receipt_sha256,build,aggregate);value['schema']='airk.cuda-static-environment/3';return value

def checkpoint_handoff_value(destination,composite_root,proposal,receipt_sha256,build,aggregate):
 return {"schema":"airk.cuda-static-environment/4","request":{"destination":str(destination),"composite_root":str(composite_root),"composer_proposal":str(proposal),"receipt_sha256":receipt_sha256},"build":build,"aggregate":aggregate,"environment":environment(destination,build,aggregate),"activation_pins_available":False}


def incremental_handoff_value(destination,incremental_root,proposal,receipt_sha256,build,aggregate):
 if build.get('schema')!='airk.cuda-static-build-proof/3' or build.get('build_receipt',{}).get('sha256')!=receipt_sha256:raise ValueError('incremental handoff requires the exact verified child proof')
 return {'schema':'airk.cuda-static-environment/5','request':{'destination':str(destination),'incremental_root':str(incremental_root),'incremental_proposal':str(proposal),'receipt_sha256':receipt_sha256},'build':build,'aggregate':aggregate,'environment':environment(destination,build,aggregate),'activation_pins_available':False}


def fresh_handoff_value(destination, aggregate_receipt, repo_root, *, deadline):
    """Build an in-memory handoff from one verification of the reviewed raw receipt."""
    _fresh_check(deadline)
    runtime = _load("cuda-static-runtime")
    repo = Path(repo_root)
    if repo != runtime.ROOT:
        raise runtime.GpuProofUnavailable("fresh handoff repository identity differs")
    core = runtime.load(repo / "scripts/provision-static-onnx-runtime.py", "cpu_owner_for_fresh_handoff")
    destination = Path(destination)
    bundle = _fresh_bundle(destination, aggregate_receipt, repo, deadline, runtime, core)
    build, aggregate = bundle["build"], bundle["aggregate"]
    expected = environment(destination, build, aggregate)
    _fresh_check(deadline)
    return {"schema": FRESH_ENV_SCHEMA,
            "request": {"destination": str(destination), "aggregate_receipt": dict(aggregate_receipt)},
            "build": build, "aggregate": aggregate, "environment": expected,
            "activation_pins_available": False}


def incremental_output_policy(build,role,path,core):
 if build.get('schema')!='airk.cuda-static-build-proof/3' or role not in ('provider_stage','environment_handoff'):raise ValueError('exact incremental output role required')
 recipe_pin=build['recipe'];recipe_path=core.plain(recipe_pin['path'],regular=True);core.verify_file(recipe_path,recipe_pin)
 policy=core.read_json(recipe_path)[role]
 if set(policy)!={'path','root','expected_sid','guard','powershell'}:raise ValueError('exact recipe-bound output policy required')
 path=core.plain(path);root=core.plain(policy['root'])
 if path!=Path(policy['path']) or path==root or not path.is_relative_to(root):raise ValueError('requested incremental output differs from approved destination')
 return policy

def audit_incremental_output(policy,path,core,created=False,timeout=10):
 if not 0<timeout<=10:raise ValueError('bounded incremental output audit required')
 for key in ['guard','powershell']:
  core.verify_file(core.plain(policy[key]['path'],regular=True),policy[key])
 def quote(value):return "'"+str(value).replace("'","''")+"'"
 function='Assert-AirkCreatedOutput' if created else 'Assert-AirkOutputParent'
 command="$ErrorActionPreference='Stop'; . "+quote(policy['guard']['path'])+"; "+function+" -Path "+quote(path)+" -Root "+quote(policy['root'])+" -ExpectedSid "+quote(policy['expected_sid'])+" | Out-Null"
 subprocess.run([policy['powershell']['path'],'-NoProfile','-Command',command],check=True,timeout=timeout)

def write_incremental_handoff(path,value,build,core):
 import time
 deadline=time.monotonic()+300
 policy=incremental_output_policy(build,'environment_handoff',path,core);path=core.plain(path)
 if path.exists():raise ValueError('existing incremental handoff retained')
 def audit(created=False):
  remaining=deadline-time.monotonic()
  if remaining<=0:raise TimeoutError('incremental handoff original consumer deadline expired')
  audit_incremental_output(policy,path,core,created,min(10,remaining))
  if time.monotonic()>=deadline:raise TimeoutError('incremental handoff original consumer deadline expired after guard')
 audit()
 with path.open('xb') as stream:stream.write((json.dumps(value,indent=2)+'\n').encode())
 audit(True)
 if time.monotonic()>=deadline:raise TimeoutError('incremental handoff original consumer deadline expired')
 return value
