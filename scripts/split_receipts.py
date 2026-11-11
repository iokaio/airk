# SPDX-License-Identifier: Apache-2.0
"""Split artifact receipts and release catalog (split contract step C7).

The single owner of the receipt schemas that the split service pair, admin
bundle, universal VSIX, release assembly and local signing adapter will write
(docs/split/contracts.md §9; docs/split/inventory/build.md A.1-A.7, B.3-B.4).
Each validator takes one decoded JSON value, returns the schema identity it
accepted and otherwise raises ReceiptError with a stable code naming the first
refusal. Validators read no files and run nothing.

No consumer uses this module yet; consumers switch in Phases 5 and 7. Pre-split
decoders in the existing scripts stay unchanged and version-gated, so no
pre-split receipt qualifies a split payload and no split receipt enters a
pre-split path.
"""
import importlib.util
import json
from pathlib import Path
import re

# Installer names and kinds are kept from the pre-split catalog (plan §9.4); reuse its owner.
_SPEC = importlib.util.spec_from_file_location(
    "airk_standalone_installer_contract", Path(__file__).with_name("standalone-installer-contract.py"))
standalone = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(standalone)

PAIR_METADATA = 3
BUILD_RECEIPT = 2
ADMIN_ASSETS = "airk-admin-assets/1"
NATIVE_CANDIDATE = "airk-native-candidate/3"
CI_RUNTIME = "airk-ci-runtime/2"
VSIX = "airk-extension-candidate/2"
RELEASE_CATALOG = "airk-release-catalog/1"
NATIVE_SIGNING_LOCAL = "airk-native-signing-local/1"
PAIR_SIGNING_LOCAL = "airk-service-pair-signing-local/1"
INSTALLER_LOCAL = "airk-standalone-installer-local/1"
HOSTED_PAIR_SIGNING = "airk-service-pair-signing/1"
HOSTED_NATIVE_SIGNING = "airk-native-signing/1"

TARGETS = tuple(standalone.TARGETS)
IMAGES = {"windows": {"airk": "airk.exe", "airkctl": "airkctl.exe"},
          "linux": {"airk": "airk", "airkctl": "airkctl"},
          "macos": {"airk": "AIRK.app/Contents/MacOS/airk", "airkctl": "AIRK.app/Contents/MacOS/airkctl"}}
PAIR_FILES = ("airk-runtime.json", "airk-build.json", "airk-install.json")
BUILD_RECEIPT_BYTES = 16_384
INVENTORY_LIMIT = 4096
ADMIN_FILE_LIMIT = 1024
ADMIN_FILE_BYTES = 32 << 20
CONTENT_TYPES = {"html": "text/html; charset=utf-8", "css": "text/css; charset=utf-8",
                 "js": "text/javascript; charset=utf-8", "json": "application/json",
                 "svg": "image/svg+xml", "png": "image/png", "woff2": "font/woff2",
                 "txt": "text/plain; charset=utf-8"}
NATIVE_VSIX_KEYS = ("platform", "native_archive_sha256", "native_manifest_sha256", "native")
NATIVE_SUFFIXES = (".exe", ".dll", ".dylib", ".so", ".node")
# Hosted producer identity never appears in a local receipt, at any depth.
HOSTED_FIELDS = frozenset({"run_id", "attempt", "run_attempt", "build_attempt", "workflow", "workflow_id",
                           "workflow_ref", "workflow_run", "repository", "event", "producer", "promotion",
                           "runner", "runner_environment"})
CODES = frozenset({"not_object", "schema_mismatch", "unknown_field", "missing_field", "digest_invalid",
                   "value_invalid", "inventory_mismatch", "native_entry", "receipt_oversize",
                   "hosted_provenance", "path_absolute", "component_mixed"})

_SHA256 = re.compile(r"[0-9a-f]{64}")
_GIT = re.compile(r"[0-9a-f]{40}")
_NUMBER = r"(?:0|[1-9][0-9]*)"
_VERSION = rf"{_NUMBER}\.{_NUMBER}\.{_NUMBER}"
_POSITIVE = r"[1-9][0-9]{0,18}"
_THUMBPRINT = r"[0-9A-Fa-f]{40}"
_CAPABILITY = re.compile(r"[a-z][a-z0-9_.-]{0,63}")
_DRIVE = re.compile(r"(?:^|[^A-Za-z])[A-Za-z]:[\\/]")
_HOME = re.compile(r"/(?:Users|home)/")


class ReceiptError(ValueError):
    """A refusal: a stable code from CODES and the JSON location of the first defect."""

    def __init__(self, code, where):
        super().__init__(f"{code}: {where or '/'}")
        self.code = code
        self.where = where or "/"


def platform_of(target):
    """(os, arch) of a native release target, in the runtime metadata vocabulary."""
    return ("windows" if "windows" in target else "macos" if "apple" in target else "linux"), target.split("-", 1)[0]


def _join(where, key):
    return f"{where}/{key}" if where else str(key)


def _object(value, where):
    if not isinstance(value, dict):
        raise ReceiptError("not_object", where)
    return value


def _schema(document, expected, where=""):
    value = _object(document, where).get("schema")
    if type(value) is not type(expected) or value != expected:
        raise ReceiptError("schema_mismatch", _join(where, "schema"))


def _fields(node, where, required, optional=()):
    _object(node, where)
    unknown = sorted(set(node) - set(required) - set(optional))
    if unknown:
        raise ReceiptError("unknown_field", _join(where, unknown[0]))
    missing = [key for key in required if key not in node]
    if missing:
        raise ReceiptError("missing_field", _join(where, missing[0]))
    return node


def _sha(node, key, where, nullable=False):
    value = node[key]
    if value is None and nullable:
        return None
    if not isinstance(value, str) or not _SHA256.fullmatch(value):
        raise ReceiptError("digest_invalid", _join(where, key))
    return value


def _git(node, key, where):
    if not isinstance(node[key], str) or not _GIT.fullmatch(node[key]):
        raise ReceiptError("digest_invalid", _join(where, key))
    return node[key]


def _text(node, key, where, pattern=None, choices=None, limit=4096, multiline=False):
    value = node[key]
    if (not isinstance(value, str) or not value or len(value) > limit
            or any(ord(c) < 32 and not (multiline and c in "\r\n\t") for c in value)
            or pattern is not None and not re.fullmatch(pattern, value)
            or choices is not None and value not in choices):
        raise ReceiptError("value_invalid", _join(where, key))
    return value


def _equal(node, key, where, expected):
    value = node[key]
    if type(value) is not type(expected) or value != expected:
        raise ReceiptError("value_invalid", _join(where, key))
    return value


def _integer(node, key, where, minimum, maximum):
    value = node[key]
    if type(value) is not int or not minimum <= value <= maximum:
        raise ReceiptError("value_invalid", _join(where, key))
    return value


def _boolean(node, key, where):
    if type(node[key]) is not bool:
        raise ReceiptError("value_invalid", _join(where, key))
    return node[key]


def _relative(name, where):
    if (not isinstance(name, str) or not name or len(name) > 1024 or "\\" in name or ":" in name
            or any(ord(c) < 32 for c in name) or any(part in ("", ".", "..") for part in name.split("/"))):
        raise ReceiptError("value_invalid", where)
    return name


def _inventory(node, key, where, limit=INVENTORY_LIMIT):
    value, location = node[key], _join(where, key)
    if not isinstance(value, dict) or not 1 <= len(value) <= limit:
        raise ReceiptError("value_invalid", location)
    for name in value:
        _relative(name, _join(location, name))
        _sha(value, name, location)
    return value


def _source(node, key, where):
    location = _join(where, key)
    source = _fields(node[key], location, ("sha256", "files"))
    _sha(source, "sha256", location)
    _integer(source, "files", location, 1, 1_000_000)


def _ascending(node, key, where, item, allow_empty):
    """A canonical list: strictly ascending (so unique), each item valid."""
    value = node[key]
    if (not isinstance(value, list) or not value and not allow_empty or not all(item(entry) for entry in value)
            or any(value[i] >= value[i + 1] for i in range(len(value) - 1))):
        raise ReceiptError("value_invalid", _join(where, key))
    return value


def _path_like(text):
    return (text.startswith(("/", "~")) or "\\" in text
            or _DRIVE.search(text) is not None or _HOME.search(text) is not None)


def _local(document, where=""):
    """Local receipts carry provenance "local", no hosted identity and no paths (build.md B.3)."""
    stack = [(where, document)]
    while stack:
        location, value = stack.pop()
        if isinstance(value, dict):
            for key, item in value.items():
                child = _join(location, key)
                if key in HOSTED_FIELDS or key.lower().startswith("github"):
                    raise ReceiptError("hosted_provenance", child)
                if _path_like(key):
                    raise ReceiptError("path_absolute", child)
                stack.append((child, item))
        elif isinstance(value, list):
            stack.extend((_join(location, index), item) for index, item in enumerate(value))
        elif isinstance(value, str) and _path_like(value):
            raise ReceiptError("path_absolute", location)
    if "provenance" in document and document["provenance"] != "local":
        raise ReceiptError("hosted_provenance", _join(where, "provenance"))


def _admin_binding(node, where):
    _fields(node, where, ("inventory_sha256", "bundle_sha256"))
    _sha(node, "inventory_sha256", where)
    _sha(node, "bundle_sha256", where)


def _pair_payload(document, payload, os_name):
    """Both images are present with the receipt's digests, beside the pair metadata files."""
    for role, name in IMAGES[os_name].items():
        if payload.get(name) != document[role + "_sha256"]:
            raise ReceiptError("inventory_mismatch", _join("payload", name))
    for name in PAIR_FILES:
        if name not in payload:
            raise ReceiptError("inventory_mismatch", _join("payload", name))


def _distinct_images(document, where=""):
    if document["airk_sha256"] == document["airkctl_sha256"]:
        raise ReceiptError("inventory_mismatch", _join(where, "airkctl_sha256"))


def validate_pair_metadata(document, where=""):
    """`airk-runtime.json` schema 3: schema-2 keys plus airkctl, interfaces and admin inventory; exact key set."""
    _schema(document, PAIR_METADATA, where)
    _fields(document, where, ("schema", "version", "os", "arch", "airk_sha256", "airkctl_sha256",
                              "i1_protocol", "i2_contract", "admin_inventory_sha256"))
    _text(document, "version", where, pattern=_VERSION)
    os_name = _text(document, "os", where, choices=IMAGES)
    arch = _text(document, "arch", where, choices=("x86_64", "aarch64"))
    if (os_name, arch) not in {platform_of(target) for target in TARGETS}:
        raise ReceiptError("value_invalid", _join(where, "arch"))
    for key in ("airk_sha256", "airkctl_sha256", "admin_inventory_sha256"):
        _sha(document, key, where)
    _integer(document, "i1_protocol", where, 1, 65_535)
    _integer(document, "i2_contract", where, 1, 65_535)
    _distinct_images(document, where)
    return PAIR_METADATA


def _imports(document, key, flags):
    report = _fields(document[key], key, ("schema", "command", "stdout", "external_onnxruntime_imports", *flags))
    _equal(report, "schema", key, 1)
    command = report["command"]
    if (not isinstance(command, list) or not 1 <= len(command) <= 16
            or any(not isinstance(part, str) or not part or len(part) > 4096 for part in command)):
        raise ReceiptError("value_invalid", _join(key, "command"))
    _text(report, "stdout", key, limit=8192, multiline=True)
    for flag in ("external_onnxruntime_imports", *flags):
        _equal(report, flag, key, False)


def validate_build_receipt(document):
    """`airk-build.json` schema 2: schema 1 plus the admin bundle block and an airkctl import report."""
    _schema(document, BUILD_RECEIPT)
    if len((json.dumps(document, indent=2) + "\n").encode("utf-8")) > BUILD_RECEIPT_BYTES:
        raise ReceiptError("receipt_oversize", "")
    _fields(document, "", ("schema", "source", "profile", "runtime", "rustc", "static_ort", "static_imports",
                           "airkctl_static_imports", "embedded_model_smoke", "admin_assets"))
    _source(document, "source", "")
    _text(document, "profile", "", choices=("debug", "release"))
    validate_pair_metadata(document["runtime"], "runtime")
    _text(document, "rustc", "", multiline=True)
    optional = ("static_link_supplement_sha256", "static_link_recovery_sha256")
    ort = _fields(document["static_ort"], "static_ort", ("build_receipt_sha256", "target", "static_link_closure_sha256",
                                                         *optional, "verified_inputs_sha256"))
    for key in ort:
        if key == "target":
            _text(ort, key, "static_ort", choices=TARGETS)
        else:
            _sha(ort, key, "static_ort", nullable=key in optional)
    _imports(document, "static_imports", ())
    _imports(document, "airkctl_static_imports", ("inference_symbols",))
    smoke = _object(document["embedded_model_smoke"], "embedded_model_smoke")
    if "schema" not in smoke:
        raise ReceiptError("missing_field", "embedded_model_smoke/schema")
    _equal(smoke, "schema", "embedded_model_smoke", 1)
    admin = _fields(document["admin_assets"], "admin_assets", ("bundle_sha256", "inventory_sha256", "node", "npm", "locks"))
    _sha(admin, "bundle_sha256", "admin_assets")
    _sha(admin, "inventory_sha256", "admin_assets")
    _text(admin, "node", "admin_assets", pattern="v" + _VERSION)
    _text(admin, "npm", "admin_assets", pattern=_VERSION)
    _inventory(admin, "locks", "admin_assets", limit=16)
    runtime = document["runtime"]
    if admin["inventory_sha256"] != runtime["admin_inventory_sha256"]:
        raise ReceiptError("inventory_mismatch", "admin_assets/inventory_sha256")
    if platform_of(ort["target"]) != (runtime["os"], runtime["arch"]):
        raise ReceiptError("inventory_mismatch", "static_ort/target")
    return BUILD_RECEIPT


def validate_admin_assets(document):
    """`admin-assets.json`: every served file with digest, size and content type; bundle digest; toolchain."""
    _schema(document, ADMIN_ASSETS)
    _fields(document, "", ("schema", "files", "bundle_sha256", "toolchain"))
    files = document["files"]
    if not isinstance(files, list) or not 1 <= len(files) <= ADMIN_FILE_LIMIT:
        raise ReceiptError("value_invalid", "files")
    seen = set()
    for index, row in enumerate(files):
        where = f"files/{index}"
        _fields(row, where, ("name", "sha256", "bytes", "content_type"))
        name = _relative(row["name"], where + "/name")
        if name in seen:
            raise ReceiptError("value_invalid", where + "/name")
        seen.add(name)
        _sha(row, "sha256", where)
        _integer(row, "bytes", where, 0, ADMIN_FILE_BYTES)
        leaf = name.rsplit("/", 1)[-1]
        expected = CONTENT_TYPES.get(leaf.rsplit(".", 1)[-1]) if "." in leaf else None
        if expected is None or row["content_type"] != expected:
            raise ReceiptError("value_invalid", where + "/content_type")
    _sha(document, "bundle_sha256", "")
    toolchain = _fields(document["toolchain"], "toolchain", ("node", "npm"))
    _text(toolchain, "node", "toolchain", pattern="v" + _VERSION)
    _text(toolchain, "npm", "toolchain", pattern=_VERSION)
    return ADMIN_ASSETS


def validate_native_candidate(document):
    """`airk-native-candidate/3`: both image digests, admin inventory, interfaces; no `vsix_target` (U29)."""
    _schema(document, NATIVE_CANDIDATE)
    _fields(document, "", ("schema", "version", "target", "commit", "tree", "dirty_checkout", "source",
                           "cargo_lock_sha256", "toolchain_file_sha256", "rustc", "cargo", "archive",
                           "archive_sha256", "install_manifest_sha256", "payload", "airk_sha256", "airkctl_sha256",
                           "admin_assets", "i1_protocol", "i2_contract", "bundled_model", "embedded_models",
                           "embedded_model_smoke", "signed_distribution", "signing", "qualification"))
    version = _text(document, "version", "", pattern=_VERSION)
    target = _text(document, "target", "", choices=TARGETS)
    commit = _git(document, "commit", "")
    _git(document, "tree", "")
    _boolean(document, "dirty_checkout", "")
    _source(document, "source", "")
    for key in ("cargo_lock_sha256", "toolchain_file_sha256", "archive_sha256", "install_manifest_sha256",
                "airk_sha256", "airkctl_sha256"):
        _sha(document, key, "")
    _text(document, "rustc", "", multiline=True)
    _text(document, "cargo", "")
    _text(document, "qualification", "")
    if document["archive"] != f"airk-{version}-{target}.zip":
        raise ReceiptError("value_invalid", "archive")
    payload = _inventory(document, "payload", "")
    _admin_binding(document["admin_assets"], "admin_assets")
    _integer(document, "i1_protocol", "", 1, 65_535)
    _integer(document, "i2_contract", "", 1, 65_535)
    for key in ("bundled_model", "embedded_models", "embedded_model_smoke"):
        _object(document[key], key)
    if document["embedded_model_smoke"].get("schema") != 1:
        raise ReceiptError("value_invalid", "embedded_model_smoke/schema")
    _distinct_images(document)
    os_name = platform_of(target)[0]
    _pair_payload(document, payload, os_name)
    if payload["airk-install.json"] != document["install_manifest_sha256"]:
        raise ReceiptError("inventory_mismatch", "install_manifest_sha256")
    signed, signing = _boolean(document, "signed_distribution", ""), document["signing"]
    if not signed:
        if signing is not None:
            raise ReceiptError("value_invalid", "signing")
        return NATIVE_CANDIDATE
    if os_name == "linux" or not isinstance(signing, dict):
        raise ReceiptError("value_invalid", "signed_distribution" if os_name == "linux" else "signing")
    if signing.get("schema") == PAIR_SIGNING_LOCAL:
        validate_service_pair_signing_local(signing, "signing")
    elif signing.get("schema") != HOSTED_PAIR_SIGNING:
        raise ReceiptError("schema_mismatch", "signing/schema")
    for key, expected in (("target", target), ("commit", commit), ("cargo_lock_sha256", document["cargo_lock_sha256"]),
                          ("install_manifest_sha256", document["install_manifest_sha256"])):
        if signing.get(key) != expected:
            raise ReceiptError("inventory_mismatch", _join("signing", key))
    return NATIVE_CANDIDATE


def validate_ci_runtime(document):
    """`airk-ci-runtime/2`: binds both executables, the admin digests and the source identity."""
    _schema(document, CI_RUNTIME)
    _fields(document, "", ("schema", "commit", "tree", "run_id", "target", "qualification", "build_attempt",
                           "source", "payload", "archive_sha256", "airk_sha256", "airkctl_sha256", "admin_assets"))
    _git(document, "commit", "")
    _git(document, "tree", "")
    _text(document, "run_id", "", pattern=_POSITIVE)
    target = _text(document, "target", "", choices=TARGETS)
    _boolean(document, "qualification", "")
    _integer(document, "build_attempt", "", 1, 1000)
    _source(document, "source", "")
    payload = _inventory(document, "payload", "")
    for key in ("archive_sha256", "airk_sha256", "airkctl_sha256"):
        _sha(document, key, "")
    _admin_binding(document["admin_assets"], "admin_assets")
    _distinct_images(document)
    _pair_payload(document, payload, platform_of(target)[0])
    return CI_RUNTIME


def validate_vsix(document):
    """Universal VSIX receipt (U29): UI/host inventory and I2 contracts, no native entry of any kind."""
    _schema(document, VSIX)
    for key in NATIVE_VSIX_KEYS:
        if key in document:
            raise ReceiptError("native_entry", key)
    if isinstance(document.get("payload"), dict):
        for name in document["payload"]:
            if name.startswith(("native/", "extension/native/")) or name.lower().endswith(NATIVE_SUFFIXES):
                raise ReceiptError("native_entry", _join("payload", name))
    _fields(document, "", ("schema", "version", "commit", "tree", "extension_id", "display_name", "pre_release",
                           "target", "i2_contracts", "required_capabilities", "ui_inventory_sha256", "locks", "node",
                           "vsce", "archive", "archive_sha256", "payload", "qualification"))
    version = _text(document, "version", "", pattern=_VERSION)
    _git(document, "commit", "")
    _git(document, "tree", "")
    _text(document, "extension_id", "", pattern=r"[a-z0-9][a-z0-9-]*\.[a-z0-9][a-z0-9-]*")
    _text(document, "display_name", "")
    _boolean(document, "pre_release", "")
    _equal(document, "target", "", "universal")
    _ascending(document, "i2_contracts", "", lambda v: type(v) is int and 1 <= v <= 65_535, allow_empty=False)
    _ascending(document, "required_capabilities", "", lambda v: isinstance(v, str) and _CAPABILITY.fullmatch(v),
               allow_empty=True)
    _sha(document, "ui_inventory_sha256", "")
    _sha(document, "archive_sha256", "")
    _inventory(document, "locks", "", limit=16)
    _text(document, "node", "", pattern="v" + _VERSION)
    _text(document, "vsce", "")
    _text(document, "qualification", "")
    if document["archive"] != f"airk-{version}.vsix":
        raise ReceiptError("value_invalid", "archive")
    _inventory(document, "payload", "")
    return VSIX


def catalog_names(version, channel, commit, producer, release):
    """The complete producer, release-input and asset name sets for one release."""
    produced = f"{commit}-{producer['run_id']}-{producer['attempt']}"
    released = f"{commit}-{release['run_id']}"
    artifacts = {f"airk-native-{target}-{produced}" for target in TARGETS} | {f"airk-vsix-universal-{produced}"}
    inputs = {f"airk-vsix-universal-{released}"}
    assets = {f"airk-{version}.vsix", f"airk-{version}.vsix-manifest.json"}
    for target in TARGETS:
        prefix = "airk" if platform_of(target)[0] == "linux" else "airk-signed"
        inputs |= {f"{prefix}-native-{target}-{released}", f"airk-source-{target}-{released}"}
        assets |= {f"airk-{version}-{target}.zip", f"airk-{version}-{target}.native-manifest.json"}
        if standalone.required(version, channel):
            assets |= {standalone.artifact(version, target), standalone.manifest(version, target)}
    return artifacts, inputs, assets


def _names(document, key, expected):
    value = document[key]
    if not isinstance(value, list) or not all(isinstance(name, str) for name in value) or len(set(value)) != len(value):
        raise ReceiptError("value_invalid", key)
    if set(value) != expected:
        raise ReceiptError("inventory_mismatch", key)


def validate_release_catalog(document):
    """Four native targets, one universal VSIX and (when required) one installer per target."""
    _schema(document, RELEASE_CATALOG)
    _fields(document, "", ("schema", "package_version", "channel", "commit", "tree", "producer", "release",
                           "native_targets", "producer_artifacts", "release_inputs", "assets"))
    version = _text(document, "package_version", "", pattern=_VERSION)
    channel = _text(document, "channel", "", choices=("stable", "preview"))
    commit = _git(document, "commit", "")
    _git(document, "tree", "")
    for key in ("producer", "release"):
        _fields(document[key], key, ("run_id", "attempt"))
        _text(document[key], "run_id", key, pattern=_POSITIVE)
        _text(document[key], "attempt", key, pattern=_POSITIVE)
    _names(document, "native_targets", set(TARGETS))
    artifacts, inputs, assets = catalog_names(version, channel, commit, document["producer"], document["release"])
    _names(document, "producer_artifacts", artifacts)
    _names(document, "release_inputs", inputs)
    if not isinstance(document["assets"], dict) or not document["assets"]:
        raise ReceiptError("value_invalid", "assets")
    for name in document["assets"]:
        _sha(document["assets"], name, "assets")
    if set(document["assets"]) != assets:
        raise ReceiptError("inventory_mismatch", "assets")
    return RELEASE_CATALOG


def _windows_signature(node, where):
    """Hosted Windows signature fields (Azure policy, Authenticode verification) plus the local adapter block."""
    identity = _text(node, "identity", where, pattern=_THUMBPRINT)
    _equal(node, "team_id", where, None)
    location = _join(where, "azure")
    azure = _fields(node["azure"], location, ("endpoint", "account", "profile", "publisher", "identity_eku"))
    for key in azure:
        _text(azure, key, location)
    location = _join(where, "verification")
    verification = _fields(node["verification"], location, ("trust", "timestamp_present", "signer_thumbprint"))
    _equal(verification, "trust", location, "authenticode_valid")
    _equal(verification, "timestamp_present", location, True)
    if verification["signer_thumbprint"] != identity:
        raise ReceiptError("inventory_mismatch", _join(location, "signer_thumbprint"))
    location = _join(where, "local_signing")
    local = _fields(node["local_signing"], location, ("mode", "client", "signtool_sha256", "metadata_sha256", "credential"))
    _equal(local, "mode", location, "artifact-signing-dlib")
    client = _fields(local["client"], _join(location, "client"), ("package", "version", "dlib_sha256"))
    _text(client, "package", _join(location, "client"))
    _text(client, "version", _join(location, "client"), pattern=_VERSION)
    _sha(client, "dlib_sha256", _join(location, "client"))
    _sha(local, "signtool_sha256", location)
    _sha(local, "metadata_sha256", location)
    _equal(local, "credential", location, "AzureCliCredential")


def validate_native_signing_local(document, where=""):
    """`airk-native-signing-local/1`: one component signed outside the hosted workflow."""
    _schema(document, NATIVE_SIGNING_LOCAL, where)
    _local(document, where)
    _fields(document, where, ("schema", "provenance", "platform", "target", "identity", "team_id", "source_sha256",
                              "binary_sha256", "commit", "cargo_lock_sha256", "notarization", "verification",
                              "local_signing"), ("azure",))
    platform = _text(document, "platform", where, choices=("Windows", "Darwin"))
    target = _text(document, "target", where, choices=TARGETS)
    if platform_of(target)[0] != {"Windows": "windows", "Darwin": "macos"}[platform]:
        raise ReceiptError("value_invalid", _join(where, "target"))
    source, binary = _sha(document, "source_sha256", where), _sha(document, "binary_sha256", where)
    _git(document, "commit", where)
    _sha(document, "cargo_lock_sha256", where)
    if source == binary:
        raise ReceiptError("value_invalid", _join(where, "binary_sha256"))
    if platform == "Windows":
        if "azure" not in document:
            raise ReceiptError("missing_field", _join(where, "azure"))
        _windows_signature(document, where)
        _equal(document, "notarization", where, None)
        return NATIVE_SIGNING_LOCAL
    if "azure" in document:
        raise ReceiptError("unknown_field", _join(where, "azure"))
    _text(document, "identity", where, pattern=r"Developer ID Application: [^\r\n]+|" + _THUMBPRINT)
    team = _text(document, "team_id", where, pattern=r"[A-Z0-9]{10}")
    location = _join(where, "verification")
    verification = _fields(document["verification"], location, ("trust", "team_id", "hardened_runtime", "timestamp_present"))
    _equal(verification, "trust", location, "apple_developer_id")
    _equal(verification, "hardened_runtime", location, True)
    _equal(verification, "timestamp_present", location, True)
    if verification["team_id"] != team:
        raise ReceiptError("inventory_mismatch", _join(location, "team_id"))
    notarization, location = document["notarization"], _join(where, "notarization")
    if notarization is not None:
        _fields(notarization, location, ("id", "status", "stapled"), ("scope",))
        _text(notarization, "id", location)
        _equal(notarization, "status", location, "Accepted")
        _boolean(notarization, "stapled", location)
        if "scope" in notarization:
            _text(notarization, "scope", location)
    location = _join(where, "local_signing")
    _fields(document["local_signing"], location, ("mode",))
    _equal(document["local_signing"], "mode", location, "developer-id-keychain")
    return NATIVE_SIGNING_LOCAL


def validate_service_pair_signing_local(document, where=""):
    """`airk-service-pair-signing-local/1`: both images signed locally; never mixed with hosted components."""
    _schema(document, PAIR_SIGNING_LOCAL, where)
    _local(document, where)
    _fields(document, where, ("schema", "provenance", "target", "commit", "cargo_lock_sha256",
                              "source_install_sha256", "install_manifest_sha256", "components"))
    target = _text(document, "target", where, choices=TARGETS)
    os_name = platform_of(target)[0]
    if os_name == "linux":
        raise ReceiptError("value_invalid", _join(where, "target"))
    commit = _git(document, "commit", where)
    lock = _sha(document, "cargo_lock_sha256", where)
    _sha(document, "source_install_sha256", where)
    _sha(document, "install_manifest_sha256", where)
    location = _join(where, "components")
    components = _object(document["components"], location)
    if set(components) != set(IMAGES[os_name].values()):
        raise ReceiptError("inventory_mismatch", location)
    for name in sorted(components):
        component, child = components[name], _join(location, name)
        if isinstance(component, dict) and component.get("schema") == HOSTED_NATIVE_SIGNING:
            raise ReceiptError("component_mixed", _join(child, "schema"))
        validate_native_signing_local(component, child)
        for key, expected in (("target", target), ("commit", commit), ("cargo_lock_sha256", lock)):
            if component[key] != expected:
                raise ReceiptError("inventory_mismatch", _join(child, key))
    return PAIR_SIGNING_LOCAL


def validate_standalone_installer_local(document):
    """`airk-standalone-installer-local/1`: the Windows installer and uninstaller signed by the local adapter."""
    _schema(document, INSTALLER_LOCAL)
    _local(document)
    _fields(document, "", ("schema", "provenance", "version", "target", "commit", "cargo_lock_sha256",
                           "runtime_install_sha256", "runtime_source", "runtime_signing_sha256", "artifact",
                           "artifact_sha256", "installer_kind", "signing", "uninstaller", "compiler",
                           "installer_source_sha256", "payload"))
    version = _text(document, "version", "", pattern=_VERSION)
    target = _text(document, "target", "", choices=TARGETS)
    if platform_of(target)[0] != "windows":
        raise ReceiptError("value_invalid", "target")
    _git(document, "commit", "")
    for key in ("cargo_lock_sha256", "runtime_install_sha256", "runtime_signing_sha256", "artifact_sha256",
                "installer_source_sha256"):
        _sha(document, key, "")
    _source(document, "runtime_source", "")
    if document["artifact"] != standalone.artifact(version, target):
        raise ReceiptError("value_invalid", "artifact")
    if document["installer_kind"] != standalone.TARGETS[target][2]:
        raise ReceiptError("value_invalid", "installer_kind")
    signed_file = ("platform", "identity", "team_id", "azure", "binary_sha256", "verification", "local_signing")
    for key, extra in (("signing", ()), ("uninstaller", ("unsigned_sha256",))):
        node = _fields(document[key], key, (*signed_file, *extra))
        _equal(node, "platform", key, "Windows")
        _sha(node, "binary_sha256", key)
        _windows_signature(node, key)
    uninstaller = document["uninstaller"]
    if _sha(uninstaller, "unsigned_sha256", "uninstaller") == uninstaller["binary_sha256"]:
        raise ReceiptError("value_invalid", "uninstaller/binary_sha256")
    if document["signing"]["binary_sha256"] != document["artifact_sha256"]:
        raise ReceiptError("inventory_mismatch", "signing/binary_sha256")
    compiler = _fields(document["compiler"], "compiler", ("version", "sha256"), ("files_sha256", "lock_sha256"))
    _text(compiler, "version", "compiler")
    for key in compiler:
        if key != "version":
            _sha(compiler, key, "compiler")
    _inventory(document, "payload", "")
    return INSTALLER_LOCAL


VALIDATORS = {
    "pair_metadata_3": validate_pair_metadata,
    "build_receipt_2": validate_build_receipt,
    "admin_assets_1": validate_admin_assets,
    "native_candidate_3": validate_native_candidate,
    "ci_runtime_2": validate_ci_runtime,
    "vsix_universal": validate_vsix,
    "release_catalog": validate_release_catalog,
    "native_signing_local": validate_native_signing_local,
    "service_pair_signing_local": validate_service_pair_signing_local,
    "standalone_installer_local": validate_standalone_installer_local,
}
