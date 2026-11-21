# SPDX-License-Identifier: Apache-2.0
"""Exercise an independently installed original Linux pair and its public device policy in one private XDG tmpfs session."""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import resource
import runpy
import shutil
import stat
import subprocess
import threading
import time

from configure_ort import BASE, CAMPAIGN, sha
import gnu_volume_policy
from gpu_opt_in_workflow import exercise
from product_projection import checked_path
from shipping_build import capacity, document, space
from unpack import guarded
from verified_inputs import identity

GIB = 1 << 30


def check(condition, detail):
    if not condition:
        raise ValueError(detail)


def private_temp(uid, attempt):
    runtime = Path(os.environ["XDG_RUNTIME_DIR"])
    check(runtime == Path("/run/user/1000") and runtime.resolve(strict=True) == runtime,"registered XDG runtime changed")
    # WSL publishes its per-login 0700 mount after starting the distro.
    # The visible /run/user/1000 directory can briefly precede that mount.
    deadline = time.monotonic() + 20
    while stat.S_IMODE(runtime.stat().st_mode) != 0o700 and time.monotonic() < deadline:
        time.sleep(0.1)
    guarded(runtime,uid,stat.S_ISDIR)
    check(stat.S_IMODE(runtime.stat().st_mode) == 0o700,"private XDG user mount unavailable at its startup deadline")
    mount = subprocess.check_output(["/usr/bin/findmnt","-T",str(runtime),"-rno","TARGET,FSTYPE,VFS-OPTIONS"],text=True).splitlines()
    check(len(mount) == 1 and mount[0].split()[:2] == [str(runtime),"tmpfs"] and "noexec" not in mount[0],
          "dedicated executable Linux user tmpfs required")
    root = runtime/("airk-cuda-service-journey-r"+str(attempt))
    root.mkdir(0o700)
    guarded(root,uid,stat.S_ISDIR)
    image = root/"exec-control"
    shutil.copyfile("/usr/bin/true",image)
    image.chmod(0o700)
    guarded(image,uid,stat.S_ISREG)
    check(sha(image) == sha(Path("/usr/bin/true")) and subprocess.run([str(image)],timeout=5).returncode == 0,
          "native XDG executable control failed")
    available = shutil.disk_usage(runtime).free
    check(available >= 4*GIB,"private runtime cannot fit the measured original installation")
    return root,mount,available


def run(args):
    uid = identity()
    os.umask(0o077)
    check(1 <= args.attempt <= 99,"invalid service attempt")
    build = document(args.build,args.build_sha256)
    source = document(CAMPAIGN/"linux-cuda-shipping-source-r2.json",build["source_receipt"])
    repo = checked_path(Path(source["source"]),True)
    runtime = checked_path(Path(build["output"]),True)
    check(build["success"] is True and runtime == BASE/"linux-cuda-descriptive-service-r3/runtime","original pair required")
    root,mount,available = private_temp(uid,args.attempt)
    state = BASE/("linux-product-service-control-r"+str(args.attempt))
    checked_path(BASE,True)
    state.mkdir(0o700)
    guarded(state,uid,stat.S_ISDIR)
    for name in ("home","config","tmp","source","data"):
        (root/name).mkdir(0o700)
    os.environ.update(HOME=str(root/"home"),XDG_CONFIG_HOME=str(root/"config"),TMPDIR=str(root/"tmp"),
                      AIRK_PRIVATE_CENSUS_BASE=str(root),PYTHONDONTWRITEBYTECODE="1",RAYON_NUM_THREADS="1",
                      TOKIO_WORKER_THREADS="2",LC_ALL="C")
    resource.setrlimit(resource.RLIMIT_AS,(8*GIB,8*GIB))
    os.sched_setaffinity(0,set(sorted(os.sched_getaffinity(0))[:2]))
    baseline = json.loads((CAMPAIGN/"linux-cuda-r4-provider-r3-space-watch.jsonl").read_text().splitlines()[0])
    journal = []
    samples = []
    finished = threading.Event()
    errors = []
    fits = lambda value: capacity(baseline,value)
    if gnu_volume_policy.active() is not None:
        # Started through volume_policy_entry.py: every total applies, and this journey reserves no storage.
        observer = gnu_volume_policy.active().guard(None,uid)
        fits = lambda value: observer(value,samples[0],False)
    def sample():
        value = space()
        value["tmpfs_used_since_start"] = max(0,available-shutil.disk_usage(root).free)
        value["swap_used_kib"] = sum(int(row.split()[3]) for row in Path("/proc/swaps").read_text().splitlines()[1:])
        samples.append(value)
        check(fits(value) and value["tmpfs_used_since_start"] <= 4*GIB and value["swap_used_kib"] == 0,
              "physical space, private tmpfs or zero-swap requirement exceeded")
    def monitor():
        while not finished.wait(1):
            try:
                sample()
            except (OSError,ValueError) as exc:
                errors.append(str(exc))
                return
    def record(name,value):
        check(not errors,"; ".join(errors))
        sample()
        journal.append({"operation":name,"value":value})
        with (state/(name+".json")).open("x") as f:
            json.dump(value,f,separators=(",",":"))
        return value
    client = repo/"scripts/check-service-clients.py"
    check(client.is_file() and not client.is_symlink(),"frozen managed-pair smoke owner required")
    owner = runpy.run_path(str(client))
    sample()
    observer = threading.Thread(target=monitor)
    observer.start()
    error = None
    try:
        @contextmanager
        def retain_native_refusal():
            with owner["Fixture"](runtime,root,preserve_pair_metadata=True) as active:
                try:
                    yield active
                except Exception as exc:
                    # Preserve the same authenticated live instance for a bounded diagnosis
                    # through its normal CLI. No replay or extension of the failed request.
                    with (state/"refusal.json").open("x") as out:
                        json.dump({"error":repr(exc),"control":str(active.control),"cli":str(active.cli),
                                   "image":str(active.binary),"mount":mount},out)
                    until = time.monotonic() + 180
                    release = state/"release"
                    while time.monotonic() < until and not errors:
                        if release.exists():
                            guarded(release,uid,stat.S_ISREG)
                            check(release.stat().st_size == 8 and release.read_bytes() == b"release\n","invalid release signal")
                            break
                        finished.wait(min(1,until-time.monotonic()))
                    raise
        with retain_native_refusal() as fixture:
            exercise(root,fixture,record,check)
        record("managed-stop",fixture.managed_stop)
    except Exception as exc:
        error=repr(exc)
        if "fixture" in locals() and fixture.managed_stop is not None:
            journal.append({"operation":"managed-stop-on-failure","value":fixture.managed_stop})
    finally:
        finished.set()
        observer.join()
        sample()
    logs={}
    for name in ("service.stderr","managed-start.json","managed-stop.json","cleanup-refusal.json"):
        inp=root/name
        if inp.is_file() and inp.stat().st_size <= 8<<20:
            logs[name] = inp.read_text(errors="replace")[-32768:]
    with (state/"volumes.json").open("x") as file:
        json.dump(samples,file)
    return {"success":error is None and not errors,"uid":uid,"tmpfs":str(root),"mount":mount,"error":error,
            "monitor_errors":errors,"journal":journal,"logs":logs,"original_build":args.build_sha256,
            "source_script_sha256":sha(client),"volume_samples":len(samples),"peak_tmpfs":max(s["tmpfs_used_since_start"] for s in samples),
            "peak_swap_kib":max(s["swap_used_kib"] for s in samples),"state":str(state),
            "volumes_sha256":sha(state/"volumes.json"),"first":samples[0],"last":samples[-1]}


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build",type=Path,required=True)
    parser.add_argument("--build-sha256",required=True)
    parser.add_argument("--attempt",type=int,required=True)
    print(json.dumps(run(parser.parse_args()),sort_keys=True))
