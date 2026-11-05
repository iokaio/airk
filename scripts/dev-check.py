# SPDX-License-Identifier: Apache-2.0
"""Scoped development Clippy with a persistent cache; not phase qualification."""
import argparse
import os
from pathlib import Path
import shlex
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    scope = parser.add_mutually_exclusive_group(required=True)
    scope.add_argument("-p", "--package", action="append", dest="packages",
                       help="affected crate; repeat for multiple crates")
    scope.add_argument("--workspace", action="store_true",
                       help="explicitly lint the whole workspace")
    features = parser.add_mutually_exclusive_group()
    features.add_argument("--all-features", action="store_true")
    features.add_argument("--features", help="additional comma-separated features")
    parser.add_argument("--target-dir", type=Path,
                        help="reuse a specific cache; otherwise CARGO_TARGET_DIR or checkout/target")
    parser.add_argument("--incremental", choices=("on", "off"),
                        help="default: on for Windows, off elsewhere to save disk space")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--cargo", default="cargo", help="Cargo executable or absolute path")
    parser.add_argument("--skip-fmt", action="store_true",
                        help="reuse an already completed formatting check")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args(argv)
    environment = os.environ.copy()
    target = args.target_dir or Path(environment.get("CARGO_TARGET_DIR") or ROOT / "target")
    if not target.is_absolute():
        target = ROOT / target
    environment["CARGO_TARGET_DIR"] = str(target.resolve())
    enabled = args.incremental == "on" if args.incremental else sys.platform == "win32"
    environment["CARGO_INCREMENTAL"] = "1" if enabled else "0"
    commands = []
    if not args.skip_fmt:
        commands.append([args.cargo, "fmt", "--all", "--", "--check"])
    clippy = [args.cargo, "clippy", "--locked", "--all-targets", "--timings"]
    if args.offline:
        clippy.append("--offline")
    if args.workspace:
        clippy.append("--workspace")
    else:
        for package in args.packages:
            clippy.extend(["-p", package])
    if args.all_features:
        clippy.append("--all-features")
    else:
        clippy.append("--no-default-features")
        if args.features:
            clippy.extend(["--features", args.features])
    clippy.extend(["--", "-D", "warnings"])
    commands.append(clippy)
    print("Development checks only; complete CONTRIBUTING.md qualification separately.", flush=True)
    print(f"CARGO_TARGET_DIR={environment['CARGO_TARGET_DIR']}", flush=True)
    print(f"CARGO_INCREMENTAL={environment['CARGO_INCREMENTAL']}", flush=True)
    for command in commands:
        print(shlex.join(command), flush=True)
        if args.dry_run:
            continue
        try:
            result = subprocess.run(command, cwd=ROOT, env=environment, check=False)
        except FileNotFoundError:
            parser.exit(2, "Cargo was not found; run the contributor bootstrap or pass --cargo.\n")
        if result.returncode:
            return result.returncode
    if not args.dry_run:
        print(f"Cargo timing reports: {target / 'cargo-timings'}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
