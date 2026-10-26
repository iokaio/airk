# SPDX-License-Identifier: Apache-2.0
"""Stage a platform VSIX from a verified native candidate and compiled AIRK modules."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shutil
import stat
import subprocess
import zipfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parent.parent
EXTENSION = ROOT / "packages" / "vscode"
TARGETS = {"x86_64-pc-windows-msvc": "win32-x64", "x86_64-unknown-linux-gnu": "linux-x64",
           "aarch64-apple-darwin": "darwin-arm64", "x86_64-apple-darwin": "darwin-x64"}
MODULES = ("extension.js", "serviceExtension.js", "serviceDashboard.js", "serviceWorkspace.js", "model.js", "native.js", "cliInstall.js",
           "usagePanel.js", "usageReport.js", "report.js",
           "agentConnections.js", "agentConfig.js", "agentMigration.js", "agentSetup.js", "serviceBootstrap.js", "serviceClient.js",
           "serviceConsole.js", "serviceConsoleController.js", "serviceConsoleView.js", "serviceIntents.js",
           "mcpStdioSession.js", "explorerSidebar.js", "explorerStatus.js", "explorerController.js", "explorerControl.js",
           "explorerContracts.js", "explorerErrors.js", "explorerEvidence.js", "explorerFeatures.js", "explorerPanel.js",
           "explorerRoutes.js", "explorerSearch.js", "explorerSource.js", "explorerWorkspaceIO.js",
           "explorerFacts.js", "explorerInspect.js")
# esbuild outputs; referenced by URI or path from host modules, never require()d.
BUNDLES = ("explorerWebview.js", "explorerWebview.css")
# Host-only implementations that must never ship a second bundled copy.
HOST_MARKERS = ("class ServiceClient", "class NativeControlTransport", "class IntentJournal", "function probeMcp",
                "class ServiceConsoleController", "class RuntimeHost")
JSONC_FILES = ("package.json", "LICENSE.md", "lib/umd/main.js", "lib/umd/impl/edit.js",
               "lib/umd/impl/format.js", "lib/umd/impl/parser.js", "lib/umd/impl/scanner.js",
               "lib/umd/impl/string-intern.js")
ILLUSTRATIONS = ("local-evidence.png", "index-to-context.png")
ICONS = ("airk.svg", "icon.png")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def safe_name(name):
    parts = name.split("/")
    if not name or "\\" in name or ":" in name or any(p in ("", ".", "..") for p in parts):
        raise ValueError("Unsafe archive path: " + name)


def stage_runtime_modules(compiled, destination):
    """Stage only reviewed CommonJS modules after checking literal local imports.

    The compiler emits require("./module") for runtime TypeScript imports. This
    conservative check deliberately does not discover or package extra files.
    """
    modules = {}
    for name in MODULES:
        source = compiled / name
        if source.is_symlink() or not source.is_file() or source.stat().st_size > 4_194_304:
            raise ValueError("Invalid or oversized runtime module: " + name)
        with source.open("rb") as stream:
            data = stream.read(4_194_305)
        if len(data) > 4_194_304:
            raise ValueError("Oversized runtime module: " + name)
        for match in re.finditer(r'''\b(?:require|import)\s*\(\s*(["'])(\.[^"']*)\1\s*\)''', data.decode("utf-8")):
            relative = match.group(2)
            target = relative.removeprefix("./")
            if not target.endswith(".js"):
                target += ".js"
            if not relative.startswith("./") or target not in MODULES:
                raise ValueError("Runtime import outside allowlisted stage: " + name + " -> " + relative)
        modules[name] = data
    destination.mkdir(parents=True, exist_ok=True)
    for name, data in modules.items():
        (destination / name).write_bytes(data)


def stage_bundles(compiled, destination, preact_version):
    """Stage reviewed esbuild bundles after checking they are self-contained.

    Bundles carry their third-party notice in a leading banner and may not load
    local modules, the VS Code API or a copy of a host-owned implementation.
    """
    bundles = {}
    for name in BUNDLES:
        source = compiled / name
        if source.is_symlink() or not source.is_file() or source.stat().st_size > 4_194_304:
            raise ValueError("Invalid or oversized bundle: " + name)
        with source.open("rb") as stream:
            data = stream.read(4_194_305)
        if len(data) > 4_194_304:
            raise ValueError("Oversized bundle: " + name)
        text = data.decode("utf-8")
        if name.endswith(".js"):
            if re.search(r'''\b(?:require|import)\s*\(\s*["']\.''', text):
                raise ValueError("Bundle loads a local module: " + name)
            if re.search(r'''\b(?:require|import)\s*\(\s*["']vscode["']''', text):
                raise ValueError("Bundle loads the VS Code API: " + name)
            marker = next((value for value in HOST_MARKERS if value in text), None)
            if marker:
                raise ValueError("Bundle contains host implementation " + marker + ": " + name)
        if name == "explorerWebview.js":
            if re.search(r"\brequire\s*\(", text):
                raise ValueError("Webview bundle must not require modules")
            if not text.startswith("/*! AIRK Explorer webview. Includes Preact " + preact_version + " (MIT).\n"):
                raise ValueError("Webview bundle lacks the pinned Preact notice")
        bundles[name] = data
    destination.mkdir(parents=True, exist_ok=True)
    for name, data in bundles.items():
        (destination / name).write_bytes(data)


def stage_jsonc_parser(source, destination):
    """Ship only the pinned parser's runtime closure and its original MIT notice."""
    metadata = json.loads((source / "package.json").read_text(encoding="utf-8"))
    if (metadata["name"], metadata["version"], metadata["main"], metadata["license"]) != (
            "jsonc-parser", "3.3.1", "./lib/umd/main.js", "MIT"):
        raise ValueError("Unexpected jsonc-parser identity")
    files = {}
    root = source.resolve()
    for name in JSONC_FILES:
        file = source / name
        if file.is_symlink() or not file.resolve().is_relative_to(root) or not file.is_file() or file.stat().st_size > 1_048_576:
            raise ValueError("Invalid jsonc-parser runtime file: " + name)
        data = file.read_bytes()
        if name.endswith(".js"):
            for match in re.finditer(r'''\brequire\s*\(\s*(["'])([^"']*)\1\s*\)''', data.decode("utf-8")):
                relative = match.group(2)
                target = (file.parent / (relative + ".js")).resolve()
                if not relative.startswith("./") or not target.is_relative_to(root) or target.relative_to(root).as_posix() not in JSONC_FILES:
                    raise ValueError("Unpackaged jsonc-parser dependency: " + relative)
        files[name] = data
    for name, data in files.items():
        target = destination / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)


def verify_vsix(archive, expected, executable, platform=None, version=None, pre_release=None, display_name=None):
    with zipfile.ZipFile(archive) as zipped:
        files = [item for item in zipped.infolist() if not item.is_dir()]
        if len(files) > 15_000 or sum(item.file_size for item in files) > 1_073_741_824:
            raise ValueError("VSIX inventory exceeds verification budget")
        names = [item.filename for item in files]
        if len(names) != len(set(names)):
            raise ValueError("Duplicate VSIX archive entry")
        for item in files:
            safe_name(item.filename)
            if stat.S_ISLNK(item.external_attr >> 16):
                raise ValueError("VSIX links are forbidden")
        generated = {"[Content_Types].xml", "extension.vsixmanifest"}
        payload = {name.removeprefix("extension/"): digest(zipped.read(name)) for name in names if name.startswith("extension/")}
        normalized = {("readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name): value for name, value in expected.items()}
        if payload != normalized or set(names) != {"extension/" + name for name in normalized} | generated:
            raise ValueError("VSIX payload differs from the allowlisted stage")
        for name in expected:
            if name in ("native/airk", "native/runtime/airk", "native/runtime/airk-embed", "native/runtime/AIRK.app/Contents/MacOS/airk", "native/runtime/AIRK.app/Contents/MacOS/airk-embed") and not (zipped.getinfo("extension/" + name).external_attr >> 16) & stat.S_IXUSR:
                raise ValueError("VSIX lost native executable permission: " + name)
        metadata = ET.fromstring(zipped.read("extension.vsixmanifest"))
        identity = next((element for element in metadata.iter()
                         if element.tag.rsplit("}", 1)[-1] == "Identity"), None)
        if identity is None or identity.get("Id") != "airk" or identity.get("Publisher") != "iokaio":
            raise ValueError("VSIX extension identity differs")
        if platform is not None and identity.get("TargetPlatform") != platform:
            raise ValueError("VSIX platform differs")
        if version is not None and identity.get("Version") != version:
            raise ValueError("VSIX version differs")
        if pre_release is not None:
            flag = next((element.get("Value") for element in metadata.iter()
                         if element.tag.rsplit("}", 1)[-1] == "Property"
                         and element.get("Id") == "Microsoft.VisualStudio.Code.PreRelease"), None)
            if (pre_release and flag != "true") or (not pre_release and flag not in (None, "false")):
                raise ValueError("VSIX pre-release flag differs")
        if display_name is not None:
            label = next((element.text for element in metadata.iter()
                          if element.tag.rsplit("}", 1)[-1] == "DisplayName"), None)
            if label != display_name:
                raise ValueError("VSIX display name differs")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--native-dir", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--channel", choices=("preview", "stable"), default="stable")
    parser.add_argument("--stage-only", action="store_true", help="Verify an allowlisted development stage without creating a VSIX")
    args = parser.parse_args()
    native = (args.native_dir or ROOT / "artifacts" / "native" / args.target / "publish").resolve()
    output = (args.output or ROOT / "artifacts" / "extension" / args.target).resolve()
    artifacts = (ROOT / "artifacts").resolve()
    if not output.is_relative_to(artifacts) or output == artifacts or not native.is_relative_to(artifacts):
        raise SystemExit("Native input and fresh extension output must be inside artifacts/")
    manifest = json.loads((native / "manifest.json").read_text(encoding="utf-8"))
    if manifest.get("schema") != "airk-native-candidate/2":
        raise SystemExit("A verified separate native service pair is required")
    package = json.loads((EXTENSION / "package.json").read_text(encoding="utf-8"))
    pre_release = args.channel == "preview"
    if manifest["target"] != args.target or manifest["vsix_target"] != TARGETS[args.target] or manifest["version"] != package["version"]:
        raise SystemExit("Native candidate and extension version/platform differ")
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    if manifest["commit"] != commit or manifest["cargo_lock_sha256"] != digest((ROOT / "Cargo.lock").read_bytes()):
        raise SystemExit("Native candidate and current source/lock identity differ")
    safe_name(manifest["archive"])
    archive = native / manifest["archive"]
    if digest(archive.read_bytes()) != manifest["archive_sha256"]:
        raise SystemExit("Native archive digest mismatch")
    output.mkdir(parents=True, exist_ok=False)
    stage = output / "stage"
    publish = output / "publish"
    stage.mkdir()
    publish.mkdir()
    specification = importlib.util.spec_from_file_location("airk_pair_builder", ROOT / "scripts/build-service-runtime.py")
    builder = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(builder)
    operating_system = "windows" if "windows" in args.target else "macos" if "apple" in args.target else "linux"
    executable, helper = builder.runtime_names(operating_system)
    mac_resources = set(builder.macos_bundle_files(manifest["version"])) | {"AIRK.app/Contents/_CodeSignature/CodeResources", "AIRK.app/Contents/CodeResources"} if operating_system == "macos" else set()
    with zipfile.ZipFile(archive) as zipped:
        if len(zipped.namelist()) != len(set(zipped.namelist())) or set(zipped.namelist()) != set(manifest["payload"]):
            raise SystemExit("Native archive inventory differs")
        for name, expected in manifest["payload"].items():
            safe_name(name)
            if name not in (executable, helper, "airk-build.json", "airk-runtime.json", "airk-install.json", "LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md") and name not in mac_resources and not name.startswith("notices/"):
                raise SystemExit("Unexpected native payload path: " + name)
            data = zipped.read(name)
            if digest(data) != expected:
                raise SystemExit("Native payload digest mismatch: " + name)
            if stat.S_ISLNK(zipped.getinfo(name).external_attr >> 16):
                raise SystemExit("Native archive links are forbidden")
            destination = stage / "native" / "runtime" / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
    runtime = stage / "native" / "runtime"
    install_digest = digest((runtime / "airk-install.json").read_bytes())
    install = json.loads((runtime / "airk-install.json").read_text(encoding="utf-8"))
    inventory = {name: value for name, value in manifest["payload"].items() if name != "airk-install.json"}
    if install.get("files") != inventory or install_digest != manifest.get("install_manifest_sha256"):
        raise SystemExit("Native pair installer inventory differs")
    for name in (executable, helper):
        if name not in inventory:
            raise SystemExit("Native pair executable missing: " + name)
        (runtime / name).chmod(0o755)
    wrapper = {"schema": 2, "version": manifest["version"], "vsix_target": manifest["vsix_target"],
               "runtime_dir": "runtime", "install_manifest_sha256": install_digest,
               "payload": {name: inventory[name] for name in (executable, helper)}}
    (stage / "native" / "manifest.json").write_text(json.dumps(wrapper, indent=2) + "\n", encoding="utf-8")
    shutil.copyfile(native / "manifest.json", stage / "native" / "candidate.json")
    stage_runtime_modules(EXTENSION / "out", stage / "out")
    stage_bundles(EXTENSION / "out", stage / "out", package["devDependencies"]["preact"])
    stage_jsonc_parser(EXTENSION / "node_modules" / "jsonc-parser", stage / "node_modules" / "jsonc-parser")
    for key in ("scripts", "devDependencies"):
        package.pop(key, None)
    (stage / "package.json").write_text(json.dumps(package, indent=2) + "\n", encoding="utf-8")
    listing = (EXTENSION / "README.md").read_text(encoding="utf-8")
    (stage / "images").mkdir()
    for name in ILLUSTRATIONS:
        source = ROOT / "docs" / "images" / name
        if source.is_symlink() or not source.is_file() or source.stat().st_size > 2_097_152:
            raise SystemExit("Invalid or oversized extension illustration: " + name)
        shutil.copyfile(source, stage / "images" / name)
        listing = listing.replace("https://raw.githubusercontent.com/iokaio/airk/main/docs/images/" + name,
                                  "images/" + name)
    for name in ICONS:
        source = EXTENSION / "images" / name
        if source.is_symlink() or not source.is_file() or source.stat().st_size > 16_384:
            raise SystemExit("Invalid or oversized extension icon: " + name)
        shutil.copyfile(source, stage / "images" / name)
    (stage / "README.md").write_text(listing, encoding="utf-8")
    for name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"):
        shutil.copyfile(ROOT / name, stage / name)
    expected = {file.relative_to(stage).as_posix(): digest(file.read_bytes()) for file in stage.rglob("*") if file.is_file()}
    if args.stage_only:
        (output / "stage-receipt.json").write_text(json.dumps({
            "schema": "airk-extension-stage/1", "commit": commit,
            "native_manifest_sha256": digest((native / "manifest.json").read_bytes()),
            "payload": expected, "qualification": "development stage; no VSIX created"
        }, indent=2) + "\n", encoding="utf-8")
        print("Verified extension stage: " + str(stage))
        return
    vsix = publish / ("airk-" + package["version"] + "-" + TARGETS[args.target] + ".vsix")
    subprocess.run(["node", str(EXTENSION / "scripts" / "package.cjs"), str(stage), TARGETS[args.target], str(vsix), args.channel], cwd=ROOT, check=True)
    verify_vsix(vsix, expected, executable, TARGETS[args.target], package["version"], pre_release, package["displayName"])
    receipt = {"schema": "airk-extension-candidate/1", "version": package["version"], "commit": commit,
               "extension_id": package["publisher"] + "." + package["name"],
               "display_name": package["displayName"], "pre_release": pre_release,
               "platform": TARGETS[args.target], "native_archive_sha256": manifest["archive_sha256"],
               "native_manifest_sha256": digest((native / "manifest.json").read_bytes()),
               "npm_lock_sha256": digest((EXTENSION / "package-lock.json").read_bytes()),
               "node": subprocess.check_output(["node", "--version"], text=True).strip(),
               "vsce": "4.0.1-3", "archive": vsix.name, "archive_sha256": digest(vsix.read_bytes()),
               "payload": expected, "qualification": "verified package; installed-editor lifecycle qualification separate"}
    receipt_path = publish / "manifest.json"
    receipt_path.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    (publish / "SHA256SUMS").write_text(digest(vsix.read_bytes()) + "  " + vsix.name + "\n" + digest(receipt_path.read_bytes()) + "  manifest.json\n", encoding="utf-8")
    print("Verified platform VSIX: " + str(vsix))


if __name__ == "__main__":
    main()
