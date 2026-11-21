# SPDX-License-Identifier: Apache-2.0
"""Retain the built native sources and stage the linked Large CAL test image beside the scientific runtime.

Reads one passed Cargo image-build receipt. Writes each roster source as
`retained-native-sources/<sha256>.source` beside that receipt, one byte copy of
the compiler-emitted Inference test image in the provider stage, and a receipt.
No Cargo, native or model process runs.
"""
import argparse
import ctypes
import json
import os
from pathlib import Path
import re
import runpy
import shutil
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CAMPAIGN = ROOT / "artifacts/v190/final-plan/exec-20261116T052942Z"
PROVIDER_STAGE = CAMPAIGN / "windows-cu15-scientific-provider-stage-r1"
MANIFEST = "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5"
BUILD_PREFIX = "windows-large-direct-cal-checks-"
STAGED_NAME = r"fresh-cu15-large-direct-cal-r[1-9][0-9]*-inference\.exe"
OUTPUT_NAME = r"windows-large-direct-cal-stage-r[1-9][0-9]*"
SCHEMA = "airk.private.large-cal-image-stage/1"
INFERENCE = "crates/airk-inference/src/inference_engine/pool/arbiter/qualification/software_v2/cuda_qualification"
RETRIEVAL = "crates/airk-retrieval/src/arbiter/runtime"
CUTLASS = RETRIEVAL + "/cuda/diagnostic/cutlass"
# The 21 sources retained for the Large pressure image, in the order of its build receipt.
PRESSURE_KEYS = (
    INFERENCE + ".rs",
    INFERENCE + "/CONTRACT.md",
    INFERENCE + "/bounded_hybrid.rs",
    INFERENCE + "/fp16_compact.rs",
    INFERENCE + "/fp16_compact/full.rs",
    INFERENCE + "/fp16_compact/timing.rs",
    INFERENCE + "/fp16_large.rs",
    INFERENCE + "/fp16_large/compensated.rs",
    INFERENCE + "/hybrid_profile_trace.rs",
    RETRIEVAL + ".rs",
    RETRIEVAL + "/bounded_hybrid.rs",
    RETRIEVAL + "/bounded_hybrid/layer0.rs",
    RETRIEVAL + "/cuda.rs",
    RETRIEVAL + "/cuda/diagnostic.rs",
    CUTLASS + ".rs",
    CUTLASS + "/large.rs",
    CUTLASS + "/large_routes.rs",
    CUTLASS + "/model.rs",
    CUTLASS + "/model/pressure.rs",
    RETRIEVAL + "/cuda/fp16.rs",
    RETRIEVAL + "/cuda/run.rs",
)
CAL_KEYS = (
    INFERENCE + "/fp16_large/cal.rs",
    CUTLASS + "/large/cal.rs",
    INFERENCE + "/fp16_compact/cal.rs",
)
SNAPSHOT_KEYS = PRESSURE_KEYS + CAL_KEYS


def require(ok, message):
    if not ok:
        raise ValueError(message)


def image_build(receipt):
    """Accept only a passed, unchanged, locked offline build of the Inference library test image."""
    command = receipt["command"]
    require(receipt["schema"] == "airk-readiness-command/1" and receipt["passed"] is True
            and receipt["inputs_unchanged"] is True and receipt["exit_code"] == 0
            and all(word in command for word in ("test", "airk-inference", "--no-run", "--all-features", "--lib",
                                                 "--locked", "--offline", "--timings", "--message-format=json"))
            and Path(receipt["target_directory"]) == ROOT / "target",
            "passed unchanged locked Cargo JSON image build required")
    files = receipt["groups"]["native"]["files"]
    missing = [key for key in SNAPSHOT_KEYS if not re.fullmatch("[0-9a-f]{64}", str(files.get(key, "")))]
    require(not missing, "build receipt lacks retained source digests: " + ", ".join(missing))
    return {key: files[key] for key in SNAPSHOT_KEYS}


def cargo_test_image(log_lines):
    """The one Inference library test executable that this Cargo JSON log reports."""
    selected = []
    for line in log_lines:
        if line.startswith("{"):
            event = json.loads(line)
            if (event.get("reason") == "compiler-artifact" and event["target"]["name"] == "airk_inference"
                    and event["profile"]["test"] is True and event.get("executable")):
                selected.append(Path(event["executable"]))
    require(len(selected) == 1, "exactly one compiler-emitted Inference test image required")
    require(selected[0].parent == ROOT / "target/debug/deps" and selected[0].name.startswith("airk_inference-")
            and selected[0].suffix == ".exe", "Inference test image is outside the campaign Cargo target")
    return selected[0]


def copy_new(source, destination, expected, digest):
    """Byte-copy one file to a fresh name and prove the copy; an equal earlier copy is kept."""
    if destination.exists():
        require(digest(destination) == expected, "existing retained copy differs: " + destination.name)
        return False
    with source.open("rb") as stream, destination.open("xb") as sink:
        shutil.copyfileobj(stream, sink, 1 << 20)
        sink.flush()
        os.fsync(sink.fileno())
    require(digest(destination) == expected, "retained byte copy differs: " + destination.name)
    return True


def snapshot(digests, source_root, directory, digest, audit):
    """Write `<sha256>.source` for every roster key from sources that still equal the built bytes."""
    live = {key: source_root / key for key in digests}
    changed = [key for key, path in live.items() if digest(path) != digests[key]]
    require(not changed, "live source differs from the built source; stage from the built revision: " + ", ".join(changed))
    retained = {key: directory / (sha + ".source") for key, sha in digests.items()}
    fresh = [path for path in dict.fromkeys(retained.values()) if not path.exists()]
    if fresh:
        audit(fresh, False)
    for key, destination in retained.items():
        copy_new(live[key], destination, digests[key], digest)
    audit(list(dict.fromkeys(retained.values())))
    return retained


def run(args):
    shared = runpy.run_path(str(HERE / "probe_compact.py"))
    components = runpy.run_path(str(ROOT / "scripts/cuda-component-execution.py"))
    budget = runpy.run_path(str(HERE / "large_cal_build.py"))
    base = components["base"]()
    parent, _ = base.helpers({"ownership_helper_root": str(base.MAINTAINED_HELPER_ROOT)})
    exact = lambda path, sha: shared["exact_pin"](path, sha, base)
    require(os.name == "nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode
            and parent.worker.windows_sid() == base.SID == components["token_owner"](ctypes.c_void_p(-1)),
            "verified isolated human required")
    guard = {"checked_guard": base.pin(ROOT / "scripts/dev/assert-human-execution.ps1"),
             "powershell": exact(args.powershell, args.powershell_sha256)}

    def audit(paths, created=True):
        components["guard_many"](guard, list(paths), created)

    out, staged, build_path = args.output, args.staged_image, args.build
    checks = build_path.parent
    require(out.is_absolute() and out.parent == CAMPAIGN and out.parent.resolve(strict=True) == out.parent
            and re.fullmatch(OUTPUT_NAME, out.name) and not out.exists(), "fresh private stage receipt directory required")
    require(checks.parent == CAMPAIGN and checks.name.startswith(BUILD_PREFIX) and build_path.name == "large-image-r1.json",
            "Large CAL image-build receipt required")
    require(staged.parent == PROVIDER_STAGE and re.fullmatch(STAGED_NAME, staged.name) and not staged.exists(),
            "fresh staged Large CAL image name in the scientific provider stage required")
    manifest_path = PROVIDER_STAGE / "airk-cuda-runtime.json"
    audit([build_path, manifest_path])
    build, manifest = exact(build_path, args.build_sha256), exact(manifest_path, MANIFEST)
    receipt = base.read(build["path"], 32 << 20)
    digests = image_build(receipt)
    leaf = receipt["log"]
    require(Path(leaf).name == leaf and leaf not in ("", ".", ".."), "Cargo log sibling leaf required")
    log = build_path.with_name(leaf)
    audit([log])
    log_pin = base.pin(log)
    require(0 < log_pin["bytes"] <= 32 << 20, "bounded actual Cargo log required")
    with log.open(encoding="utf-8") as stream:
        linked_path = cargo_test_image(stream)
    audit([linked_path])
    linked = base.pin(linked_path)
    before = shutil.disk_usage(ROOT).free
    budget["require_disk"](before - linked["bytes"])
    directory = checks / "retained-native-sources"
    if not directory.exists():
        components["mkdir"](guard, directory)
    audit([directory])
    retained = snapshot(digests, ROOT, directory, base.digest, audit)
    audit([staged], False)
    copy_new(linked_path, staged, linked["sha256"], base.digest)
    audit([staged])
    image = exact(staged, linked["sha256"])
    require(image["bytes"] == linked["bytes"], "staged image length differs")
    base.pinned(linked)
    # The consumers' own reader must accept the staged copy and the retained sources.
    observed_log, artifact, common = shared["linked_image"](build, image, base, audit, "metadata", source_snapshot=directory)
    require(observed_log == log_pin and artifact == linked, "probe reader selected a different Cargo log or image")
    sources = {key: exact(path, digests[key]) for key, path in retained.items()}
    require(all(sources[key] == pin for key, pin in common.items()), "probe reader and stager disagree on a retained source")
    for pin in (build, manifest, log_pin, linked, image):
        base.pinned(pin)
    after = shutil.disk_usage(ROOT).free
    components["mkdir"](guard, out)
    components["write_new"](guard, out / "execution.json", {
        "schema": SCHEMA, "complete": True, "native_invocations": 0, "qualification": False,
        "build": build, "build_log": log_pin, "linked_image": linked, "image": image, "runtime_manifest": manifest,
        "source_snapshot": str(directory), "source_roster": list(SNAPSHOT_KEYS), "sources": sources,
        "d_free_bytes": {"before": before, "after": after, "required_floor": budget["disk_floor"]() + budget["D_CUSHION"]},
        "powershell": guard["powershell"], "checked_guard": guard["checked_guard"],
        "checked_stager": base.pin(Path(__file__).resolve())})
    print(json.dumps({"image_sha256": image["sha256"], "image_bytes": image["bytes"], "sources": len(sources),
                      "receipt_sha256": base.digest(out / "execution.json")}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build", type=Path, required=True, help="large-image-r1.json of the Large CAL build")
    parser.add_argument("--build-sha256", required=True)
    parser.add_argument("--staged-image", type=Path, required=True)
    parser.add_argument("--powershell", type=Path, required=True)
    parser.add_argument("--powershell-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    run(parser.parse_args())
