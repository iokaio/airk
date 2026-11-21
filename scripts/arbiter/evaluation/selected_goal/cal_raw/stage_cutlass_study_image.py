# SPDX-License-Identifier: Apache-2.0
"""Copy the linked Kernel study test image beside its Cargo receipts and write its staging descriptor."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from r8_cutlass_study_inputs import check_outputs, pinned, read, require
from phase_reference import digest
from run_cutlass_study import SELECTOR

NAMES = ["airk_kernel.exe", "images.json"]


def linked_image(build_pin, unit_pin):
    """The one Kernel library test executable an unchanged `--no-run` build linked from the unit-tested sources."""
    build, unit = read(build_pin), read(unit_pin)
    for receipt in (build, unit):
        require(receipt["schema"] == "airk-readiness-command/1" and receipt["passed"] is True
                and receipt["inputs_unchanged"] is True and receipt["command_inputs_unchanged"] is True,
                "passing Cargo receipt with unchanged inputs required")
    require("--no-run" in build["command"] and "nextest" in unit["command"] and unit["selected_tests"] >= 1
            and build["groups"]["native"]["sha256"] == unit["groups"]["native"]["sha256"]
            and build["target_directory"] == unit["target_directory"],
            "image build and unit tests used different sources, targets or commands")
    log = Path(build_pin["path"]).parent / build["log"]
    images = []
    for line in log.read_bytes().splitlines():
        if line.startswith(b"{"):
            item = json.loads(line)
            if (item.get("reason") == "compiler-artifact" and item["target"]["name"] == "airk_kernel"
                    and item["profile"]["test"] and item.get("executable")):
                images.append(Path(item["executable"]))
    require(len(images) == 1 and images[0].is_file()
            and images[0].resolve().is_relative_to(Path(build["target_directory"])),
            "exactly one linked Kernel test image in the receipt's target required")
    return images[0]


def stage(build_pin, unit_pin):
    source = linked_image(build_pin, unit_pin)
    out = Path(build_pin["path"]).parent / "images"
    require(out.is_dir() and not any(out.iterdir()), "empty checked images directory beside the receipts required")
    check_outputs(out, NAMES, created=False)
    image, descriptor = (out / name for name in NAMES)
    with source.open("rb") as reader, image.open("xb") as writer:
        shutil.copyfileobj(reader, writer, 1 << 20)
    sha = digest(image)
    require(sha == digest(source), "staged image differs from the linked image")
    value = {"name": "airk_kernel", "source": str(source), "path": str(image), "sha256": sha,
             "bytes": image.stat().st_size, "build_sha256": build_pin["sha256"],
             "unit_sha256": unit_pin["sha256"], "study_selector": SELECTOR, "study_invocations": 0}
    with descriptor.open("x", encoding="utf-8", newline="\n") as stream:
        stream.write(json.dumps(value, indent=2) + "\n")
    check_outputs(out, NAMES, created=True)
    return {"path": str(descriptor), "sha256": hashlib.sha256(descriptor.read_bytes()).hexdigest()}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("build", "unit"):
        parser.add_argument("--" + name, type=Path, required=True)
        parser.add_argument("--" + name + "-sha256", required=True)
    args = parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use checked Windows Python -I -B -S")
    print(json.dumps(stage(*(pinned({"path": str(getattr(args, name)), "sha256": getattr(args, name + "_sha256")},
                                    name + " receipt") for name in ("build", "unit")))))
