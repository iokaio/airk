# SPDX-License-Identifier: Apache-2.0
"""Authenticate the existing static-1024 installation and retain mutable state before reuse."""
import hashlib
import json
from pathlib import Path
import shutil
import runpy


E = Path(__file__).resolve().parents[4] / "artifacts/v190/final-plan/exec-20261116T052942Z"
R3 = E / "cal100-focus1024-capture-r3"
R4 = E / "cal100-focus1024-capture-r4"
R5 = E / "cal100-focus1024-capture-r5"
CONTROL = R3 / "fixture/control"
GROUPED_SHA = "8588e26901dc869a955949083aa0580abca727e735a22166a9ecd81c4dbea667"
PLAN = "9259b4be9e340ac8e050060e904f53acffee27e3807eca6145a9a8f3be3e299d"


def require(ok, message):
    if not ok:
        raise ValueError(message)


def ordinary(path):
    require(path.resolve(strict=True) == path and all(not p.is_symlink() and not p.is_junction()
            for p in (path, *path.parents)), "noncanonical installation path")
    return path


def link(path):
    with ordinary(path).open("rb") as stream:
        seal = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"path": str(path), "sha256": seal}


def read(path, seal=None):
    raw = ordinary(path).read_bytes()
    require(seal is None or hashlib.sha256(raw).hexdigest() == seal, "changed installation evidence " + path.name)
    return json.loads(raw)


def linked(pin):
    return read(Path(pin["path"]), pin["sha256"])


def inventory(descriptor):
    plan = read(R4 / "lifetime-plan.json", PLAN)
    base = plan["installation"]
    ready, closed = linked(base["ready"]), linked(base["closed"])
    roster_pin = descriptor["runtime_inputs"]["inputs"]
    roster = linked(roster_pin)
    profiles = [read(Path(row["profile"]), row["profile_sha256"]) for row in roster]
    expected = [(p["manifest"], p["model"], row["profile_sha256"]) for row, p in zip(roster, profiles, strict=True)]
    require(Path(ready["control_root"]).samefile(CONTROL) and Path(closed["control_root"]).samefile(CONTROL)
            and ready["health"]["instance"] == closed["instance"] and closed["shutdown_completed"] and closed["service_lease_reacquired"]
            and ready["health"]["executable_sha256"] == descriptor["runtime_inputs"]["service"]["sha256"]
            and ready["inputs_sha256"] == roster_pin["sha256"] and
            [(p["manifest"], p["model"], p["profile_sha256"]) for p in ready["profiles"]] == expected,
            "physical installation or original model roster differs")
    immutable = {}
    for profile in profiles:
        for a in profile["assets"]:
            key = (Path("arbiter-v3/assets") / profile["manifest"] / a["name"]).as_posix()
            immutable[key] = a["sha256"]
    # Each installed MiniLM set member is authenticated against original publisher bytes.
    sets = list(ordinary(CONTROL / "model-files-1.4/sets").iterdir())
    require(len(sets) == 1 and sets[0].is_dir(), "ambiguous MiniLM install set")
    for a in descriptor["minilm_assets"]:
        file = sets[0] / a["name"]
        immutable[file.relative_to(CONTROL).as_posix()] = a["sha256"]
    for name in immutable:
        require(ordinary(CONTROL / name).is_file(), "installed immutable asset missing")
    return base, ready, immutable


def settled(owner):
    return (owner["owned_descendants_settled"] and not owner["owned_job_forced_termination"]
            and owner["owned_job_final_active_processes"] == 0 and owner["owned_job_final_member_pids"] == [])


def failed_cal_lifetimes(ready, status):
    folders = sorted(R4.glob("lifetime-[0-9][0-9]"))
    require(0 < len(folders) <= 13 and folders == [R4 / f"lifetime-{i:02}" for i in range(len(folders))],
            "partial CAL lifetime order is ambiguous")
    for folder in folders:
        start = read(folder / "fixture-start.json")
        opened = read(folder / "fixture/ready.json")
        closed = read(folder / "fixture/closed.json")
        require(opened["health"]["process_id"] == start["pid"] and opened["profiles"] == ready["profiles"]
                and opened["inputs_sha256"] == ready["inputs_sha256"]
                and opened["health"]["executable_sha256"] == ready["health"]["executable_sha256"]
                and opened["health"]["instance"] == closed["instance"] and Path(opened["control_root"]).samefile(CONTROL)
                and Path(closed["control_root"]).samefile(CONTROL) and closed["shutdown_completed"]
                and closed["service_lease_reacquired"], "failed CAL lifetime lacks natural native shutdown")
        before = read(folder / "commands/stage-before.stdout")
        require(before["configuration"] == status["configuration"], "partial CAL configuration differs")
    return status


def latest_cal_receipt():
    markers = ("control-mutex-acquired.json", "lifetime-plan.json", "lifetime-00", "outer-owned.json", "control-mutex-released.json")
    if any((R5 / name).exists() for name in markers):
        return R5 / "control-mutex-released.json"
    return R4 / "outer-owned.json"


def grouped_verifier():
    source = Path(__file__).with_name("grouped_reuse.py").resolve()
    require(link(source)["sha256"] == GROUPED_SHA, "grouped predecessor verifier source changed")
    return runpy.run_path(str(source))["verify"]


def restart_inputs(descriptor, prior, prior_sha, custody_sha=None):
    base, ready, immutable = inventory(descriptor)
    if prior == R5 / "control-mutex-released.json":
        require(isinstance(custody_sha, str) and len(custody_sha) == 64, "grouped release also requires sealed source custody")
        status, _ = grouped_verifier()(E, prior_sha, custody_sha, base)
        return {k: {"path": p["path"], "sha256": p["sha256"]} for k, p in
                (("ready", base["ready"]), ("closed", base["closed"]), ("status", status))}, immutable
    require(custody_sha is None, "unexpected grouped custody digest")
    if prior == R4 / "outer-owned.json":
        require(latest_cal_receipt() == prior, "grouped owner started; stale R4 control handoff refused")
    owner = read(prior, prior_sha)
    require(settled(owner) and not owner["timed_out"], "prior owner did not naturally settle")
    if prior == R4 / "outer-owned.json" and owner["exit_code"] != 0:
        status_pin = base["status"]
        failed_cal_lifetimes(ready, linked(status_pin))
    elif prior == R4 / "outer-owned.json":
        terminal = read(R4 / "fixture-closure.json")
        require(terminal["plan"]["sha256"] == PLAN and terminal["ordered_questions"] == 100
                and len(terminal["lifetimes"]) == 13, "original CAL complete closure absent")
        for i, pin in enumerate(terminal["lifetimes"]):
            folder = R4 / f"lifetime-{i:02}"
            require(Path(pin["path"]) == folder / "lifetime-closure.json", "foreign CAL closure")
            life = linked(pin)
            closed = linked(life["closed"])
            require(life["phase"] == i and life["instance"] == closed["instance"] and closed["shutdown_completed"]
                    and closed["service_lease_reacquired"] and Path(closed["control_root"]).samefile(CONTROL),
                    "CAL lifetime did not close the original installation")
        status_pin = base["status"]
    else:
        require(owner["exit_code"] == 0 and prior.name == "owned.json" and prior.parent.parent == E and prior.parent.name.startswith("compact-development-")
                and owner["focus_width"] == 1024 and owner["source_seals_unchanged"], "foreign preceding case")
        seal = owner["shared_installation"]
        require(seal["anchor_plan_sha256"] == PLAN and Path(seal["control_root"]).samefile(CONTROL)
                and Path(seal["closed"]["path"]) == prior.parent / "fixture/closed.json"
                and Path(seal["ready"]["path"]) == prior.parent / "fixture/ready.json"
                and Path(seal["status"]["path"]) == prior.parent / "commands/reuse-final-status.stdout",
                "previous shared-control provenance differs")
        old, closed = linked(seal["ready"]), linked(seal["closed"])
        require(old["health"]["instance"] == closed["instance"] and Path(closed["control_root"]).samefile(CONTROL)
                and closed["shutdown_completed"] and closed["service_lease_reacquired"], "previous TRAIN closure incomplete")
        for label in ("control_before", "control_after"):
            require(Path(seal[label]["path"]) == prior.parent / (label.replace("_", "-") + ".json"),
                    "foreign custody manifest")
            custody_record = linked(seal[label])
            require(custody_record["immutable"] == immutable and Path(custody_record["control_root"]).samefile(CONTROL)
                    and all(custody_record["files"][k] == v for k, v in immutable.items()),
                    "previous capture lost original installed assets")
        status_pin = seal["status"]
    status = linked(status_pin)
    require(status.get("initialization") is None and all(status[k]["queued"] == status[k]["running"] == 0 for k in ("focus", "context"))
            and ready["health"]["catalog_clock"]["namespace"] == status["configuration"]["catalog"], "preceding idle status differs")
    return {k: {"path": p["path"], "sha256": p["sha256"]} for k, p in
            (("ready", base["ready"]), ("closed", base["closed"]), ("status", status_pin))}, immutable


def custody(output, immutable, preserve):
    phase = "before" if preserve else "after"
    target = output / "control-before"
    if preserve:
        target.mkdir()
    files = {}
    directories = []
    for path in sorted(ordinary(CONTROL).rglob("*")):
        ordinary(path)
        name = path.relative_to(CONTROL).as_posix()
        if path.is_dir():
            directories.append(name)
            continue
        require(path.is_file(), "unsupported entry in physical control")
        seal = link(path)["sha256"]
        files[name] = seal
        if name in immutable:
            require(seal == immutable[name], "installed asset content changed: " + name)
        elif preserve:
            require(not name.endswith((".onnx", ".onnx.data", ".safetensors")), "unrecognized model cannot be copied")
            destination = target / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            with path.open("rb") as src, destination.open("xb") as dst:
                shutil.copyfileobj(src, dst, 1 << 20)
            require(link(destination)["sha256"] == seal, "mutable control backup changed")
    require(immutable.keys() <= files.keys(), "expected source asset missing from shared installation")
    manifest = output / ("control-" + phase + ".json")
    with manifest.open("x", encoding="utf8", newline="\n") as stream:
        json.dump({"control_root": str(CONTROL), "immutable": immutable, "directories": directories, "files": files}, stream, sort_keys=True)
    return link(manifest)
