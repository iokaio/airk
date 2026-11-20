# SPDX-License-Identifier: Apache-2.0
"""Check the existing D: DrvFS output semantics and estimate retained native build work."""
import argparse
import hashlib
import json
import mmap
import os
from pathlib import Path
import shutil
import stat
import subprocess

from verified_inputs import identity
from configure_ort import BASE
from unpack import guarded

CAMPAIGN = Path("/mnt/d/code/Github/airk/artifacts/v190/final-plan/exec-20261116T052942Z")


def native_journal(path, epoch):
    rows = {}
    for line in path.read_text().splitlines():
        if line.startswith("#"):
            continue
        start, end, _, name, _ = line.split("\t")
        key = name.replace("linux-cuda-ort-" + epoch, "linux-cuda-ort-@")
        rows[key] = int(end) - int(start)
    return rows


def budgets():
    old = BASE / "linux-cuda-ort-r3/b/Release"
    latest = BASE / "linux-cuda-ort-r4/b/Release"
    earlier = native_journal(old / ".ninja_log", "r3")
    retained = native_journal(latest / ".ninja_log", "r4")
    remaining = {name:elapsed for name,elapsed in earlier.items() if name not in retained}
    return {"r3_output_rows":len(earlier),"r4_output_rows":len(retained),"unmatched_r3_outputs":len(remaining),
            "r3_remaining_worker_seconds":round(sum(remaining.values()) / 1000, 3),
            "r3_unmatched_longest_task_seconds":round(max(remaining.values(),default=0)/1000, 3),
            "retained_provider_exists":(latest / "libonnxruntime_providers_cuda.so").is_file(),
            "retained_aggregate_exists":(latest.parents[1] / "aggregate/libonnxruntime.a").is_file()}


def metadata(path):
    info = path.lstat()
    return {"uid":info.st_uid,"gid":info.st_gid,"mode":format(stat.S_IMODE(info.st_mode),"04o"),
            "inode":info.st_ino,"device":info.st_dev,"links":info.st_nlink}


def run(destination):
    uid = identity()
    if destination.parent != CAMPAIGN or not destination.name.startswith("linux-drvfs-probe-"):
        raise ValueError("expected fresh, Windows-owner-checked D: probe directory")
    if any(destination.iterdir()) or destination.resolve(strict=True) != destination:
        raise ValueError("require empty checked physical directory")
    os.umask(0o077)
    private = destination / "private"
    private.mkdir(mode=0o700)
    image = private / "true"
    system = Path("/usr/bin/true")
    original = system.stat()
    if not stat.S_ISREG(original.st_mode) or original.st_uid or original.st_mode & 0o022:
        raise ValueError("expected protected operating-system ELF control")
    with system.open("rb") as src, open(os.open(image,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o700),"wb") as out:
        shutil.copyfileobj(src,out)
    if image.read_bytes() != system.read_bytes() or image.read_bytes()[:4] != b"\x7fELF":
        raise ValueError("disposable ELF differs from installed true")
    with image.open("rb") as file, mmap.mmap(file.fileno(),0,flags=mmap.MAP_PRIVATE,prot=mmap.PROT_READ|mmap.PROT_EXEC) as mapped:
        mapped_elf = mapped[:4] == b"\x7fELF"
    result = subprocess.run([str(image)],cwd=private,env={"PATH":"/usr/bin:/bin","LC_ALL":"C","TMPDIR":str(private)},
                            capture_output=True,timeout=10)
    link_results = {}
    for name, operation in (("hard",lambda path:os.link(image,path)),("symbolic",lambda path:path.symlink_to(image.name))):
        try:
            path = private / name
            operation(path)
            link_results[name] = {"metadata":metadata(path),"same_inode":path.samefile(image)}
        except OSError as exc:
            link_results[name] = {"errno":exc.errno}
    eligible = True
    try:
        guarded(private,uid,stat.S_ISDIR)
    except ValueError:
        eligible = False
    return {"directory":metadata(private),"elf":metadata(image),"sha256":hashlib.sha256(image.read_bytes()).hexdigest(),
            "mapped_elf_rx":mapped_elf,"native_elf_exit":result.returncode,"links":link_results,
            "private_build_directory_guard_passed":eligible,"ninja":budgets()}


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination",type=Path,required=True)
    print(json.dumps(run(parser.parse_args().destination),sort_keys=True))
