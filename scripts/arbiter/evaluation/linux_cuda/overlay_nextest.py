# SPDX-License-Identifier: Apache-2.0
"""Build R7 in a same-UID private target overlay; run its new test image in the original host namespace."""
import argparse
import json
import os
from pathlib import Path
import re
import resource
import runpy
import shutil
import stat
import subprocess
import sys
import tempfile
import threading
import time

from cargo_gate import ENV_PIN
from configure_ort import BASE, CAMPAIGN, CPU, CPU_SHA, sha
from overlay_confinement import mounts, require_original_test_mounts
from overlay_target import PRIVATE_SIZE, isolated, original_budget, prepare
from shipping_build import document
import signed_gate
from target_overlay import require, xdg
from unpack import guarded
from verified_inputs import identity

SCRIPT = Path(__file__).resolve()
GATE = "58c3f27e676d478706e1d4a4fce1cbf8a219ff2e7e847260b19c1daa76afd809"
TESTS = "test(arbiter::runtime::cuda) | test(cpu_source_transition)"
CARGO = BASE/"rustup/toolchains/1.98.0-x86_64-unknown-linux-gnu/bin/cargo"
NEXTEST = BASE/"nextest/cargo-nextest"


def environment(root, *, original=False):
    env = dict(document(CAMPAIGN/"linux-driver-fix-checks-r1/gate-preflight.json",ENV_PIN)["environment"])
    env.update(signed_gate.native(),LC_ALL="C",CARGO_BUILD_JOBS="1",CARGO_INCREMENTAL="0",
               CARGO_NET_OFFLINE="true",RAYON_NUM_THREADS="1",RUST_TEST_THREADS="1",
               HOME=str(root/"home"),TMPDIR=str(root/"tmp"),TMP=str(root/"tmp"),TEMP=str(root/"tmp"),
               AIRK_PRIVATE_CENSUS_BASE=str(root/"private-census"),XDG_CACHE_HOME=str(root/"tmp"))
    if original:
        env.update(CARGO_HOME=str(root/"cargo"),CARGO_TARGET_DIR=str(root/"export"))
    return env


def shas(path):
    return {"bytes":path.stat().st_size,"sha256":sha(path)}


def execute(command, cwd, env, output, errors, deadline):
    with output.open("xb") as stream,errors.open("xb") as err:
        result=subprocess.run(command,cwd=cwd,env=env,stdout=stream,stderr=err,
                              timeout=max(0.001,deadline-time.monotonic()))
    require(result.returncode == 0,"original command failed: "+errors.read_text(errors="replace")[-9000:])


def build(root, namespace):
    setup=prepare(root,namespace)
    os.umask(0o077)
    resource.setrlimit(resource.RLIMIT_AS,(8<<30,8<<30))
    private=root/"private"
    for name in ("home","tmp","private-census"):
        (private/name).mkdir(0o700)
    env=environment(private)
    deadline=time.monotonic()+1800
    catalog=[str(CARGO),"metadata","--format-version","1","--locked","--offline"]
    meta=root/"cargo-metadata.json"
    execute(catalog,signed_gate.ROOT,env,meta,root/"metadata.log",deadline)
    command=[str(CARGO),"nextest","list","--locked","--offline","--timings","-p","airk-retrieval",
             "--lib","--all-features","--list-type","binaries-only","--message-format","json","--profile","ci"]
    binaries=root/"binaries.json"
    execute(command,signed_gate.ROOT,env,binaries,root/"build.log",deadline)
    rows=json.loads(binaries.read_bytes())["rust-binaries"]
    require(len(rows)==1,"expected exactly one compiled Retrieval test binary")
    row,=rows.values()
    native=Path(row["binary-path"])
    relative=native.relative_to(BASE/"target")
    require(relative.parent==Path("debug/deps") and re.fullmatch(r"airk_retrieval-[0-9a-f]{16}",relative.name)
            and row["kind"]=="lib","unexpected library test image")
    upper=private/"target-upper"/relative
    guarded(upper,identity(),stat.S_ISREG)
    require(os.path.samefile(upper,native) or sha(upper)==sha(native),"newly built overlay executable required")
    require(0<upper.stat().st_size<=256<<20,"mutable test export exceeds reservation")
    exported=root/"export"/relative
    exported.parent.mkdir(mode=0o700,parents=True)
    shutil.copyfile(upper,exported)
    exported.chmod(0o700)
    require(shas(upper)==shas(exported),"test image differs after export from private mount")
    timing_dir=private/"target-upper/cargo-timings"
    timings=list(timing_dir.glob("cargo-timing-*.html"))
    require(len(timings)==1 and timings[0].stat().st_size<4<<20,"exact new Cargo timing report required")
    shutil.copyfile(timings[0],root/"timing.html")
    result={"namespace":setup,"build_command":command,"metadata_command":catalog,"binary":shas(exported),
            "relative":str(relative),"metadata":shas(meta),"binaries":shas(binaries),
            "timings":shas(root/"timing.html"),"private_tmpfs_used":shutil.disk_usage(private).used}
    (root/"build-receipt.json").write_text(json.dumps(result))


def outer():
    uid=identity()
    os.umask(0o077)
    before=original_budget()
    session=xdg(uid)
    require_original_test_mounts(mounts(), session)
    require(len(Path("/proc/swaps").read_text().splitlines()) == 1,
            "the full R7 build requires disabled guest swap for its tmpfs working set")
    require(shutil.disk_usage(session).free > (300<<20),"outer executable export reservation unavailable")
    require(sha(CPU)==CPU_SHA,"original held group supervisor changed")
    source=document(CAMPAIGN/"linux-signed-retrieval-source-r7.json",GATE)
    require(signed_gate.authenticate()==source["joint"] and signed_gate.native()==source["native"],
            "signed source or static stage changed before compilation")
    original_budget()
    original_ns=os.readlink("/proc/self/ns/user")
    for proc in Path("/proc").iterdir():
        if proc.name.isdigit():
            try:
                if proc.stat().st_uid==uid and (proc/"comm").read_text().strip() in {"cargo","cargo-clippy","rustc","clippy-driver","cargo-nextest"}:
                    raise ValueError("another GNU Cargo target owner is active")
            except (FileNotFoundError,ProcessLookupError):
                pass
    tool=runpy.run_path(str(CPU))
    with tempfile.TemporaryDirectory(prefix="airk-r7-",dir=session) as text:
        root=Path(text)
        for name in ("home","tmp","cargo","private-census"):
            (root/name).mkdir(0o700)
        worker=isolated(root,SCRIPT,[])
        done=threading.Event()
        observations=[]
        issues=[]
        def sample():
            from shipping_build import space
            snap=space()
            snap["host_tmpfs_bytes"]=shutil.disk_usage(session).used
            snap["swap_used_kib"]=sum(int(row.split()[3]) for row in Path("/proc/swaps").read_text().splitlines()[1:])
            require(snap["vhd_length"]==before["vhd_length"] and snap["swap_used_kib"]==0,
                    "VHD allocation or native swap changed")
            original_budget()
            observations.append(snap)
        def monitor():
            while not done.wait(1):
                try:
                    sample()
                except Exception as exc:
                    issues.append(repr(exc))
                    # Only the real group leader launched by our local run_owned may be stopped.
                    import signal
                    from checkpoint_ort import processes
                    # unshare execs the checked inner interpreter at the same PID.
                    inner=worker[worker.index(sys.executable):]
                    for cmd in (worker,inner,run_cmd):
                        for process in processes(cmd):
                            pid=int(process.name)
                            try:
                                fd=os.pidfd_open(pid)
                            except ProcessLookupError:
                                continue
                            try:
                                parent = next(row for row in (process/"status").read_text().splitlines() if row.startswith("PPid:"))
                                if (int(parent.split()[1])==os.getpid() and os.getpgid(pid)==pid and os.getsid(pid)==pid
                                        and (process/"cwd").samefile(signed_gate.ROOT) and process in processes(cmd)):
                                    signal.pidfd_send_signal(fd,signal.SIGTERM)
                            finally:
                                os.close(fd)
                    return
        run_cmd=[str(NEXTEST),"nextest","run","--cargo-metadata",str(root/"cargo-metadata.json"),
                 "--binaries-metadata",str(root/"binaries.json"),"--target-dir-remap",str(root/"export"),
                 "--profile","ci","--test-threads","1","-E",TESTS]
        sample()
        guard=threading.Thread(target=monitor)
        guard.start()
        try:
            build_run=tool["run_owned"](worker,signed_gate.ROOT,root/"child.log",1800)
            require(tool["successful"](build_run) and not issues,"private build failed: "+(root/"child.log").read_text(errors="replace")[-9000:])
            setup=json.loads((root/"build-receipt.json").read_bytes())
            require(os.readlink("/proc/self/ns/user")==original_ns and not list((root/"private").iterdir())
                    and shas(root/"export"/setup["relative"])==setup["binary"],"ordinary original namespace and exact new ELF required")
            require_original_test_mounts(mounts(), session)
            report=tool["run_owned"](run_cmd,signed_gate.ROOT,root/"nextest.log",1800,environment(root,original=True))
            log=(root/"nextest.log").read_text(errors="replace")
            counts=re.search(r"Summary \[[^\]]+\]\s+(\d+) tests? run: (\d+) passed",log)
            passed=tool["successful"](report) and counts is not None and int(counts[1])>0 and counts[1]==counts[2]
            result={"success":passed,"uid":uid,"source_receipt":GATE,"selected":int(counts[1]) if counts else 0,
                    "source":source["joint"],"original_native_stage":source["native"],"private_build":build_run,
                    "build":setup,"cargo_sources":(root/"build.log").read_text(errors="replace"),"scope":"airk-retrieval --lib --all-features",
                    "nextest":report,"nextest_output":log,"timing_html":(root/"timing.html").read_text(),
                    "original_namespace":original_ns,"target_lower":str(BASE/"target"),"upper_limit":PRIVATE_SIZE}
        finally:
            done.set()
            guard.join()
        sample()
        require(not issues,"; ".join(issues))
        result["volume_samples"]=observations
    result["after"]=original_budget()
    require(result["after"]["vhd_length"]==before["vhd_length"] and not root.exists(),"overlay cleanup or VHD changed")
    require(signed_gate.authenticate()==source["joint"] and signed_gate.native()==source["native"],"joint source/ELF changed after Nextest")
    return result


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--child",type=Path)
    p.add_argument("--namespace")
    a=p.parse_args()
    if a.child:
        build(a.child,a.namespace)
    else:
        print(json.dumps(outer(),sort_keys=True))
