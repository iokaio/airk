# SPDX-License-Identifier: Apache-2.0
"""Local Windows signing adapter (split decision U70; contracts §7 C10, §9 C7).

This machine has no code-signing certificate; it has Azure CLI access to the
same Artifact Signing account and certificate profile CI uses (U22). The
adapter therefore:

* provisions the pinned `Microsoft.ArtifactSigning.Client` package from NuGet
  into a per-user folder outside the repository and every AIRK root, checking
  the package digest, the dlib digest and the dlib's Microsoft Authenticode
  signature before every use;
* signs a fresh copy with `signtool /dlib` using the checked-in account,
  profile, digest and timestamp settings and only the Azure CLI credential;
* verifies the result with `sign-native.py`'s existing policy (Authenticode
  trust, publisher, identity EKU, timestamp) and writes an
  `airk-native-signing-local/1` receipt that `split_receipts.py` accepts.

Local receipts say `provenance: local` and never stand in for hosted release
provenance. Nothing here installs, publishes or changes trust.

    python scripts/local_signing.py provision
    python scripts/local_signing.py sign --input airk.exe --output signed/airk.exe \
        --target windows-x64 --commit <sha> --cargo-lock-sha256 <sha> --receipt r.json
"""
import argparse
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parent.parent
NUGET = "https://www.nuget.org/api/v2/package/{package}/{version}"
MAX_PACKAGE_BYTES = 64 << 20


def _load(name, file):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SIGN = _load("airk_sign_native", "sign-native.py")
RECEIPTS = _load("airk_split_receipts", "split_receipts.py")
IDENTITY = SIGN.SIGNING_IDENTITY
CLIENT = IDENTITY["windows"]["local_client"]
SIGNING = IDENTITY["windows"]["artifact_signing"]


def digest_bytes(data):
    return hashlib.sha256(data).hexdigest()


def client_root():
    """`%LOCALAPPDATA%\\airk-artifact-signing-client\\<version>`: per user, outside
    the repository and outside every AIRK root (`%LOCALAPPDATA%\\AIRK`, `Programs\\AIRK`)."""
    base = os.environ.get("LOCALAPPDATA")
    if not base:
        raise RuntimeError("LOCALAPPDATA is not set; local signing runs on Windows only")
    root = Path(base) / "airk-artifact-signing-client" / CLIENT["version"]
    if ROOT in root.parents or root == ROOT:
        raise RuntimeError("signing client must live outside the repository")
    return root


def _authenticode_subject(path, run=SIGN.run):
    """The file's Authenticode status and signer subject, read by PowerShell."""
    script = ("$s = Get-AuthenticodeSignature -LiteralPath $env:AIRK_SIGNED_FILE; "
              "[pscustomobject]@{status=[string]$s.Status; subject=[string]$s.SignerCertificate.Subject; "
              "thumbprint=[string]$s.SignerCertificate.Thumbprint} | ConvertTo-Json -Compress")
    previous = os.environ.get("AIRK_SIGNED_FILE")
    os.environ["AIRK_SIGNED_FILE"] = str(path)
    try:
        return json.loads(run(["pwsh", "-NoProfile", "-NonInteractive", "-Command", script]))
    finally:
        if previous is None:
            os.environ.pop("AIRK_SIGNED_FILE", None)
        else:
            os.environ["AIRK_SIGNED_FILE"] = previous


def check_client(dlib, run=SIGN.run):
    """The provisioned dlib is the pinned, Microsoft-signed build."""
    if not dlib.is_file() or SIGN.digest(dlib) != CLIENT["dlib_sha256"]:
        raise RuntimeError("Artifact Signing dlib is missing or differs from the pinned digest")
    observed = _authenticode_subject(dlib, run)
    if observed.get("status") != "Valid" or observed.get("subject") != CLIENT["dlib_publisher"]:
        raise RuntimeError("Artifact Signing dlib is not validly signed by the pinned publisher")
    return dlib


def extract(package_bytes, target):
    """Unpack the verified package into `target` without following any path out of it."""
    with zipfile.ZipFile(io.BytesIO(package_bytes)) as archive:
        for member in archive.infolist():
            name = Path(member.filename)
            if name.is_absolute() or ".." in name.parts or member.filename.startswith(("/", "\\")):
                raise RuntimeError("Artifact Signing package entry escapes its folder")
        staging = Path(tempfile.mkdtemp(prefix="airk-signing-client-", dir=target.parent))
        try:
            archive.extractall(staging)
            if target.exists():
                raise RuntimeError("signing client folder appeared during provisioning")
            staging.rename(target)
        except BaseException:
            shutil.rmtree(staging, ignore_errors=True)
            raise


def provision(download=urllib.request.urlopen, run=SIGN.run):
    """Return the checked dlib, downloading the pinned package once if needed."""
    root = client_root()
    dlib = root / CLIENT["dlib"]
    if root.exists():
        return check_client(dlib, run)
    root.parent.mkdir(parents=True, exist_ok=True)
    url = NUGET.format(package=CLIENT["package"], version=CLIENT["version"])
    with download(url) as response:
        data = response.read(MAX_PACKAGE_BYTES + 1)
    if len(data) > MAX_PACKAGE_BYTES:
        raise RuntimeError("Artifact Signing package exceeds its size bound")
    if digest_bytes(data) != CLIENT["nupkg_sha256"]:
        raise RuntimeError("Artifact Signing package digest differs from the pinned value")
    extract(data, root)
    return check_client(dlib, run)


def metadata():
    """The dlib's account file, exactly the checked-in CI settings."""
    return {"Endpoint": SIGNING["endpoint"],
            "CodeSigningAccountName": SIGNING["account"],
            "CertificateProfileName": SIGNING["certificate_profile"],
            "ExcludeCredentials": list(SIGNING["excluded_credentials"])}


def signtool_path():
    kits = Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")) / "Windows Kits/10/bin"
    candidates = sorted(kits.glob("10.*/x64/signtool.exe"), key=lambda path: path.parent.parent.name)
    if not candidates:
        raise RuntimeError("signtool.exe from the Windows SDK is required")
    return candidates[-1]


def sign(source, output, target, commit, cargo_lock_sha256, dlib=None, signtool=None,
         run=SIGN.run):
    """Sign a fresh copy of `source` at `output`; return its validated local receipt."""
    if platform.system() != "Windows":
        raise RuntimeError("The local Artifact Signing adapter runs on Windows")
    source, output = Path(source), Path(output)
    if output.exists():
        raise RuntimeError("signed output already exists; signing never overwrites")
    dlib = dlib or provision(run=run)
    signtool = Path(signtool or signtool_path())
    output.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, output)
    with tempfile.TemporaryDirectory(prefix="airk-signing-") as folder:
        meta = Path(folder) / "metadata.json"
        meta_bytes = json.dumps(metadata(), indent=1).encode("utf-8")
        meta.write_bytes(meta_bytes)
        try:
            run([signtool, "sign", "/v", "/fd", SIGNING["file_digest"], "/tr", SIGNING["timestamp_url"],
                 "/td", SIGNING["timestamp_digest"], "/dlib", dlib, "/dmdf", meta, output], timeout=600)
        except BaseException:
            output.unlink(missing_ok=True)
            raise
    observed = _authenticode_subject(output, run)
    thumbprint = observed.get("thumbprint", "")
    receipt = {"platform": "Windows", "binary_sha256": SIGN.digest(output), "identity": thumbprint,
               "azure": SIGN.AZURE_SIGNING}
    verification = SIGN.verify(output, receipt, str(signtool))
    document = {
        "schema": RECEIPTS.NATIVE_SIGNING_LOCAL, "provenance": "local", "platform": "Windows",
        "target": target, "identity": thumbprint, "team_id": None,
        "source_sha256": SIGN.digest(source), "binary_sha256": receipt["binary_sha256"],
        "commit": commit, "cargo_lock_sha256": cargo_lock_sha256, "notarization": None,
        "verification": verification, "azure": SIGN.AZURE_SIGNING,
        "local_signing": {"mode": "artifact-signing-dlib",
                          "client": {"package": CLIENT["package"], "version": CLIENT["version"],
                                     "dlib_sha256": CLIENT["dlib_sha256"]},
                          "signtool_sha256": SIGN.digest(signtool),
                          "metadata_sha256": digest_bytes(meta_bytes),
                          "credential": SIGNING["credential"]},
    }
    RECEIPTS.validate_native_signing_local(document)
    return document


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("provision", help="download and check the pinned signing client")
    signing = commands.add_parser("sign", help="sign a fresh copy and write its receipt")
    signing.add_argument("--input", type=Path, required=True)
    signing.add_argument("--output", type=Path, required=True)
    signing.add_argument("--target", required=True)
    signing.add_argument("--commit", required=True)
    signing.add_argument("--cargo-lock-sha256", required=True)
    signing.add_argument("--receipt", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.command == "provision":
        print(provision())
        return 0
    document = sign(args.input, args.output, args.target, args.commit, args.cargo_lock_sha256)
    args.receipt.write_text(json.dumps(document, indent=1) + "\n", encoding="utf-8")
    print(json.dumps({"signed": str(args.output), "signer": document["identity"]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
