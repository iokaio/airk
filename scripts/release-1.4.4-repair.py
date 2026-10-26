# SPDX-License-Identifier: Apache-2.0
"""One-release repair: retain the tested 1.4.4 source, review tooling separately.

This is deliberately not a general ancestor-artifact promotion mechanism.
Remove this entry point after 1.4.4; ordinary releases keep exact-main promotion.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent
SOURCE = "3c64111b445821cf8db36e2545042c1ee1cb6fcc"
RUN = "40369075574"
VERSION = "1.4.4"
RUNTIME = {"sha256": "2c12276e161cf04ee52765086159cbc21e595c7386c705507392ec30db901000", "files": 664}
ALLOWED = frozenset({
    ".github/workflows/github-release.yml", "scripts/github-release.py",
    ".github/actions/macos-ticket-network/action.yml", ".github/workflows/macos-signing.yml",
    "scripts/tests/test_ci_macos_signing.py",
    "scripts/release-1.4.4-repair.py", "scripts/tests/test_github_release.py",
    "scripts/tests/test_release_1_4_4_repair.py", "scripts/ci-plan.py", "scripts/tests/test_ci_plan.py",
    "README.md", "CONTRIBUTING.md", "docs/README.md", "docs/ci-and-builds.md",
    "docs/release-workflow.md", "docs/release-1.4.4.md", "docs/native-signing.md",
})


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / filename)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


release = module("repair_release", "github-release.py")
producers = release.producers


def git(*arguments, directory=ROOT):
    return subprocess.check_output(["git", "-c", f"safe.directory={directory.as_posix()}", *arguments], cwd=directory)


def validate_scope(paths, version, runtime):
    if not paths or not set(paths) <= ALLOWED or version != VERSION or runtime != RUNTIME:
        raise ValueError("The 1.4.4 repair cannot change runtime, extension, version or unrelated inputs")


def scope():
    """Complete cumulative no-renames diff, not just this push's changed files."""
    tooling = git("rev-parse", "HEAD").decode().strip()
    if git("merge-base", SOURCE, tooling).decode().strip() != SOURCE:
        raise ValueError("Repair tooling must descend from the original tested main commit")
    if git("status", "--porcelain").strip():
        raise ValueError("Repair tooling checkout is dirty")
    paths = git("diff", "--name-only", "--no-renames", "-z", SOURCE, tooling).decode("utf-8", errors="strict").split("\0")
    paths = [path for path in paths if path]
    version = json.loads((ROOT / "packages/vscode/package.json").read_text(encoding="utf-8"))["version"]
    runtime = release.native.runtime_builder().source_identity()
    validate_scope(paths, version, runtime)
    return {"schema": "airk-release-tooling-repair/1", "source_commit": SOURCE,
            "tooling_commit": tooling, "producer_run_id": RUN, "producer_attempt": "1",
            "runtime_source": runtime, "changed_paths": sorted(paths)}


def context():
    tooling, version, tag = release.context()  # Real workflow/main identity; never spoof GITHUB_SHA.
    evidence = scope()
    if tooling != evidence["tooling_commit"] or version != VERSION or os.environ.get("AIRK_CHANNEL", "stable") != "stable":
        raise ValueError("Repair requires stable 1.4.4 on the checked-out protected main tooling commit")
    return evidence


def producer():
    record = json.loads((ROOT / "artifacts/producer.json").read_text(encoding="utf-8"))
    if record.get("run_id") != RUN or record.get("attempt") != "1" or record.get("commit") != SOURCE:
        raise ValueError("Repair producer differs from the pinned successful main attempt")
    producers.verify_record(record, SOURCE)
    return record


def recorded(evidence):
    if json.loads((ROOT / "artifacts/repair.json").read_text(encoding="utf-8")) != evidence:
        raise ValueError("Repair tooling evidence changed after preflight")
    return producer()


def original_checkout():
    source = ROOT / "artifacts/repair-source"
    if source.is_symlink() or not source.is_dir() or source.resolve() != source:
        raise ValueError("Expected the separate original-source checkout")
    if git("rev-parse", "HEAD", directory=source).decode().strip() != SOURCE or git("status", "--porcelain", directory=source).strip():
        raise ValueError("Signing/packaging source must remain the clean original tested commit")
    return source


def verify_repair_bundle(directory, evidence):
    record = release.extension.read_receipt(directory / "release.json")
    if record.get("release_tooling_repair") != evidence or record.get("producer") != producer():
        raise ValueError("Release bundle does not retain the pinned source and current repair tooling")
    release.verify_bundle(directory, SOURCE, VERSION, "v" + VERSION, os.environ["GITHUB_RUN_ID"], record["attempt"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("preflight", "download", "stage-signing", "assemble", "bundle", "publish"))
    parser.add_argument("--directory", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--target", choices=producers.TARGETS)
    parser.add_argument("--producer", type=Path)  # Same invocation shape as ordinary assembly.
    parser.add_argument("--channel", choices=("stable",), default="stable")
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    evidence = context()
    if args.command == "preflight":
        if os.environ.get("AIRK_CANDIDATE_RUN_ID", "") not in ("", RUN):
            raise ValueError("Repair cannot select another producer")
        release.require_unused_tag("v" + VERSION)
        record = producers.attest(RUN, SOURCE)
        if record["attempt"] != "1":
            raise ValueError("Original producer attempt changed")
        (ROOT / "artifacts").mkdir(exist_ok=True)
        for name, value in (("producer", record), ("repair", evidence)):
            (ROOT / f"artifacts/{name}.json").write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            action = "publish" if os.environ.get("AIRK_PUBLISH") == "true" else "rehearse"
            output.write(f"version={VERSION}\ntag=v{VERSION}\naction={action}\nsource_commit={SOURCE}\n")
    elif args.command == "bundle":
        producers.release_bundle(evidence["tooling_commit"], os.environ["GITHUB_RUN_ID"], os.environ["GITHUB_RUN_ATTEMPT"], args.output)
        verify_repair_bundle(args.output, evidence)
    elif args.command == "publish":
        verify_repair_bundle(args.directory, evidence)
        release.publish(args.directory, resume=args.resume, source_commit=SOURCE)
    else:
        record = recorded(evidence)
        if args.command == "download":
            if args.target:
                original_checkout()
            for name, artifact in record["artifacts"].items():
                if name.startswith("airk-native-") and (not args.target or name.startswith(f"airk-native-{args.target}-")):
                    destination = args.output if args.target else args.output / name
                    producers.download(artifact, destination)
                    target = next(target for target in producers.TARGETS if name.startswith(f"airk-native-{target}-"))
                    receipt, _ = release.verify_native(destination, target, SOURCE, VERSION, False)
                    if receipt.get("source") != RUNTIME:
                        raise ValueError("Native artifact runtime source differs from the pinned tested inputs")
        elif args.command == "stage-signing":
            release.stage_signing(args.directory, args.target, SOURCE, VERSION, output_root=original_checkout())
        else:
            release.assemble(args.directory, args.output, SOURCE, VERSION, "v" + VERSION, "stable",
                             os.environ["GITHUB_RUN_ID"], os.environ["GITHUB_RUN_ATTEMPT"], record, repair=evidence)


if __name__ == "__main__":
    main()
