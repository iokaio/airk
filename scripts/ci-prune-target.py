# SPDX-License-Identifier: Apache-2.0
"""Remove workspace outputs before exporting the Docker dependency cache."""
import json
from pathlib import Path
import shutil
import subprocess


def main():
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"]))
    target = Path(metadata["target_directory"]).resolve()
    if target.parent != Path.cwd().resolve() or target.name != "target":
        raise ValueError("Dependency pruning requires the checkout's own target directory")
    packages = metadata["packages"]
    names = {t["name"].replace("-", "_") for p in packages for t in p["targets"]}
    names.update(p["name"].replace("-", "_") for p in packages)
    for profile in (target / "debug", target / "release"):
        if not profile.is_dir():
            continue
        if profile.is_symlink():
            raise ValueError("Linked cache profile rejected")
        candidates = [profile / "incremental", profile / "examples"]
        for directory in (profile / "deps", profile / "build", profile / ".fingerprint", profile):
            if not directory.is_dir():
                continue
            if directory.is_symlink():
                raise ValueError("Linked compiler cache directory rejected")
            for path in directory.iterdir():
                stem = path.name.split(".", 1)[0].replace("-", "_")
                if any(stem == name or stem.startswith(name + "_") or
                       stem == "lib" + name or stem.startswith("lib" + name + "_") for name in names):
                    candidates.append(path)
        for path in candidates:
            if not path.parent.resolve().is_relative_to(target):
                raise ValueError("Cache cleanup parent escaped the Cargo target directory")
            if path.is_symlink():
                path.unlink()
            elif path.exists():
                if not path.resolve().is_relative_to(target):
                    raise ValueError("Cache cleanup escaped the Cargo target directory")
                if path.is_dir():
                    shutil.rmtree(path)
                else:
                    path.unlink()


if __name__ == "__main__":
    main()
