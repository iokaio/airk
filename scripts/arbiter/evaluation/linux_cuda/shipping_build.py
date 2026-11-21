# SPDX-License-Identifier: Apache-2.0
"""Run the standard local Linux CUDA service pair builder with the frozen inputs and physical volume limits."""
import argparse
import json
import os
from pathlib import Path
import runpy
import shutil
import signal
import stat
import subprocess
import threading
import time

from cargo_gate import ENV_PIN
from checkpoint_ort import processes
from configure_ort import BASE, CAMPAIGN, CPU, CPU_SHA, audit, sha
import gnu_volume_policy
from product_projection import checked_path
from shipping_source import owner
from verified_inputs import identity

PROJECTION = "1bfe8357383b61e86ae7eb81437925477136f37b4adea20f74b5b76312b73f88"
CORE = "9e164456d0b29b2072dbff22f96670c39842579393f84bab60fe2a7bc48fd2c0"
ADMIN = "8514ece887a2a6d56744008c453da6726bff30c82fc066d39696ec949eca06a7"
VHD = Path("/mnt/c/Users/tdotn/AppData/Local/Packages/CanonicalGroupLimited.Ubuntu_79rhkp1fndgsc/LocalState/ext4.vhdx")


def document(path, digest):
    if sha(path) != digest:
        raise ValueError("shipping source or native producer changed: " + str(path))
    return json.loads(path.read_bytes())


def space():
    return {"time":time.time(),"guest_free":shutil.disk_usage(BASE).free,"backing_free":shutil.disk_usage("/mnt/c").free,
            "vhd_length":VHD.stat().st_size}


def capacity(baseline, now, reserve=0):
    guest = max(0, baseline["guest_free"] - now["guest_free"])
    backing = max(0, baseline["backing_free"] - now["backing_free"], now["vhd_length"] - baseline["vhd_length"])
    limit = 24 << 30
    return all(growth + reserve <= limit and free - max(0, limit - growth) >= 100 << 30 for free, growth in
               ((now["guest_free"],guest),(now["backing_free"],backing)))


def monitored(command, root, result_dir, env, reserve, uid, *, address_space=8 << 30, timeout=3600, log_name="build.log",
              volume_guard=None):
    if volume_guard is None and gnu_volume_policy.active() is not None:
        # A run started through volume_policy_entry.py replaces the original 24 GiB guard below.
        policy = gnu_volume_policy.active()
        volume_guard, timeout = policy.guard(reserve, uid), policy.bounded(timeout)
    baseline = json.loads((CAMPAIGN/"linux-cuda-r4-provider-r3-space-watch.jsonl").read_text().splitlines()[0])
    samples = [space()]
    def fits(sample, *, admitting=False):
        if volume_guard is None:
            return capacity(baseline, sample, reserve if admitting else 0)
        return volume_guard(sample, samples[0], admitting)

    if not fits(samples[0], admitting=True):
        raise ValueError("insufficient guest or backing reservation for the original operation")
    done, exceeded = threading.Event(), threading.Event()
    def observe():
        while not done.wait(10):
            try:
                samples.append(space())
                if fits(samples[-1]):
                    continue
            except (OSError, ValueError, KeyError) as error:
                samples[-1]["volume_observation_error"] = str(error)
            exceeded.set()
            for proc in processes(command):
                pid = int(proc.name)
                try:
                    fd = os.pidfd_open(pid)
                except ProcessLookupError:
                    continue
                try:
                    if (os.getsid(pid) == pid and os.getpgid(pid) == pid and (proc/"cwd").samefile(root)
                            and proc in processes(command)):
                        signal.pidfd_send_signal(fd, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                finally:
                    os.close(fd)
    monitor=threading.Thread(target=observe)
    monitor.start()
    process = runpy.run_path(str(CPU))
    try:
        observed = process["run_owned"](["/usr/bin/nice","-n","10","/usr/bin/prlimit",f"--as={address_space}","--",*command],
                                        root,result_dir/log_name,timeout,env)
    finally:
        done.set()
        monitor.join()
        samples.append(space())
        try:
            final_fits = fits(samples[-1])
        except (OSError, ValueError, KeyError) as error:
            samples[-1]["volume_observation_error"] = str(error)
            final_fits = False
        with (result_dir/"volumes.json").open("x") as log:
            json.dump(samples,log)
    audit(result_dir,uid)
    success = process["successful"](observed) and not exceeded.is_set() and final_fits
    return observed, samples, exceeded.is_set(), success


def run(args):
    uid = identity()
    os.umask(0o077)
    source = document(args.source,args.source_sha256)
    root = checked_path(Path(source["source"]),True)
    result_dir = BASE / ("linux-cuda-descriptive-service-r" + str(args.attempt))
    if not 1 <= args.attempt <= 99 or result_dir.exists() or VHD.stat().st_uid != uid:
        raise ValueError("unexpected volume or reused shipping attempt")
    checked_path(BASE,True)
    native = document(CAMPAIGN/"linux-cuda-product-projection-r1.json", PROJECTION)
    core = CAMPAIGN/"linux-cuda-ort-final-core-r1.json"
    if sha(core) != CORE or sha(CPU) != CPU_SHA:
        raise ValueError("original verified native producer or process owner changed")
    admin = CAMPAIGN/"linux-service-admin-r1/admin-assets.json"
    if sha(admin) != ADMIN:
        raise ValueError("staged Admin asset inventory differs")
    env = document(CAMPAIGN/"linux-driver-fix-checks-r1/gate-preflight.json",ENV_PIN)["environment"]
    env.update(HOME=str(BASE.parents[1]),LC_ALL="C",CARGO_NET_OFFLINE="true",CARGO_BUILD_JOBS="1",
               CARGO_INCREMENTAL="0",RAYON_NUM_THREADS="1",PYTHONDONTWRITEBYTECODE="1")
    if owner(root)["source_identity"]() != source["identity"]:
        raise ValueError("frozen source changed before the native build")
    result_dir.mkdir(0o700)
    checked_path(result_dir,True)
    output = result_dir/"runtime"
    script = root / "scripts/build-service-runtime.py"
    command = ["/usr/bin/python3","-B","-S",str(script),"--destination",str(output),"--profile","debug",
               "--admin-inventory",str(admin),"--cuda-environment",str(core),"--cuda-environment-sha256",CORE,
               "--cuda-projection",native["projection"]["path"],"--cuda-projection-sha256",native["projection"]["sha256"],
               "--cuda-native-execution-receipt",native["execution"]["path"],
               "--cuda-native-execution-receipt-sha256",native["execution"]["sha256"]]
    observed, samples, stopped, success = monitored(command,root,result_dir,env,int(6.2 * (1 << 30)),uid)
    if success and owner(root)["source_identity"]() != source["identity"]:
        raise ValueError("frozen source changed during the native build")
    return {"success":success,"profile":"debug","source":source["identity"],"source_receipt":args.source_sha256,
            "output":str(output),"command":observed,"log":str(result_dir/"build.log"),"log_sha256":sha(result_dir/"build.log"),
            "volume_observations":len(samples),"volumes_sha256":sha(result_dir/"volumes.json"),"stopped_on_space":stopped,
            "first":samples[0],"last":samples[-1],"product_projection":PROJECTION,"runtime_files":
            ({p.relative_to(output).as_posix():{"sha256":sha(p),"bytes":p.stat().st_size}
              for p in output.rglob("*") if p.is_file()} if success else {})}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source",type=Path,required=True)
    parser.add_argument("--source-sha256",required=True)
    parser.add_argument("--attempt",type=int,default=1)
    print(json.dumps(run(parser.parse_args()),sort_keys=True))
