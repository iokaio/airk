# SPDX-License-Identifier: Apache-2.0
"""Force a local Windows AIRK Service installer build (split plan §10.5.1, U21, U22, U37).

Entry point: scripts/build-local-service-installer.ps1 -ForceBuild. Steps, stopping at the first
failure: prerequisites; the admin bundle check (scripts/build-admin-assets.py: `npm ci` and
`npm run check` in packages/admin); release `airk` (shipping inference) and lightweight `airkctl`,
both through build-service-runtime.py's one Cargo owner, with its verified static ORT inputs and
airkctl dependency proof; the §9.5 staged layout (bin/, service/, notices/) with
`payload-inventory.json` (airk-service-payload/1, setup_role CONTRACT.md); signing; the Inno
script installers/windows/airk-service.iss; a receipt. Nothing is installed, started, trusted or
published.

Signing (U22): the local Trusted Signing adapter of scripts/sign-native.py is used when that
script provides one (`sign_windows_local`). It does not yet, so a build stops before packaging
unless --unsigned is given. --unsigned labels everything unsigned (inventory `signing: null`,
artifact name `...-UNSIGNED-installer.exe`); setup refuses such a payload at placement
(`payload_unsigned`), so an unsigned installer is a packaging check, never an installable product.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import tomllib

ROOT = Path(__file__).resolve().parent.parent
TARGET = "x86_64-pc-windows-msvc"
SCRIPT = ROOT / "installers/windows/airk-service.iss"
INVENTORY_SCHEMA = "airk-service-payload/1"
OUTPUT = ROOT / "artifacts/local-service-installer"


def load_script(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), ROOT / "scripts" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def admin_digests():
    """The I4 admin digests the airk image computes from its embedded bundle; one owner:
    scripts/build-admin-assets.py."""
    return load_script("build-admin-assets").digests()


class Steps:
    """Timed, logged steps; the receipt records each one."""

    def __init__(self, folder):
        self.folder = folder
        self.records = []

    def run(self, name, command, cwd=ROOT):
        started = time.monotonic()
        log = self.folder / f"{len(self.records):02d}-{name}.log"
        with log.open("w", encoding="utf-8") as stream:
            result = subprocess.run([str(part) for part in command], cwd=cwd, stdout=stream,
                                    stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL)
        self.records.append({"step": name, "command": [str(part) for part in command],
                             "seconds": round(time.monotonic() - started, 1),
                             "exit_code": result.returncode, "log": log.name})
        if result.returncode:
            raise RuntimeError(f"{name} failed ({result.returncode}); see {log}")

    def call(self, name, action):
        started = time.monotonic()
        value = action()
        self.records.append({"step": name, "seconds": round(time.monotonic() - started, 1)})
        return value


def prerequisites(iscc):
    if sys.platform != "win32":
        raise SystemExit("The local service installer is built on Windows")
    missing = [tool for tool in ("cargo", "rustc", "npm", "node", "pwsh", "git") if shutil.which(tool) is None]
    if not iscc.is_file():
        missing.append(f"Inno Setup ISCC ({iscc})")
    if missing:
        raise SystemExit("Missing prerequisites: " + ", ".join(missing))


def stage(layout, airk, airkctl, version, signing):
    (layout / "bin").mkdir(parents=True)
    (layout / "service").mkdir()
    shutil.copy2(airkctl, layout / "bin/airkctl.exe")
    shutil.copy2(airk, layout / "service/airk.exe")
    notices = layout / "notices"
    notices.mkdir()
    for name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"):
        shutil.copyfile(ROOT / name, notices / name)
    subprocess.run([sys.executable, str(ROOT / "scripts/license-notices.py"), "--output",
                    str(notices / "dependencies"), "--release"], cwd=ROOT, check=True)
    files = {item.relative_to(layout).as_posix(): {"sha256": digest(item), "bytes": item.stat().st_size}
             for item in sorted(layout.rglob("*")) if item.is_file()}
    inventory = {"schema": INVENTORY_SCHEMA, "version": version, "channel": "stable", "target": TARGET,
                 "executables": {"airkctl": "bin/airkctl.exe", "airk": "service/airk.exe"},
                 "files": files, "admin_assets": admin_digests(), "signing": signing}
    (layout / "payload-inventory.json").write_text(json.dumps(inventory, indent=1) + "\n", encoding="utf-8")
    return inventory


def compile_installer(steps, iscc, layout, output, artifact, uninstallers=None):
    definitions = {"AirkVersion": json.loads((layout / "payload-inventory.json").read_text())["version"],
                   "AirkPayload": layout, "AirkOutput": output, "AirkArtifact": artifact,
                   "AirkInventorySHA": digest(layout / "payload-inventory.json"),
                   "AirkBinarySHA": digest(layout / "service/airk.exe"),
                   "AirkCtlBinarySHA": digest(layout / "bin/airkctl.exe")}
    if uninstallers is not None:
        definitions["AirkUninstallers"] = uninstallers
    if any(any(char in str(value) for char in ("\r", "\n", '"')) for value in definitions.values()):
        raise ValueError("Unsafe Inno Setup definition")
    steps.run("iscc", [iscc, "/Qp", "/NI", *[f"/D{name}={value}" for name, value in definitions.items()],
                       SCRIPT])


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--force-build", action="store_true", required=True,
                        help="Build from this checkout; completed artifacts are never reused")
    parser.add_argument("--unsigned", action="store_true",
                        help="Label the payload and installer unsigned instead of stopping without signing access")
    parser.add_argument("--iscc", type=Path, default=Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)"))
                        / "Inno Setup 6/ISCC.exe")
    args = parser.parse_args()
    prerequisites(args.iscc)
    signer = load_script("sign-native")
    local_signing = getattr(signer, "sign_windows_local", None)
    if local_signing is None and not args.unsigned:
        raise SystemExit("scripts/sign-native.py has no local Trusted Signing adapter yet (U70); "
                         "a signed installer cannot be built. Pass --unsigned for a labeled packaging check.")
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    if not load_script("split_receipts").split_version(version):
        raise SystemExit(f"Version {version} predates the split; increment the patch version before a local build")
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip())
    run = OUTPUT / time.strftime("%Y%m%d-%H%M%S")
    run.mkdir(parents=True, exist_ok=False)
    steps = Steps(run)
    receipt = {"schema": "airk-local-service-installer/1", "version": version, "target": TARGET,
               "commit": commit, "dirty": dirty, "signed": False, "result": "failed"}
    try:
        admin = load_script("build-admin-assets")
        # The shared owner runs npm ci and the drift check, then writes the checked inventory.
        admin.check(run / "admin-assets.json", runner=lambda command, cwd: steps.run(
            "admin-" + command[1], [admin._resolve(command[0]), *command[1:]], cwd=cwd))
        runtime = load_script("build-service-runtime")
        steps.call("static-ort-inputs", runtime.verify_static_inputs)
        airk = steps.call("airk", lambda: runtime.cargo_binary("airk", "release", "local-inference"))
        steps.call("airkctl-dependencies", runtime.check_lightweight_dependencies)
        airkctl = steps.call("airkctl", lambda: runtime.cargo_binary("airkctl", "release", None, package="airkctl"))
        layout = run / "layout"
        signing = None
        if local_signing is not None:
            # The adapter signs both images in place and returns the record's signing identity.
            signing = steps.call("sign-images", lambda: local_signing([airk, airkctl]))
        inventory = steps.call("stage", lambda: stage(layout, airk, airkctl, version, signing))
        artifact = f"airk-service-{version}-{TARGET}-{'installer' if signing else 'UNSIGNED-installer'}"
        compiled = run / "compiled"
        compiled.mkdir()
        compile_installer(steps, args.iscc, layout, compiled, artifact)
        installer = compiled / f"{artifact}.exe"
        if not installer.is_file():
            raise RuntimeError("ISCC did not produce the installer")
        if signing is not None:
            # Signed uninstaller and outer EXE follow package-standalone-windows.py's
            # prepare/build/record order once the local adapter exists (§10.5.4).
            raise RuntimeError("Signed installer packaging awaits the local signing adapter's installer stage")
        receipt.update({"result": "built", "signed": signing is not None, "installer": str(installer),
                        "installer_sha256": digest(installer),
                        "payload_inventory_sha256": digest(layout / "payload-inventory.json"),
                        "files": inventory["files"], "admin_assets": inventory["admin_assets"]})
    except Exception as error:  # Reported with the receipt; no older installer is announced.
        receipt["error"] = f"{type(error).__name__}: {error}"
    finally:
        receipt["steps"] = steps.records
        (run / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    if receipt["result"] != "built":
        print(f"FAILED: {receipt['error']}", file=sys.stderr)
        print(f"Receipt: {run / 'receipt.json'}", file=sys.stderr)
        return 1
    print(f"AIRK Service {version} {TARGET} ({'signed' if receipt['signed'] else 'UNSIGNED'}), commit {commit}"
          f"{' (dirty)' if dirty else ''}: {receipt['installer']}")
    print(f"Receipt: {run / 'receipt.json'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
