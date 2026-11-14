# SPDX-License-Identifier: Apache-2.0
"""Provision a private pinned CPU baseline environment, with wheel/rights receipts."""
import argparse
from email.parser import Parser
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import time
import zipfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
BOOTSTRAP = {"pip": "25.2"}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def installed_distributions(python):
    observed = subprocess.check_output([str(python), "-c",
        "import importlib.metadata as m,json; print(json.dumps([[d.metadata['Name'],d.version] for d in m.distributions()]))"], text=True)
    rows = json.loads(observed)
    result = {}
    for name, version in rows:
        normalized = name.lower().replace("_", "-")
        if normalized in result:
            raise ValueError("environment contains duplicate distributions")
        result[normalized] = version
    return result


def check_environment(installed, expected, allow_missing=False):
    permitted = {**expected, **BOOTSTRAP}
    for name, version in installed.items():
        if name not in permitted or permitted[name] != version:
            raise ValueError(f"environment has unreviewed distribution/version: {name}=={version}")
    if not allow_missing and set(installed) != set(permitted):
        raise ValueError("environment is missing reviewed distributions")


def check_seals(seals):
    for path, expected in seals.items():
        try:
            actual = digest(Path(path))
        except OSError as error:
            raise ValueError("provisioning consumed input became unavailable") from error
        if actual != expected:
            raise ValueError("provisioning consumed inputs changed during the run")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--download", action="store_true")
    parser.add_argument("--profile", choices=("baseline", "ort130", "training"), default="baseline")
    args = parser.parse_args()
    destination = args.destination.resolve()
    # This developer tool never writes outside ignored campaign state.
    if not destination.is_relative_to(ROOT / "artifacts"):
        parser.error("destination must be inside the repository's ignored artifacts directory")
    if destination.is_symlink():
        parser.error("destination must not be a symlink")
    destination.mkdir(parents=True, exist_ok=True)
    wheels = destination / "wheels"
    wheels.mkdir(exist_ok=True)
    requirements = HERE / f"requirements-{args.profile}.txt"
    supplement_path = HERE / "license-supplements.json"
    retained = {str(path.resolve()): path.read_bytes() for path in (Path(__file__), requirements, supplement_path)}
    supplements = json.loads(retained[str(supplement_path.resolve())])
    for supplement in supplements.values():
        path = HERE / supplement["file"]
        retained[str(path.resolve())] = path.read_bytes()
        if hashlib.sha256(retained[str(path.resolve())]).hexdigest() != supplement["sha256"]:
            raise ValueError("pinned license supplement differs")
    seals = {path: hashlib.sha256(data).hexdigest() for path, data in retained.items()}
    started = time.monotonic()
    retained_requirements = destination / f"requirements-input-{time.time_ns()}.txt"
    with retained_requirements.open("xb") as snapshot:
        snapshot.write(retained[str(requirements.resolve())])
    seals[str(retained_requirements)] = digest(retained_requirements)
    if args.download:
        index_args = ["--extra-index-url", "https://download.pytorch.org/whl/cu128"] if args.profile == "training" else []
        subprocess.run([sys.executable, "-m", "pip", "download", "--only-binary=:all:", *index_args,
                        "--no-deps", "--disable-pip-version-check", "--dest", str(wheels),
                        "-r", str(retained_requirements)], check=True)
    inventory = []
    locked = []
    expected = {}
    for line in retained[str(requirements.resolve())].decode("utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        if ";" in line:
            line, marker = line.split(";", 1)
            if marker.strip() != "sys_platform == 'win32'":
                raise ValueError("unreviewed dependency marker")
            if sys.platform != "win32":
                continue
        match = re.fullmatch(r"([A-Za-z0-9_-]+)==([0-9][A-Za-z0-9.+]*)", line.strip())
        if match is None:
            raise ValueError("development requirement must use an exact version")
        expected[match[1].lower().replace("_", "-")] = match[2]
    seen = set()
    for path in sorted(wheels.glob("*.whl")):
        with zipfile.ZipFile(path) as wheel:
            metadata_names = [name for name in wheel.namelist()
                              if name.endswith(".dist-info/METADATA") and len(Path(name).parts) == 2]
            if len(metadata_names) != 1:
                raise ValueError("wheel has no unique METADATA")
            metadata = wheel.read(metadata_names[0]).decode("utf-8")
            fields = Parser().parsestr(metadata)
            name = fields["Name"].lower().replace("_", "-")
            if name in seen or expected.get(name) != fields["Version"]:
                raise ValueError("wheel is duplicate or outside the reviewed exact dependency closure")
            seen.add(name)
            licenses = [name for name in wheel.namelist()
                        if any(piece.lower().startswith(("license", "licence", "copying", "notice"))
                               for piece in Path(name).parts)]
            if not licenses:
                supplement = supplements.get(fields["Name"] + "-" + fields["Version"], supplements.get(fields["Name"]))
                if supplement is None or supplement["version"] != fields["Version"]:
                    raise ValueError(f"wheel has no retained license/notice artifact: {path.name}")
                retained = HERE / supplement["file"]
                if not retained.is_file() or digest(retained) != supplement["sha256"]:
                    raise ValueError("pinned license supplement differs")
            else:
                supplement = None
            inventory.append({"name": path.name, "bytes": path.stat().st_size, "sha256": digest(path),
                              "metadata_sha256": hashlib.sha256(metadata.encode()).hexdigest(),
                              "license_supplement": supplement,
                              "license_artifacts": [{"name": name, "sha256": hashlib.sha256(wheel.read(name)).hexdigest()}
                                                    for name in licenses]})
            locked.append(f"{fields['Name']}=={fields['Version']} --hash=sha256:{digest(path)}")
    if not inventory:
        raise ValueError("no pinned wheels; use --download explicitly")
    if seen != set(expected):
        raise ValueError("reviewed dependency wheels are missing")
    environment = destination / "venv"
    python = environment / ("Scripts/python.exe" if sys.platform == "win32" else "bin/python")
    if not python.exists():
        subprocess.run([sys.executable, "-m", "venv", str(environment)], check=True)
    check_environment(installed_distributions(python), expected, allow_missing=True)
    lockfile = destination / "locked-requirements.txt"
    lockfile.write_text("\n".join(locked) + "\n")
    subprocess.run([str(python), "-m", "pip", "install", "--no-index", "--no-deps", "--require-hashes", "--only-binary=:all:",
                    "--find-links", str(wheels), "--disable-pip-version-check", "-r", str(lockfile)], check=True)
    subprocess.run([str(python), "-m", "pip", "check"], check=True)
    check_environment(installed_distributions(python), expected)
    check = subprocess.check_output([str(python), "-m", "pip", "freeze", "--all"], text=True)
    receipt = {"schema": "airk.development-retrieval-environment/1", "python": sys.version,
               "profile": args.profile,
               "requirements_sha256": digest(requirements), "wheels": inventory,
               "lockfile_sha256": digest(lockfile),
               "input_seals": seals,
               "installed": check.splitlines(), "elapsed_seconds": time.monotonic() - started,
               "scope": "Developer-only pinned experiment. No product dependency, static runtime upgrade or model promotion."}
    receipt_path = destination / f"environment-receipt-{time.time_ns()}.json"
    check_seals(seals)
    with receipt_path.open("x", encoding="utf-8") as output:
        output.write(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"python": str(python), "wheels": len(inventory),
                      "elapsed_seconds": receipt["elapsed_seconds"], "receipt": str(receipt_path)}))


if __name__ == "__main__":
    main()
