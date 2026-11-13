# SPDX-License-Identifier: Apache-2.0
"""Stage, package and verify the universal, UI-only AIRK VSIX (split plan §8.5, §10.5.1, U29).

The VSIX carries the compiled extension host, its webview bundles, the pinned
jsonc-parser closure, media and legal files. It contains no native runtime and
no executable content of any kind; the service is installed separately and the
extension reaches it only through airkctl. Pre-split platform VSIXs remain
verifiable through the explicit `platform=` path of verify_vsix (release tooling
for versions before split_receipts.SPLIT_VERSION).
"""
import argparse
from html.parser import HTMLParser
import hashlib
import importlib.util
import json
from pathlib import Path
import posixpath
import re
import shutil
import stat
import subprocess
import zipfile
from urllib.parse import urlsplit
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parent.parent
EXTENSION = ROOT / "packages" / "vscode"
_SPEC = importlib.util.spec_from_file_location("airk_split_receipts", ROOT / "scripts" / "split_receipts.py")
receipts = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(receipts)
# Rust triple -> VS Code platform of the pre-split platform VSIXs. Only historical
# verification and the esbuild host roster read it; split packaging is universal.
TARGETS = {"x86_64-pc-windows-msvc": "win32-x64", "x86_64-unknown-linux-gnu": "linux-x64",
           "aarch64-apple-darwin": "darwin-arm64", "x86_64-apple-darwin": "darwin-x64"}
UNIVERSAL = "universal"
LOCK = "packages/vscode/package-lock.json"
# The reviewed runtime closure of the extension host (paths below out/), from its
# entry points: extension.js (main), extensionUpdateLoader.js and uninstallHook.js.
# A new host module must be added here; stage_runtime_modules rejects any import
# that resolves outside this list.
MODULES = ("extension.js", "extensionUpdateLoader.js", "uninstallHook.js", "serviceExtension.js",
           "serviceDashboard.js", "serviceClient.js", "serviceHttp.js", "serviceConsoleView.js",
           "nativeGate.js", "nativeMetadata.js", "nativeOutcome.js", "readRetry.js", "model.js",
           "privateDirectory.js", "mcpStdioSession.js", "sourceVerification.js", "publications.js",
           "agentConfig.js", "agentConfigHealth.js", "agentMigration.js", "agentReconnect.js", "agentToml.js",
           "unifiedAgentConfig.js",
           "airkctl/index.js", "airkctl/install.js", "airkctl/runner.js", "airkctl/session.js",
           "airkctl/signingIdentity.js", "airkctl/states.js", "airkctl/tools.js", "airkctl/verify.js",
           "airkctl/version.js", "airkctl/agents/index.js", "airkctl/agents/jsonc.js",
           "airkctl/agents/recognizer.js", "airkctl/agents/toml.js", "airkctl/features/adminOpener.js",
           "airkctl/features/explorerHost.js", "airkctl/features/liveStatus.js",
           "airkctl/features/windowAssistance.js",
           "airkctl/features/windowScope.js",
           "airkctl/status/guidance.js",
           "explorerSidebar.js", "explorerStatus.js", "explorerController.js", "explorerControl.js",
           "explorerContracts.js", "explorerErrors.js", "explorerEvidence.js", "explorerFeatures.js",
           "explorerPanel.js", "explorerRoutes.js", "explorerSearch.js", "explorerSource.js",
           "explorerWorkspaceIO.js", "explorerFacts.js", "explorerInspect.js", "explorerManagement.js",
           "explorerCommands.js", "explorerAssistanceHost.js", "explorerAssistanceLaunch.js",
           "explorerStrategies.js", "explorerStrategyRunner.js", "explorerDrafts.js", "explorerSuggestions.js",
           "explorerReview.js", "explorerPlanFiles.js", "explorerHandoffs.js",
           "indexingCatalog.js", "indexingSplit.js", "referenceRefresh.js", "indexingCatalogController.js",
           "catalogSession.js", "scopeSelection.js", "indexLibrary.js",
           "workbenchView.js", "workbenchContracts.js", "workbenchRender.js", "sharedCapabilityFields.js",
           "workbenchSessions.js", "workbenchStrategies.js", "workbenchReferenceRefresh.js")
# esbuild outputs; referenced by URI or path from host modules, never require()d.
BUNDLES = ("explorerWebview.js", "explorerWebview.css", "explorerAssistance.js", "workbenchWebview.js", "workbenchWebview.css")
# Host-only implementations that must never ship a second bundled copy.
HOST_MARKERS = ("class ServiceClient", "class NativeControlTransport", "class NativeSession", "class IntentJournal", "class PrivateIntentStore", "function probeMcp",
                "class ServiceConsoleController", "class RuntimeHost")
# The update loader's inventory name rule (extensionUpdateLoader.ts); every sealed name must match it.
LOADER_NAME = re.compile(r"(?:package\.json|images/airk\.svg|out/(?:airkctl/(?:(?:agents|features|status)/)?)?"
                         r"[A-Za-z0-9_-]+\.(?:js|css)|node_modules/jsonc-parser/(?:package\.json|lib/umd/(?:main|impl/[A-Za-z-]+)\.js))")
# Executable image signatures: PE, ELF, Mach-O (thin, both byte orders) and fat/universal.
NATIVE_MAGIC = (b"MZ", b"\x7fELF", b"\xfe\xed\xfa\xce", b"\xfe\xed\xfa\xcf", b"\xce\xfa\xed\xfe",
                b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca")
JSONC_FILES = ("package.json", "LICENSE.md", "lib/umd/main.js", "lib/umd/impl/edit.js",
               "lib/umd/impl/format.js", "lib/umd/impl/parser.js", "lib/umd/impl/scanner.js",
               "lib/umd/impl/string-intern.js")
ILLUSTRATIONS = ("local-evidence.png", "index-to-context.png")
PRODUCT_ILLUSTRATIONS = {"workbench-1.8.7-results.png": "workbench", "explorer-1.7.1.png": "explorer"}
ICONS = ("airk.svg", "icon.png")


def source_tree(revision="HEAD"):
    """Git tree identity of a checked-out revision; None fails source comparisons closed."""
    try:
        tree = subprocess.check_output(["git", "rev-parse", f"{revision}^{{tree}}"], cwd=ROOT, text=True,
                                       stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError):
        return None
    return tree if re.fullmatch(r"[0-9a-f]{40}", tree) else None


def same_source(receipt, commit, tree):
    """A PR merge-ref candidate is the same source only when its recorded tree is identical."""
    return receipt.get("commit") == commit or (
        tree is not None and re.fullmatch(r"[0-9a-f]{40}", str(receipt.get("tree", ""))) is not None
        and receipt["tree"] == tree)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def runtime_package(package):
    """Retain the reviewed uninstall hook; development scripts never ship."""
    package = dict(package)
    package.pop("devDependencies", None)
    if package.get("scripts", {}).get("vscode:uninstall") != "node ./out/uninstallHook.js":
        raise ValueError("Expected the packaged AIRK uninstall hook")
    package["scripts"] = {"vscode:uninstall": "node ./out/uninstallHook.js"}
    return package


def update_inventory_paths(native=False):
    """The complete graph consumed by the isolated extension loader.

    The universal VSIX seals no native entry. `native=True` is the pre-split
    platform VSIX graph, which also sealed its native wrapper manifest.
    """
    paths = ["package.json", "images/airk.svg"] + (["native/manifest.json"] if native else [])
    paths += ["out/" + name for name in MODULES + BUNDLES]
    paths += ["node_modules/jsonc-parser/" + name for name in JSONC_FILES
              if name.endswith((".js", ".json"))]
    return sorted(paths)


def encode_update_inventory(files, version, package_manifest):
    reviewed = json.loads(package_manifest)
    if not isinstance(reviewed, dict) or "__metadata" in reviewed or len(package_manifest.encode("utf-8")) > 131_072:
        raise ValueError("Invalid reviewed extension package manifest")
    encoded = json.dumps({
        "schema": "airk-extension-update/1", "version": version, "files": files,
        "package_manifest": package_manifest
    }, indent=2) + "\n"
    inventory_bytes = encoded.encode("utf-8")
    if len(inventory_bytes) > 262_144:
        raise ValueError("Extension update inventory exceeds its reader budget")
    return inventory_bytes


def write_update_inventory(stage, version):
    """Seal the exact staged host/UI graph for isolated in-process AIRK handoff."""
    files = {name: digest((stage / name).read_bytes()) for name in update_inventory_paths()}
    package_manifest = (stage / "package.json").read_bytes().decode("utf-8")
    (stage / "airk-update.json").write_bytes(encode_update_inventory(files, version, package_manifest))


def read_update_inventory(zipped, version=None, platform=None):
    """Verify the loader's complete graph before accepting or promoting a VSIX.

    Historical VSIXs predate the loader. Current versions, or archives containing
    its entry module, must have an inventory. Archive bytes are checked exactly;
    VS Code's installed __metadata rewrite is separately allowed by the loader.
    `platform=None` is the universal VSIX: no native entry may be sealed. A
    platform is the pre-split VSIX, whose native wrapper identity is checked.
    """
    names = set(zipped.namelist())
    inventory_name = "extension/airk-update.json"
    if inventory_name not in names:
        modern = isinstance(version, str) and re.fullmatch(r"\d+\.\d+\.\d+", version) and tuple(map(int, version.split("."))) >= (1, 8, 12)
        if modern or "extension/out/extensionUpdateLoader.js" in names:
            raise ValueError("VSIX is missing its extension update inventory")
        return None
    if zipped.getinfo(inventory_name).file_size > 262_144:
        raise ValueError("Extension update inventory exceeds its reader budget")
    inventory = json.loads(zipped.read(inventory_name).decode("utf-8"))
    if (not isinstance(inventory, dict) or set(inventory) != {"schema", "version", "files", "package_manifest"}
            or inventory.get("schema") != "airk-extension-update/1"
            or not isinstance(inventory.get("version"), str)
            or not isinstance(inventory.get("files"), dict)
            or set(inventory["files"]) != set(update_inventory_paths(native=platform is not None))
            or not isinstance(inventory.get("package_manifest"), str)):
        raise ValueError("Invalid or incomplete extension update inventory")
    snapshot = inventory["package_manifest"].encode("utf-8")
    if len(snapshot) > 131_072 or digest(snapshot) != inventory["files"]["package.json"]:
        raise ValueError("Reviewed extension manifest does not match its update inventory")
    manifest = json.loads(snapshot)
    if (not isinstance(manifest, dict) or "__metadata" in manifest
            or manifest.get("name") != "airk" or manifest.get("publisher") != "iokaio"
            or manifest.get("version") != inventory["version"]
            or (version is not None and inventory["version"] != version)):
        raise ValueError("Extension update inventory manifest identity differs")
    total = 0
    for name, expected in inventory["files"].items():
        if not isinstance(expected, str) or not re.fullmatch(r"[0-9a-f]{64}", expected):
            raise ValueError("Invalid extension update inventory digest")
        if platform is None and not LOADER_NAME.fullmatch(name):
            raise ValueError("Extension update inventory name is outside the loader's rule: " + name)
        entry = "extension/" + name
        if entry not in names or zipped.getinfo(entry).file_size > 8_388_608:
            raise ValueError("Missing or oversized extension update asset: " + name)
        total += zipped.getinfo(entry).file_size
        if total > 33_554_432 or digest(zipped.read(entry)) != expected:
            raise ValueError("Extension update inventory differs from archive asset: " + name)
    if platform is None:
        return inventory
    native = json.loads(zipped.read("extension/native/manifest.json"))
    if not isinstance(native, dict) or native.get("version") != inventory["version"] or native.get("vsix_target") != platform:
        raise ValueError("Extension update inventory native identity differs")
    return inventory


def safe_name(name):
    parts = name.split("/")
    if not name or "\\" in name or ":" in name or any(p in ("", ".", "..") for p in parts):
        raise ValueError("Unsafe archive path: " + name)


def resolve_import(name, request):
    """The module a relative require() in out/<name> loads, by the update loader's rule.

    extensionUpdateLoader.ts tries `x`, `x.js` and `x.json` only; a directory
    import (index resolution) or a path leaving out/ is not loadable there.
    """
    if not request.startswith(("./", "../")):
        return None
    relative = posixpath.normpath(posixpath.join(posixpath.dirname(name), request))
    if relative.startswith("../") or relative == "..":
        return None
    return next((value for value in (relative, relative + ".js", relative + ".json") if value in MODULES), None)


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
            if resolve_import(name, match.group(2)) is None:
                raise ValueError("Runtime import outside allowlisted stage: " + name + " -> " + match.group(2))
        modules[name] = data
    for name, data in modules.items():
        target = destination / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)


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
        if name in ("explorerWebview.js", "workbenchWebview.js"):
            if re.search(r"\brequire\s*\(", text):
                raise ValueError("Webview bundle must not require modules")
            if not text.startswith("/*! AIRK Explorer webview. Includes Preact " + preact_version + " (MIT).\n"):
                raise ValueError("Webview bundle lacks the pinned Preact notice")
        if name == "explorerAssistance.js":
            if not text.startswith("/*! AIRK Explorer Assistance MCP server "):
                raise ValueError("Assistance bundle lacks its identifying banner")
            loads = re.findall(r'''\b(?:require|import)\s*\(\s*(["'`]?)([^"'`)]*)''', text)
            if any(not quote or not module.startswith("node:") for quote, module in loads):
                raise ValueError("Assistance bundle may load Node built-ins only")
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


def verify_listing_images(listing):
    """The gallery serves README separately; bundled relative images cannot render."""
    sources = [a or b for a, b in re.findall(r'!\[[^\]\n]*\]\(\s*(?:<([^>\n]+)>|([^\s)]+))', listing)]

    class Images(HTMLParser):
        def handle_starttag(self, tag, attrs):
            if tag == "img":
                sources.append(dict(attrs).get("src", ""))

    Images().feed(listing)
    for source in sources:
        parsed = urlsplit(source)
        if parsed.scheme != "https" or not parsed.netloc:
            raise ValueError("Marketplace README images require absolute HTTPS URLs: " + source)


def native_content(name, data, mode):
    """Why `name` is forbidden in the universal VSIX, or None (split plan §8.5, U29)."""
    lowered = name.lower()
    if lowered.startswith(("native/", "extension/native/")):
        return "native subtree"
    if lowered.endswith(receipts.NATIVE_SUFFIXES):
        return "native library or executable suffix"
    if mode & (stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH):
        return "executable permission"
    if data.startswith(NATIVE_MAGIC):
        return "executable image header"
    return None


def check_stage_content(stage):
    """Refuse native or executable content in the stage before VSCE runs."""
    for file in sorted(stage.rglob("*")):
        if file.is_symlink():
            raise ValueError("Linked stage entry: " + file.relative_to(stage).as_posix())
        if not file.is_file():
            continue
        name = file.relative_to(stage).as_posix()
        with file.open("rb") as stream:
            head = stream.read(8)
        # Staged files are freshly written (no mode carried over); the archive check covers modes.
        reason = native_content(name, head, 0)
        if reason:
            raise ValueError(f"Universal VSIX stage contains forbidden content ({reason}): {name}")


def verify_vsix(archive, expected, *, version=None, pre_release=None, display_name=None, platform=None):
    """Verify a packaged VSIX against its allowlisted stage digests.

    Without `platform` this is the universal, UI-only VSIX: TargetPlatform must be
    absent and no native or executable entry may exist. `platform` verifies a
    historical (pre-split) platform VSIX only.
    """
    if platform is not None and version is not None and receipts.split_version(version):
        raise ValueError("Split versions ship one universal VSIX; a platform VSIX is not accepted")
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
        if platform is None:
            for item in files:
                with zipped.open(item) as stream:
                    head = stream.read(8)
                reason = native_content(item.filename.removeprefix("extension/"), head, item.external_attr >> 16)
                if reason:
                    raise ValueError(f"Universal VSIX contains forbidden content ({reason}): {item.filename}")
        generated = {"[Content_Types].xml", "extension.vsixmanifest"}
        payload = {name.removeprefix("extension/"): digest(zipped.read(name)) for name in names if name.startswith("extension/")}
        normalized = {("readme.md" if name == "README.md" else "LICENSE.txt" if name == "LICENSE" else name): value for name, value in expected.items()}
        if payload != normalized or set(names) != {"extension/" + name for name in normalized} | generated:
            raise ValueError("VSIX payload differs from the allowlisted stage")
        read_update_inventory(zipped, version, platform)
        if platform is not None:
            for name in expected:
                if name in ("native/airk", "native/runtime/airk", "native/runtime/airk-embed", "native/runtime/AIRK.app/Contents/MacOS/airk", "native/runtime/AIRK.app/Contents/MacOS/airk-embed", "native/runtime/AIRK.app/Contents/MacOS/airk-launcher-image") and not (zipped.getinfo("extension/" + name).external_attr >> 16) & stat.S_IXUSR:
                    raise ValueError("VSIX lost native executable permission: " + name)
        if "extension/readme.md" in names:
            verify_listing_images(zipped.read("extension/readme.md").decode("utf-8"))
        metadata = ET.fromstring(zipped.read("extension.vsixmanifest"))
        identity = next((element for element in metadata.iter()
                         if element.tag.rsplit("}", 1)[-1] == "Identity"), None)
        if identity is None or identity.get("Id") != "airk" or identity.get("Publisher") != "iokaio":
            raise ValueError("VSIX extension identity differs")
        if identity.get("TargetPlatform") != platform:
            raise ValueError("VSIX platform differs" if platform else "Universal VSIX must not declare a TargetPlatform")
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


def extension_requirements(stage):
    """The I2 contracts and required capabilities the staged airkctl/version.js declares.

    Reads the exact compiled module that ships, so the receipt cannot drift from it.
    """
    module = (stage / "out" / "airkctl" / "version.js").resolve()
    script = ("const r=require(process.argv[1]).EXTENSION_REQUIREMENTS;"
              "process.stdout.write(JSON.stringify({supported:r.supported,required:r.required}))")
    value = json.loads(subprocess.check_output(["node", "-e", script, str(module)], text=True, encoding="utf-8", timeout=60))
    supported, required = value.get("supported"), value.get("required")
    if (not isinstance(supported, list) or not supported or not all(type(item) is int for item in supported)
            or not isinstance(required, list) or not all(isinstance(item, str) for item in required)):
        raise ValueError("Staged airkctl/version.js has no valid EXTENSION_REQUIREMENTS")
    return sorted(set(supported)), sorted(set(required))


def universal_receipt(*, package, commit, tree, pre_release, stage, vsix, expected, node_version, requirements):
    """The C7 universal VSIX receipt (airk-extension-candidate/2), validated before it is written."""
    contracts, capabilities = requirements
    receipt = {"schema": receipts.VSIX, "version": package["version"], "commit": commit, "tree": tree,
               "extension_id": package["publisher"] + "." + package["name"],
               "display_name": package["displayName"], "pre_release": pre_release, "target": UNIVERSAL,
               "i2_contracts": contracts, "required_capabilities": capabilities,
               "ui_inventory_sha256": digest((stage / "airk-update.json").read_bytes()),
               "locks": {LOCK: digest((ROOT / LOCK).read_bytes())},
               "node": node_version, "vsce": package["devDependencies"]["@vscode/vsce"],
               "archive": vsix.name, "archive_sha256": digest(vsix.read_bytes()), "payload": expected,
               "qualification": "verified universal package; installed-editor qualification separate"}
    receipts.validate_vsix(receipt)
    return receipt


def stage_extension(stage, package):
    """Build the complete allowlisted UI-only stage from compiled packages/vscode output."""
    stage_runtime_modules(EXTENSION / "out", stage / "out")
    stage_bundles(EXTENSION / "out", stage / "out", package["devDependencies"]["preact"])
    stage_jsonc_parser(EXTENSION / "node_modules" / "jsonc-parser", stage / "node_modules" / "jsonc-parser")
    (stage / "package.json").write_text(json.dumps(runtime_package(package), indent=2) + "\n", encoding="utf-8")
    listing = (EXTENSION / "README.md").read_text(encoding="utf-8")
    (stage / "images").mkdir()
    for name in ILLUSTRATIONS:
        source = ROOT / "docs" / "images" / name
        if source.is_symlink() or not source.is_file() or source.stat().st_size > 2_097_152:
            raise ValueError("Invalid or oversized extension illustration: " + name)
        shutil.copyfile(source, stage / "images" / name)
    for name, component in PRODUCT_ILLUSTRATIONS.items():
        source = ROOT / "docs" / component / "images" / name
        if source.is_symlink() or not source.is_file() or source.stat().st_size > 2_097_152:
            raise ValueError("Invalid or oversized product illustration: " + name)
        shutil.copyfile(source, stage / "images" / name)
    for name in ICONS:
        source = EXTENSION / "images" / name
        if source.is_symlink() or not source.is_file() or source.stat().st_size > 16_384:
            raise ValueError("Invalid or oversized extension icon: " + name)
        shutil.copyfile(source, stage / "images" / name)
    verify_listing_images(listing)
    (stage / "README.md").write_text(listing, encoding="utf-8")
    for name in ("LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"):
        shutil.copyfile(ROOT / name, stage / name)
    write_update_inventory(stage, package["version"])
    check_stage_content(stage)
    return {file.relative_to(stage).as_posix(): digest(file.read_bytes()) for file in stage.rglob("*") if file.is_file()}


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--output", type=Path, help="Fresh directory under artifacts/ (default artifacts/extension/universal)")
    parser.add_argument("--channel", choices=("preview", "stable"), default="stable")
    parser.add_argument("--stage-only", action="store_true", help="Verify an allowlisted development stage without creating a VSIX")
    args = parser.parse_args()
    output = (args.output or ROOT / "artifacts" / "extension" / UNIVERSAL).resolve()
    artifacts = (ROOT / "artifacts").resolve()
    if not output.is_relative_to(artifacts) or output == artifacts:
        raise SystemExit("Fresh extension output must be inside artifacts/")
    package = json.loads((EXTENSION / "package.json").read_text(encoding="utf-8"))
    if not receipts.split_version(package["version"]):
        raise SystemExit(f"Version {package['version']} predates the split ({'.'.join(map(str, receipts.SPLIT_VERSION))}); "
                         "a universal VSIX would reuse a published platform-VSIX version. Increment the version first.")
    pre_release = args.channel == "preview"
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    tree = source_tree()
    if tree is None:
        raise SystemExit("Cannot determine the source tree identity")
    output.mkdir(parents=True, exist_ok=False)
    stage = output / "stage"
    publish = output / "publish"
    stage.mkdir()
    publish.mkdir()
    expected = stage_extension(stage, package)
    requirements = extension_requirements(stage)
    if args.stage_only:
        (output / "stage-receipt.json").write_text(json.dumps({
            "schema": "airk-extension-stage/2", "version": package["version"], "commit": commit, "tree": tree,
            "target": UNIVERSAL, "i2_contracts": requirements[0], "required_capabilities": requirements[1],
            "payload": expected, "qualification": "development stage; no VSIX created"
        }, indent=2) + "\n", encoding="utf-8")
        print("Verified extension stage: " + str(stage))
        return
    vsix = publish / ("airk-" + package["version"] + ".vsix")
    subprocess.run(["node", str(EXTENSION / "scripts" / "package.cjs"), str(stage), str(vsix), args.channel], cwd=ROOT, check=True)
    verify_vsix(vsix, expected, version=package["version"], pre_release=pre_release, display_name=package["displayName"])
    node_version = subprocess.check_output(["node", "--version"], text=True).strip()
    receipt = universal_receipt(package=package, commit=commit, tree=tree, pre_release=pre_release, stage=stage,
                                vsix=vsix, expected=expected, node_version=node_version, requirements=requirements)
    receipt_path = publish / "manifest.json"
    receipt_path.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    (publish / "SHA256SUMS").write_text(digest(vsix.read_bytes()) + "  " + vsix.name + "\n" + digest(receipt_path.read_bytes()) + "  manifest.json\n", encoding="utf-8")
    print(f"Verified universal VSIX {package['version']} ({args.channel}), commit {commit}: {vsix}")
    print(f"Receipt: {receipt_path}")


if __name__ == "__main__":
    main()
