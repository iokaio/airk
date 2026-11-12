# SPDX-License-Identifier: Apache-2.0
"""Force a local universal AIRK VS Code Extension VSIX build (split plan §10.5.1, U21, U29, U37).

Entry point: scripts/build-local-vsix.ps1 -ForceBuild. Steps, stopping at the first failure:
prerequisites; the locked `npm ci` in packages/vscode; the I2 generated-type drift check
(scripts/dev/contract_types.py i2 --check); host/view compilation (`npm run compile`); UI-only
packaging and content verification by scripts/package-extension.py; a receipt. No native build,
service installation or Marketplace publication is involved; nothing is installed or published.

Every run writes a fresh folder under artifacts/local-vsix/; no earlier VSIX is reused, and a
failed run reports failure with its receipt instead of an older artifact.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
EXTENSION = ROOT / "packages/vscode"
OUTPUT = ROOT / "artifacts/local-vsix"
TOOLS = ("node", "npm", "git")


def load_script(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), ROOT / "scripts" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def prerequisites(which=shutil.which):
    """Resolve every tool before any step runs; a missing tool stops the build early."""
    found = {tool: which(tool) for tool in TOOLS}
    missing = [tool for tool, path in found.items() if path is None]
    if missing:
        raise SystemExit("Missing prerequisites: " + ", ".join(missing))
    return found


def extension_version():
    return json.loads((EXTENSION / "package.json").read_text(encoding="utf-8"))["version"]


def source_state():
    """Commit, tree and dirty paths of the checkout this build reads."""
    def git(*arguments):
        return subprocess.check_output(["git", *arguments], cwd=ROOT, text=True, encoding="utf-8").strip()
    status = [line for line in git("status", "--porcelain").splitlines() if line.strip()]
    return {"commit": git("rev-parse", "HEAD"), "tree": git("rev-parse", "HEAD^{tree}"),
            "dirty": bool(status), "dirty_paths": status[:200]}


def build(run, channel, tools, steps, receipts):
    """The ordered VSIX sequence; returns the verified universal receipt and VSIX path."""
    steps.run("npm-ci", [tools["npm"], "ci", "--ignore-scripts", "--no-audit", "--no-fund"], cwd=EXTENSION)
    steps.run("contracts-i2", [sys.executable, ROOT / "scripts/dev/contract_types.py", "i2", "--check"])
    steps.run("compile", [tools["npm"], "run", "compile"], cwd=EXTENSION)
    output = run / "vsix"
    steps.run("package", [sys.executable, ROOT / "scripts/package-extension.py", "--channel", channel,
                          "--output", output])
    manifest = output / "publish/manifest.json"
    receipt = json.loads(manifest.read_text(encoding="utf-8"))
    receipts.validate_vsix(receipt)
    vsix = output / "publish" / receipt["archive"]
    with vsix.open("rb") as stream:
        if hashlib.file_digest(stream, "sha256").hexdigest() != receipt["archive_sha256"]:
            raise RuntimeError("Packaged VSIX differs from its receipt")
    return receipt, vsix, manifest


def main(argv=None, *, which=shutil.which, steps_factory=None, clock=time.strftime):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--force-build", action="store_true", required=True,
                        help="Build from this checkout; completed VSIXs are never reused")
    parser.add_argument("--channel", choices=("stable", "preview"), default="stable")
    args = parser.parse_args(argv)
    tools = prerequisites(which)
    receipts = load_script("split_receipts")
    version = extension_version()
    if not receipts.split_version(version):
        raise SystemExit(f"Version {version} predates the split; increment the patch version before a local build")
    source = source_state()
    run = OUTPUT / clock("%Y%m%d-%H%M%S")
    run.mkdir(parents=True, exist_ok=False)
    steps = (steps_factory or load_script("build-local-service-installer").Steps)(run)
    record = {"schema": "airk-local-vsix/1", "version": version, "target": "universal", "channel": args.channel,
              **source, "result": "failed",
              "marketplace_signature": "none; Marketplace signing happens only at authorized publication"}
    try:
        receipt, vsix, manifest = build(run, args.channel, tools, steps, receipts)
        if (receipt["version"], receipt["commit"], receipt["tree"]) != (version, source["commit"], source["tree"]):
            raise RuntimeError("Packaged VSIX identity differs from this checkout")
        record.update({"result": "built", "vsix": str(vsix), "vsix_sha256": receipt["archive_sha256"],
                       "vsix_receipt": str(manifest), "ui_inventory_sha256": receipt["ui_inventory_sha256"],
                       "i2_contracts": receipt["i2_contracts"]})
    except Exception as error:  # Reported with the receipt; no artifact is announced.
        record["error"] = f"{type(error).__name__}: {error}"
    finally:
        record["steps"] = steps.records
        (run / "receipt.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    if record["result"] != "built":
        print(f"FAILED: {record['error']}", file=sys.stderr)
        print(f"Receipt: {run / 'receipt.json'}", file=sys.stderr)
        return 1
    print(f"AIRK VS Code Extension {version} universal ({args.channel}), commit {source['commit']}"
          f"{' (dirty)' if source['dirty'] else ''}: {record['vsix']}")
    print(f"Receipt: {run / 'receipt.json'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
