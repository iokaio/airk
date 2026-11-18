# SPDX-License-Identifier: Apache-2.0
"""Explicit separately-owned GPU producer/aggregate/handoff command. No model/publish.
Requires an actual Root-reviewed completed receipt; missing evidence never falls back
through the CPU owner. This is a private proposal until reviewed/applied.
"""
import argparse,hashlib,importlib.util,json,math
from pathlib import Path
import sys
import time

FRESH_RECEIPT_LIMIT = 32 << 20
LEGACY_INPUTS = (
    "producer_file", "lock_file", "continuation_root", "continuation_proposal",
    "r12_continuation_root", "r12_proposal", "checkpointed_root", "checkpointed_proposal",
    "incremental_root", "incremental_proposal",
)

def load(name):
 p=Path(__file__).with_name(name+'.py');s=importlib.util.spec_from_file_location(name.replace('-','_'),p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m

def _fresh_output_recipe(pin, destination, repo, runtime, core, env, deadline):
    if (not isinstance(pin, dict) or set(pin) != {"path", "bytes", "sha256"}
            or not isinstance(pin["path"], str) or not Path(pin["path"]).is_absolute()
            or type(pin["bytes"]) is not int or not 0 < pin["bytes"] <= FRESH_RECEIPT_LIMIT
            or not runtime._sha256(pin["sha256"])):
        raise ValueError("verified fresh configure recipe FilePin required")
    path = core.plain(pin["path"], regular=True)
    env._fresh_check(deadline)
    with path.open("rb") as stream:
        raw = stream.read(pin["bytes"] + 1)
    if len(raw) != pin["bytes"] or hashlib.sha256(raw).hexdigest() != pin["sha256"]:
        raise ValueError("authenticated fresh configure recipe bytes differ")
    recipe = json.loads(raw)
    if (recipe["schema"] != "airk.private.cuda-fresh-configure-recipe/1"
            or Path(recipe["repository_root"]) != repo or Path(recipe["fresh_output"]) != destination
            or recipe["execution_authorized"] is not True or recipe["qualification"] is not False):
        raise ValueError("fresh environment output policy differs from verified build")
    env._fresh_check(deadline)
    return recipe


def _write_fresh_environment(target, destination, repo, result, data, runtime, core, env, deadline):
    if not target.is_absolute() or target.parent != destination or core.plain(target) != target:
        raise ValueError("fresh environment output must be a direct child of the verified build root")
    recipe = _fresh_output_recipe(result["build"]["configure_recipe"], destination, repo,
                                  runtime, core, env, deadline)
    guard_path = repo / "scripts/cuda-component-execution.py"
    owner = env._fresh_owner(guard_path, recipe["owners"]["guards"], "fresh_handoff_output_guard",
                             core, runtime, deadline)
    owner.guard(recipe, target, deadline=deadline)
    env._fresh_check(deadline)
    stream = target.open("xb")
    try:
        with stream:
            stream.write(data)
    except Exception as failure:
        try:
            owner.guard(recipe, target, True, deadline=deadline)
        except Exception as audit:
            failure.add_note("Failed to audit retained environment output: " + str(audit))
        raise
    owner.guard(recipe, target, True, deadline=deadline)
    env._fresh_check(deadline)


def _provision_fresh(args):
    began = time.monotonic()
    if any(getattr(args, field) is not None for field in LEGACY_INPUTS):
        raise ValueError("fresh aggregate receipt is mutually exclusive with legacy producer inputs")
    if args.aggregate or args.stage_providers is not None:
        raise ValueError("fresh handoff forbids aggregate relink and provider staging")
    duration = args.verification_seconds
    if duration is None or not math.isfinite(duration) or duration <= 0:
        raise ValueError("finite positive --verification-seconds required for fresh handoff")
    deadline = began + duration
    if not math.isfinite(deadline) or deadline <= began:
        raise ValueError("fresh handoff verification deadline is invalid")

    sys.dont_write_bytecode = True
    runtime = load("cuda-static-runtime")
    env = load("cuda-runtime-environment")
    destination, receipt, repo = args.destination, args.fresh_aggregate_receipt, args.repo_root
    if (repo != runtime.ROOT or not destination.is_absolute() or receipt != destination / "fresh-aggregate.json"
            or not runtime._sha256(args.receipt_sha256)):
        raise ValueError("absolute fresh build, exact receipt path and reviewed SHA256 required")
    env._fresh_check(deadline)
    core = runtime.load(repo / "scripts/provision-static-onnx-runtime.py", "cpu_acquisition_owner_fresh_cli")
    if core.plain(destination) != destination or core.plain(receipt, regular=True) != receipt:
        raise ValueError("fresh build or aggregate receipt path differs")
    observed_size = receipt.stat().st_size
    if not 0 < observed_size <= FRESH_RECEIPT_LIMIT:
        raise ValueError("fresh aggregate receipt size exceeds its bound")
    aggregate_pin = {"path": str(receipt), "bytes": observed_size, "sha256": args.receipt_sha256}
    env._fresh_check(deadline)
    result = env.fresh_handoff_value(destination, aggregate_pin, repo, deadline=deadline)
    env._fresh_check(deadline)
    text = json.dumps(result, indent=2, allow_nan=False)
    data = (text + "\n").encode("utf-8")
    if len(data) > core.MAX_JSON:
        raise ValueError("fresh environment handoff exceeds its JSON bound")
    env._fresh_check(deadline)
    if args.write_environment is not None:
        _write_fresh_environment(args.write_environment, destination, repo, result, data, runtime, core, env, deadline)
    print(text)


def main():
 ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--repo-root',type=Path,default=Path(__file__).resolve().parents[1]);ap.add_argument('--destination',type=Path,required=True);ap.add_argument('--producer-file',type=Path);ap.add_argument('--lock-file',type=Path);ap.add_argument('--continuation-root',type=Path);ap.add_argument('--continuation-proposal',type=Path);ap.add_argument('--r12-continuation-root',type=Path);ap.add_argument('--r12-proposal',type=Path);ap.add_argument('--checkpointed-root',type=Path);ap.add_argument('--checkpointed-proposal',type=Path);ap.add_argument('--incremental-root',type=Path);ap.add_argument('--incremental-proposal',type=Path);ap.add_argument('--stage-providers',type=Path);ap.add_argument('--receipt-sha256',required=True);ap.add_argument('--aggregate',action='store_true');ap.add_argument('--write-environment',type=Path)
 ap.add_argument('--fresh-aggregate-receipt',type=Path)
 ap.add_argument('--verification-seconds',type=float)
 a=ap.parse_args()
 if a.fresh_aggregate_receipt is not None:return _provision_fresh(a)
 if a.verification_seconds is not None:raise ValueError('--verification-seconds requires --fresh-aggregate-receipt')
 runtime=load('cuda-static-runtime');aggregate=load('cuda-static-aggregate');env=load('cuda-runtime-environment');o=runtime.load(a.repo_root/'scripts/provision-static-onnx-runtime.py','cpu_acquisition_owner');destination=o.plain(a.destination)
 if a.incremental_root is not None or a.incremental_proposal is not None:
  if a.incremental_root is None or a.incremental_proposal is None or any(x is not None for x in [a.checkpointed_root,a.checkpointed_proposal,a.r12_continuation_root,a.r12_proposal,a.continuation_root,a.continuation_proposal,a.producer_file,a.lock_file]):raise ValueError('explicit complete incremental mode required')
  if a.aggregate:
   raise ValueError('incremental aggregate is already producer-owned; consumer relink refused')
  bundle = load('cuda-static-incremental').verify_incremental_bundle(
   destination, a.incremental_root, a.incremental_proposal, a.receipt_sha256, a.repo_root)
  build, proof = bundle['build'], bundle['aggregate']
 elif a.checkpointed_root is not None or a.checkpointed_proposal is not None:
  if a.checkpointed_root is None or a.checkpointed_proposal is None or any(x is not None for x in [a.r12_continuation_root,a.r12_proposal,a.continuation_root,a.continuation_proposal,a.producer_file,a.lock_file]):raise ValueError('explicit complete checkpointed mode required')
  build=load('cuda-static-checkpointed').verify_checkpointed(destination,a.checkpointed_root,a.checkpointed_proposal,a.receipt_sha256,a.repo_root)
 elif a.r12_continuation_root is not None or a.r12_proposal is not None:
  if a.r12_continuation_root is None or a.r12_proposal is None or any(x is not None for x in [a.continuation_root,a.continuation_proposal,a.producer_file,a.lock_file]):raise ValueError('explicit complete R12 mode required')
  build=load('cuda-static-continuation-r12').verify_continuation(destination,a.r12_continuation_root,a.r12_proposal,a.receipt_sha256,a.repo_root)
 elif a.continuation_root is not None or a.continuation_proposal is not None:
  if a.continuation_root is None or a.continuation_proposal is None or a.producer_file is not None or a.lock_file is not None:raise ValueError('explicit complete continuation mode required')
  build=load('cuda-static-continuation').verify_continuation(destination,a.continuation_root,a.continuation_proposal,a.receipt_sha256,a.repo_root)
 else:
  if a.producer_file is None or a.lock_file is None:raise ValueError('explicit original GPU producer inputs required')
  build=runtime.verify_build(destination,a.producer_file,a.lock_file,a.receipt_sha256,a.repo_root)
 if a.incremental_root is None:
  if a.aggregate:
   if (destination/'gpu-static-aggregate-receipt.json').exists():proof=aggregate.verify_aggregate(destination,build,o)
   else:proof=aggregate.aggregate(destination,build,o)
  else:proof=aggregate.verify_aggregate(destination,build,o)
 result=env.incremental_handoff_value(destination,a.incremental_root,a.incremental_proposal,a.receipt_sha256,build,proof) if a.incremental_root is not None else env.checkpoint_handoff_value(destination,a.checkpointed_root,a.checkpointed_proposal,a.receipt_sha256,build,proof) if a.checkpointed_root is not None else env.r12_handoff_value(destination,a.r12_continuation_root,a.r12_proposal,a.receipt_sha256,build,proof) if a.r12_continuation_root is not None else env.continuation_handoff_value(destination,a.continuation_root,a.continuation_proposal,a.receipt_sha256,build,proof) if a.continuation_root is not None else env.handoff_value(destination,a.producer_file,a.lock_file,a.receipt_sha256,build,proof)
 if a.stage_providers:
  if a.incremental_root is not None:load('cuda-provider-staging').stage_incremental_providers(destination,build,proof,a.stage_providers,a.incremental_root,a.incremental_proposal,a.receipt_sha256,a.repo_root)
  elif a.checkpointed_root is not None:load('cuda-provider-staging').stage_checkpointed_providers(destination,build,proof,a.stage_providers,a.checkpointed_root,a.checkpointed_proposal,a.receipt_sha256,a.repo_root)
  elif a.r12_continuation_root is None:raise ValueError('provider staging only enabled for exact R12 proof')
  else:load('cuda-provider-staging').stage_providers(destination,build,proof,a.stage_providers,a.r12_continuation_root,a.r12_proposal,a.receipt_sha256,a.repo_root)
 if a.write_environment:
  target=o.plain(a.write_environment)
  if target.exists():raise ValueError('existing GPU environment retained; no overwrite')
  if a.incremental_root is not None:env.write_incremental_handoff(target,result,build,o)
  else:
   target.parent.mkdir(parents=True,exist_ok=True)
   with target.open('xb') as f:f.write((json.dumps(result,indent=2)+'\n').encode())
 print(json.dumps(result,indent=2))
if __name__=='__main__':main()
