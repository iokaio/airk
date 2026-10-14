# SPDX-License-Identifier: Apache-2.0
"""Package an already-built host binary; never archive the checkout or index state."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parent.parent
TARGETS = {
    "x86_64-unknown-linux-gnu": "linux-x64",
    "x86_64-pc-windows-msvc": "win32-x64",
    "aarch64-apple-darwin": "darwin-arm64",
    "x86_64-apple-darwin": "darwin-x64",
}


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def capture(*command):
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def inventory(stage):
    result = {}
    for path in sorted(stage.rglob("*")):
        if path.is_symlink():
            raise ValueError("Payload links are forbidden")
        if path.is_file():
            result[path.relative_to(stage).as_posix()] = sha256(path)
    return result


def verify_archive(archive, expected, executable):
    with zipfile.ZipFile(archive) as zipped:
        names = zipped.namelist()
        if len(names) != len(set(names)) or set(names) != set(expected):
            raise ValueError("Archive entries differ from payload inventory")
        for name, digest in expected.items():
            if hashlib.sha256(zipped.read(name)).hexdigest() != digest:
                raise ValueError("Archive payload digest mismatch: " + name)
        if not (zipped.getinfo(executable).external_attr >> 16) & stat.S_IXUSR:
            raise ValueError("Archive lost executable permission")


def write_archive(stage, archive, expected, executable):
    with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED) as zipped:
        for name in sorted(expected):
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = (stat.S_IFREG | (0o755 if name == executable else 0o644)) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            zipped.writestr(info, (stage / name).read_bytes())
    verify_archive(archive, expected, executable)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--output", type=Path, help="Fresh directory inside ignored artifacts/; defaults to artifacts/native/<target>")
    args = parser.parse_args()
    compiler = capture("rustc", "-vV")
    if "host: " + args.target not in compiler.splitlines():
        raise SystemExit("Package and smoke-test on the intended native runner")
    metadata = json.loads(capture("cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"))
    version = next(p["version"] for p in metadata["packages"] if p["name"] == "airk-cli")
    executable = "airk.exe" if "windows" in args.target else "airk"
    binary = ROOT / "target" / args.target / "release" / executable
    if capture(str(binary), "--version") != "airk " + version:
        raise SystemExit("Built executable and package version differ")
    output = (args.output or ROOT / "artifacts" / "native" / args.target).resolve()
    if not output.is_relative_to((ROOT / "artifacts").resolve()) or output == (ROOT / "artifacts").resolve():
        raise SystemExit("Candidate output must be a fresh subdirectory of artifacts/")
    # Fresh staging prevents stale files from entering a new candidate.
    output.mkdir(parents=True, exist_ok=False)
    stage = output / "stage"
    publish = output / "publish"
    stage.mkdir()
    publish.mkdir()
    shutil.copyfile(binary, stage / executable)
    (stage / executable).chmod(0o755)
    for name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"):
        shutil.copyfile(ROOT / name, stage / name)
    subprocess.run([sys.executable, str(ROOT / "scripts" / "license-notices.py"),
                    "--output", str(stage / "notices")], cwd=ROOT, check=True)
    files = inventory(stage)
    archive = publish / ("airk-" + version + "-" + args.target + ".zip")
    write_archive(stage, archive, files, executable)
    # Exercise the archived binary, rather than only the build-tree executable.
    smoke = output / "smoke"
    smoke.mkdir()
    with zipfile.ZipFile(archive) as zipped:
        (smoke / executable).write_bytes(zipped.read(executable))
    (smoke / executable).chmod(0o755)
    if capture(str(smoke / executable), "--version") != "airk " + version:
        raise SystemExit("Archived executable version differs")
    capture(str(smoke / executable), "--help")
    receipt = {
        "schema": "airk-native-candidate/1", "version": version,
        "target": args.target, "vsix_target": TARGETS[args.target],
        "commit": capture("git", "rev-parse", "HEAD"),
        "dirty_checkout": bool(capture("git", "status", "--porcelain")),
        "cargo_lock_sha256": sha256(ROOT / "Cargo.lock"),
        "toolchain_file_sha256": sha256(ROOT / "rust-toolchain.toml"),
        "rustc": compiler, "cargo": capture("cargo", "--version"),
        "archive": archive.name, "archive_sha256": sha256(archive),
        "payload": files, "signed_distribution": False,
        "qualification": "native CLI candidate; installed editor and oldest OS not qualified",
    }
    (publish / "manifest.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    (publish / "SHA256SUMS").write_text(
        sha256(archive) + "  " + archive.name + "\n" +
        sha256(publish / "manifest.json") + "  manifest.json\n", encoding="utf-8")
    print("Verified native candidate: " + str(archive))


if __name__ == "__main__":
    main()
