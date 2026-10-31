# SPDX-License-Identifier: Apache-2.0
"""Build an installation-free AIRK runtime for this platform under builds/.

Shared by build.ps1 and build.sh. The output folder name starts with a UTC
timestamp (YYYYMMDD-HHMMSS) so folders sort chronologically. Nothing is
installed: no PATH, profile, login-startup or per-user runtime changes.
"""
import argparse
from datetime import datetime, timezone
from pathlib import Path
import platform
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
OPERATING_SYSTEMS = {"Windows": "windows", "Darwin": "macos", "Linux": "linux"}
ARCHITECTURES = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}

WINDOWS_SETUP = r"""```powershell
$airk = "<build>\{bin}"
$test = "$env:TEMP\airk-experiment"
New-Item -ItemType Directory -Force "$test\source", "$test\data" | Out-Null
Set-Content "$test\source\notes.md" "# Copper harbor`nThe copper harbor ledger records tides."
$control = "$test\control"

# 1. Start the service in its own terminal (Ctrl+C or step 6 stops it).
& $airk serve --runtime-dir $control

# 2. In a second terminal (set $airk, $test and $control again), wait until it is healthy.
& $airk service health --runtime-dir $control

# 3. Prepare and register a project.
$project = & $airk --root "$test\source" --data-dir "$test\data" --collection demo service prepare-registration
& $airk --root "$test\source" --data-dir "$test\data" --collection demo service register --runtime-dir $control
$binding = ($project | ConvertFrom-Json).binding
$scope = @('--service-runtime-dir', $control, '--service-project', $binding, '--root', "$test\source", '--data-dir', "$test\data", '--collection', 'demo')

# 4. Index, check status and search.
& $airk @scope index
& $airk @scope tools call airk_status --args '{{}}'
& $airk @scope search "copper harbor" --mode lexical

# 5. Optional: serve MCP over stdio for an agent or an MCP inspector.
& $airk @scope mcp

# 6. Stop the service.
& $airk service shutdown --runtime-dir $control
```"""

POSIX_SETUP = r"""```sh
airk="<build>/{bin}"
test="${{TMPDIR:-/tmp}}/airk-experiment"
mkdir -p -m 700 "$test" "$test/source" "$test/data"
printf '# Copper harbor\nThe copper harbor ledger records tides.\n' > "$test/source/notes.md"
control="$test/control"

# 1. Start the service in its own terminal (Ctrl+C or step 6 stops it).
"$airk" serve --runtime-dir "$control"

# 2. In a second terminal (set airk, test and control again), wait until it is healthy.
"$airk" service health --runtime-dir "$control"

# 3. Prepare and register a project.
project=$("$airk" --root "$test/source" --data-dir "$test/data" --collection demo service prepare-registration)
"$airk" --root "$test/source" --data-dir "$test/data" --collection demo service register --runtime-dir "$control"
binding=$(printf '%s' "$project" | python3 -c 'import json,sys; print(json.load(sys.stdin)["binding"])')
set -- --service-runtime-dir "$control" --service-project "$binding" --root "$test/source" --data-dir "$test/data" --collection demo

# 4. Index, check status and search.
"$airk" "$@" index
"$airk" "$@" tools call airk_status --args '{{}}'
"$airk" "$@" search "copper harbor" --mode lexical

# 5. Optional: serve MCP over stdio for an agent or an MCP inspector.
"$airk" "$@" mcp

# 6. Stop the service.
"$airk" service shutdown --runtime-dir "$control"
```"""


def git(*args):
    try:
        return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        return ""


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--profile", choices=["release", "debug"], default="release")
    args = parser.parse_args()
    operating_system = OPERATING_SYSTEMS.get(platform.system())
    architecture = ARCHITECTURES.get(platform.machine().lower())
    if not operating_system or not architecture:
        raise SystemExit(f"Unsupported build host: {platform.system()} {platform.machine()}")
    built = datetime.now(timezone.utc)
    name = f"{built:%Y%m%d-%H%M%S}-{operating_system}-{architecture}"
    output = ROOT / "builds" / name
    if output.exists():
        raise SystemExit(f"Build folder already exists: {output}")
    # The runtime builder stages, verifies and seals the complete offline payload.
    subprocess.run([sys.executable, str(ROOT / "scripts" / "build-service-runtime.py"),
                    "--destination", str(output / "runtime"), "--profile", args.profile], cwd=ROOT, check=True)
    suffix = ".exe" if operating_system == "windows" else ""
    prefix = "runtime/AIRK.app/Contents/MacOS/" if operating_system == "macos" else "runtime/"
    binary, embed = f"{prefix}airk{suffix}", f"{prefix}airk-embed{suffix}"
    if not (output / binary).is_file() or not (output / embed).is_file():
        raise SystemExit("The runtime builder did not produce the expected engine pair")
    native_bin = binary.replace("/", "\\") if operating_system == "windows" else binary
    setup = (WINDOWS_SETUP if operating_system == "windows" else POSIX_SETUP).format(bin=native_bin)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    values = {"VERSION": version, "PLATFORM": f"{operating_system}-{architecture}", "BUILT": f"{built:%Y-%m-%d %H:%M:%S} UTC",
              "COMMIT": git("rev-parse", "HEAD") or "unknown", "DIRTY": " (with uncommitted changes)" if git("status", "--porcelain") else "",
              "PROFILE": args.profile, "BIN": binary, "EMBED": embed, "SETUP": setup}
    readme = (ROOT / "scripts" / "local-build-README.md").read_text(encoding="utf-8")
    for key, value in values.items():
        readme = readme.replace("{{" + key + "}}", value)
    (output / "README.md").write_text(readme, encoding="utf-8", newline="\n")
    print(f"Local AIRK {version} build ({args.profile}): {output}")
    print(f"Start with: {output / binary} serve --runtime-dir <private-control-directory>; see README.md")


if __name__ == "__main__":
    main()
