# SPDX-License-Identifier: Apache-2.0
"""Reviewed local container qualification runner; requires explicit --execute."""
import argparse
import difflib
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
BASE = Path(__file__).resolve().parent


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_source_owner():
    spec = importlib.util.spec_from_file_location("container_source_owner", ROOT / "scripts/build-service-container.py")
    owner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owner)
    return owner


def qualification_dockerfile(original):
    edits = [
        ("FROM toolchain AS source", "FROM toolchain AS checked-toolchain\nRUN rustup show active-toolchain && rustup which cargo-clippy && rustup which rustfmt\nFROM checked-toolchain AS source"),
        ('true) python3 scripts/ci-metrics.py run --name test-compile -- cargo nextest run',
         'true) python3 scripts/ci-metrics.py run --name formatting -- cargo fmt --all --check \\\n        && python3 scripts/ci-metrics.py run --name clippy -- cargo clippy --workspace --all-targets --all-features --locked --offline --timings -- -D warnings \\\n        && python3 scripts/ci-metrics.py run --name lightweight-component-compile -- cargo check -p airk-cli --no-default-features --all-targets --locked --offline --timings \\\n        && python3 scripts/ci-metrics.py run --name test-compile -- cargo nextest run'),
        ('--locked --offline --no-run \\', '--locked --offline --no-run --timings \\'),
        ('&& python3 scripts/ci-prune-target.py',
         '&& test -d target/cargo-timings && cp -a target/cargo-timings /build/ci-report/cargo-timings \\\n    && python3 scripts/ci-prune-target.py'),
    ]
    value = original
    for before, after in edits:
        if value.count(before) != 1:
            raise RuntimeError("Canonical Dockerfile qualification edit no longer has one exact owner: " + before)
        value = value.replace(before, after)
    return value + """
FROM scratch AS retained-local-export
COPY --from=build /build/ci-report/ /reports/
COPY --from=build /build/out/runtime/ /runtime/

# Failure recovery reads only the existing cache: no source, compiler or tests.
FROM toolchain AS failed-cache-evidence
RUN --mount=type=cache,id=airk-linux-target,target=/build/target,uid=10001,gid=10001,sharing=locked \\
    mkdir -p /cache-evidence && \\
    if test -d /build/target/cargo-timings; then cp -a /build/target/cargo-timings /cache-evidence/cargo-timings; fi && \\
    if test -f /build/target/nextest/ci/junit.xml; then cp -a /build/target/nextest/ci/junit.xml /cache-evidence/junit.xml; fi
FROM scratch AS failed-cache-export
COPY --from=failed-cache-evidence /cache-evidence/ /
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--execute", action="store_true")
    parser.add_argument("--prepare", action="store_true", help="Write reviewable ignored derivative and diff without Docker execution")
    parser.add_argument("--label", default="linux-container-qualification-r1")
    args = parser.parse_args()
    if not args.label or any(char not in "abcdefghijklmnopqrstuvwxyz0123456789-" for char in args.label):
        parser.error("label must contain only lowercase letters, digits and hyphens")
    source_owner = load_source_owner()
    frozen = source_owner.container_source_identity(ROOT)
    output = BASE / args.label
    image = "airk:local-1814-" + frozen["sha256"][:12]
    # Public COPY inputs remain byte-identical. Explicit local qualification adds
    # missing native compile gates and retains HTML before canonical pruning.
    original = (ROOT / "Dockerfile").read_text(encoding="utf-8")
    derived = qualification_dockerfile(original)
    difference = "".join(difflib.unified_diff(original.splitlines(keepends=True), derived.splitlines(keepends=True),
                                            fromfile="Dockerfile", tofile="Dockerfile.local-qualification"))
    derivative = output / "Dockerfile.local-qualification"
    common = ["docker", "buildx", "build", "--builder", "desktop-linux", "--platform", "linux/amd64",
              "--file", str(derivative), "--build-arg", "AIRK_QUALIFICATION=true", "--progress", "plain"]
    commands = [
        ("cache-owner-regression", [sys.executable, "scripts/check-container-build-cache.py"]),
        ("full-container-build", common + ["--target", "smoke", "--load", "--tag", image, "."]),
        ("retained-report-export", common + ["--target", "retained-local-export",
                                            "--output", "type=local,dest=" + str(output / "export"), "."]),
        ("container-smoke", [sys.executable, "scripts/check-container.py", "--image", "<captured-image-id>",
                             "--report", str(output / "container-smoke.json")]),
    ]
    plan = {"schema": "airk-local-container-qualification/1", "source": frozen,
            "source_basis": "actual-public-container-copy-inputs/1", "platform": "linux/amd64",
            "builder": "desktop-linux", "cargo_target_directory": "/build/target (airk-linux-target locked cache mount)",
            "incremental": "off (canonical Dockerfile CARGO_INCREMENTAL=0)",
            "source_read_only": "Docker public COPY context; .dockerignore excludes artifacts, temp and private state",
            "dockerfile_sha256": digest(ROOT / "Dockerfile"), "dockerignore_sha256": digest(ROOT / ".dockerignore"),
            "derivative_dockerfile_sha256": hashlib.sha256(derived.encode()).hexdigest(),
            "derivative_diff": difference,
            "commands": commands, "stages": [], "passed": False,
            "coverage": ["formatting", "workspace all-target/all-feature strict Clippy", "lightweight all-target no-feature CLI compile",
                         "full all-feature workspace Nextest with four routine exclusions restored", "workspace doctests",
                         "Linux static ORT build and embedded model/import/runtime self-test", "offline one-container isolated collections and persistence smoke"],
            "not_covered": ["native systemd installation/upgrade lifecycle",
                            "external assistant UI/trust", "native Linux VSIX/editor acceptance", "other architectures"],
            "timing_limits": "Phase durations/JUnit and Cargo HTML for Clippy, lightweight, test compile and release-runtime retained before pruning."}
    if not args.execute:
        if args.prepare:
            review = BASE / "Dockerfile.linux-qualification-r1"
            review.write_text(derived, encoding="utf-8", newline="\n")
            Path(str(review) + ".diff").write_text(difference, encoding="utf-8", newline="\n")
            print("Prepared " + str(review), file=sys.stderr)
        print(json.dumps(plan, indent=2))
        return
    # All evidence remains ignored. Existing runs and user artifacts are never replaced.
    ignored = subprocess.run(["git", "-c", "safe.directory=" + ROOT.as_posix(), "check-ignore", str(output)],
                             cwd=ROOT, capture_output=True, text=True)
    if ignored.returncode != 0:
        raise RuntimeError("Qualification evidence directory is not ignored")
    output.mkdir(exist_ok=False)
    derivative.write_text(derived, encoding="utf-8", newline="\n")
    (output / "Dockerfile.local-qualification.diff").write_text(difference, encoding="utf-8", newline="\n")
    receipt = output / "receipt.json"

    def retain():
        receipt.write_text(json.dumps(plan, indent=2) + "\n", encoding="utf-8")

    def unchanged():
        if source_owner.container_source_identity(ROOT) != frozen:
            raise RuntimeError("Frozen public container COPY inputs changed; retain this run without relabeling its source")

    def run(name, command):
        unchanged()
        started = time.monotonic()
        row = {"name": name, "command": command, "log": name + ".log", "status": "running"}
        plan["stages"].append(row)
        retain()
        print("Starting " + name, flush=True)
        with (output / row["log"]).open("wb") as log:
            completed = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
        row.update(exit_code=completed.returncode, elapsed_seconds=time.monotonic() - started,
                   status="passed" if completed.returncode == 0 else "failed")
        retain()
        if completed.returncode:
            raise RuntimeError(name + " failed; complete log retained at " + row["log"])
        unchanged()

    def recover_failed_cache():
        # This target depends only on the already-used toolchain and cache. It
        # cannot rerun a failed test/compile RUN or turn old cache files into passes.
        target = output / "failed-cache-evidence"
        command = common + ["--target", "failed-cache-export", "--output", "type=local,dest=" + str(target), "."]
        started = time.monotonic()
        row = {"name": "failed-cache-evidence-export", "command": command,
               "started_utc": datetime.now(timezone.utc).isoformat(),
               "scope": "No Cargo or test command; existing shared cache files may be historical and are not qualification results."}
        plan["failed_cache_recovery"] = row
        retain()
        with (output / "failed-cache-evidence-export.log").open("wb") as log:
            observed = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
        row.update(exit_code=observed.returncode, elapsed_seconds=time.monotonic() - started,
                   completed_utc=datetime.now(timezone.utc).isoformat())
        row["files"] = [{"path": str(path.relative_to(output)), "sha256": digest(path),
                         "bytes": path.stat().st_size, "exported_mtime_ns": path.stat().st_mtime_ns,
                         "freshness": "not_certified; may predate this failed run"}
                        for path in sorted(target.rglob("*")) if path.is_file()] if target.exists() else []
        retain()

    try:
        for name, command in commands:
            if name == "container-smoke":
                inspected = subprocess.run(["docker", "image", "inspect", image, "--format", "{{.Id}}"],
                                           cwd=ROOT, capture_output=True, text=True, check=True)
                identity = inspected.stdout.strip()
                if not identity.startswith("sha256:") or len(identity) != 71:
                    raise RuntimeError("Unexpected immutable Docker image identity")
                plan["image_id"] = identity
                command = [identity if value == "<captured-image-id>" else value for value in command]
            run(name, command)
        retained = output / "export"
        build = json.loads((retained / "runtime/airk-build.json").read_text(encoding="utf-8"))
        if build["source"] != frozen or build["source_basis"] != plan["source_basis"]:
            raise RuntimeError("Exported runtime does not bind the frozen public COPY source")
        plan["runtime_manifest_sha256"] = digest(retained / "runtime/airk-install.json")
        plan["cargo_timings"] = [str(path.relative_to(output)) for path in (retained / "reports/cargo-timings").glob("*.html")]
        if not plan["cargo_timings"]:
            raise RuntimeError("No retained Cargo HTML timing evidence")
        unchanged()
        plan["passed"] = True
    except BaseException as error:
        plan["failure"] = {"type": type(error).__name__, "message": str(error)}
        # Only a failed build can own failed compiler/test cache evidence. A
        # smoke refusal does not justify another build or an expensive retry.
        if any(row["name"] == "full-container-build" and row.get("status") == "failed" for row in plan["stages"]):
            try:
                recover_failed_cache()
            except BaseException as recovery:
                plan["failure_cache_export_error"] = {"type": type(recovery).__name__, "message": str(recovery)}
        raise
    finally:
        retain()


if __name__ == "__main__":
    main()
