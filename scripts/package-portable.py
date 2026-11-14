# SPDX-License-Identifier: Apache-2.0
"""Wrap an already verified canonical native candidate in a separate portable ZIP.
This does not rebuild, sign, publish or claim installed qualification.
"""
import argparse
import html
import importlib.util
import json
from pathlib import Path
import stat
import shutil
import zipfile

ROOT = Path(__file__).resolve().parent.parent

def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / filename)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result

native = module("portable_native", "package-native.py")
layouts = module("portable_layouts", "package-standalone-linux.py")
contract = module("portable_contract", "standalone-installer-contract.py")


def instructions(version, target):
    windows = target.endswith("windows-msvc")
    mac = target.endswith("apple-darwin")
    cli = "runtime/AIRK.app/Contents/MacOS/airkctl" if mac else "runtime/airkctl" + (".exe" if windows else "")
    prefix = "& " if windows else ""
    recovery = (f'\nTo resume a retained setup operation, use the matching verified CLI:\n'
                f'{prefix}"{cli}" service setup --resume "<absolute retained operation-request.json>" --portable\n'
                'Keep the original request unchanged. Do not add location overrides or use a PID as authority.\n') if tuple(map(int, version.split('.'))) >= (1, 9, 0) else ""
    return f"""AIRK {version} portable ({target})

Extract the complete ZIP into a private folder owned by your account. Keep every
file together. You do not need VS Code, a developer toolchain or an installer.
Use an absolute path for PROGRAM and quote paths containing spaces or Unicode.
PROGRAM is the extracted runtime folder. The inventory is beside that folder.

{prefix}"{cli}" service setup --portable --program "<absolute extracted folder>/runtime"
{prefix}"{cli}" service start
{prefix}"{cli}" service status
{prefix}"{cli}" --help

Setup verifies the runtime, signatures where supported, inventory and locations.
It does not add PATH, login startup or certificate trust. Run the CLI by its full
path. Optional --data, --state, --cache and --logs accept absolute private roots.
The bundled embedding model works offline; source documents remain read-only.
Use CLI help for creating an index, indexing documents and evidence search.

{prefix}"{cli}" service stop
{prefix}"{cli}" service uninstall

Uninstall keeps index data by default. Review CLI help before explicitly purging.
Do not manually remove a running service or delete recovery receipts. If setup
fails, retain its reported private operation and diagnostic paths. A stopped or
damaged existing installation needs recovery; it is not permission to replace it.{recovery}
Linux distributions must supply the documented system runtime libraries. OS
signature checks may require the platform trust store. Internet is not needed for
offline indexing once this complete verified package has been acquired.
"""


def inventory_signing(target):
    if target.endswith("windows-msvc"):
        signer=module("portable_signer","sign-native.py")
        identity=signer.SIGNING_IDENTITY["windows"]
        return {"platform":"windows","publisher":identity["publisher"],"identity_eku":identity["identity_eku"]}
    if target.endswith("apple-darwin"):
        return {"platform":"macos","team_id":module("portable_macos_signer","sign-native.py").SIGNING_IDENTITY["macos"]["team_id"]}
    return dict(layouts.LINUX_SIGNING)


def package(native_dir, output, channel="stable"):
    native_dir = Path(native_dir).resolve(strict=True)
    receipt = native.bounded_json(native_dir / "manifest.json", 4 << 20)
    layouts.split_receipts().validate_native_candidate(receipt)
    version, target = receipt["version"], receipt["target"]
    if target not in contract.TARGETS or not contract.portable_required(version, channel):
        raise ValueError("Unsupported portable candidate")
    signed = target.endswith(("windows-msvc", "apple-darwin"))
    if receipt["signed_distribution"] is not signed or (signed and not isinstance(receipt["signing"], dict)):
        raise ValueError("Portable platform requires the reviewed native signing candidate")
    archive = native_dir / receipt["archive"]
    if archive.name != f"airk-{version}-{target}.zip" or archive.parent != native_dir:
        raise ValueError("Noncanonical native archive")
    if set(p.name for p in native_dir.iterdir()) != {archive.name,"manifest.json","SHA256SUMS"}:
        raise ValueError("Unexpected native candidate directory contents")
    native.inventory(native_dir)
    if native.sha256(archive) != receipt["archive_sha256"]:
        raise ValueError("Native archive digest differs")
    expected_sums = f"{native.sha256(archive)}  {archive.name}\n{native.sha256(native_dir / 'manifest.json')}  manifest.json\n"
    if (native_dir / "SHA256SUMS").read_text(encoding="utf-8") != expected_sums:
        raise ValueError("Native candidate checksum roster differs")
    images = ("AIRK.app/Contents/MacOS/airk", "AIRK.app/Contents/MacOS/airkctl") if target.endswith("apple-darwin") else ("airk.exe","airkctl.exe") if target.endswith("windows-msvc") else ("airk","airkctl")
    for name in receipt["payload"]:
        layouts.safe_name(name)
    native.verify_archive(archive, receipt["payload"], images)
    output = Path(output).resolve()
    output.relative_to(ROOT / "artifacts")
    output.mkdir(parents=True, exist_ok=False)
    stage = output / "stage"
    runtime = stage / "runtime"
    runtime.mkdir(parents=True)
    with zipfile.ZipFile(archive) as zipped:
        for name in sorted(receipt["payload"]):
            info = zipped.getinfo(name)
            if stat.S_IFMT(info.external_attr >> 16) != stat.S_IFREG:
                raise ValueError("Native archive includes a non-regular file")
            destination = runtime / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            with zipped.open(info) as source, destination.open("xb") as dest:
                shutil.copyfileobj(source,dest,1 << 20)
            destination.chmod(0o755 if name in images else 0o644)
    files, actual_images = native.validate_runtime(runtime, version, target, receipt["source"])
    if files != receipt["payload"] or tuple(actual_images) != images:
        raise ValueError("Extracted canonical runtime differs")
    signing = inventory_signing(target)
    inventory = layouts.payload_inventory(runtime,version,receipt["admin_assets"],target,
                                         dict(zip(("airk","airkctl"),images)),signing)
    inventory["channel"] = channel
    text = instructions(version,target)
    (stage / "README.txt").write_text(text,encoding="utf-8",newline="\n")
    (stage / "manual.html").write_text('<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>AIRK portable manual</title><main><h1>AIRK portable</h1><pre style="white-space:pre-wrap">'+html.escape(text)+'</pre></main></html>\n',encoding="utf-8",newline="\n")
    (stage / "payload-inventory.json").write_text(json.dumps(inventory,indent=1)+"\n",encoding="utf-8",newline="\n")
    bound = native.inventory(stage)
    embedded = {"schema":"airk-portable/1","version":version,"target":target,"runtime_prefix":"runtime/",
                "native_manifest_sha256":native.sha256(native_dir / "manifest.json"),"files":bound}
    (stage / "portable-manifest.json").write_text(json.dumps(embedded,indent=2)+"\n",encoding="utf-8",newline="\n")
    payload = native.inventory(stage)
    publish = output / "publish"
    publish.mkdir()
    portable = publish / contract.portable_artifact(version,target)
    native.write_archive(stage,portable,payload,tuple("runtime/"+name for name in images))
    manifest = publish / contract.portable_manifest(version,target)
    document = {"schema":"airk-portable-candidate/1","version":version,"target":target,"archive":portable.name,
                "archive_sha256":native.sha256(portable),"payload":payload,"native_candidate_sha256":native.sha256(native_dir / "manifest.json"),
                "qualification":"candidate only; extracted native/platform journeys required"}
    manifest.write_text(json.dumps(document,indent=2)+"\n",encoding="utf-8",newline="\n")
    (publish / "SHA256SUMS").write_text(f"{native.sha256(portable)}  {portable.name}\n{native.sha256(manifest)}  {manifest.name}\n",encoding="utf-8",newline="\n")
    verify(publish,receipt,native.sha256(native_dir / "manifest.json"),channel=channel)
    return document


def verify(publish_dir, native_receipt, native_manifest_sha256, published=False, channel=None):
    """One portable format verifier, shared by release assembly and downloads."""
    import re
    publish_dir=Path(publish_dir).resolve(strict=True)
    version,target=native_receipt["version"],native_receipt["target"]
    archive=publish_dir / contract.portable_artifact(version,target)
    manifest=publish_dir / contract.portable_manifest(version,target)
    if not re.fullmatch(r"[a-f0-9]{64}",native_manifest_sha256):
        raise ValueError("Invalid native manifest binding")
    if not published:
        if {p.name for p in publish_dir.iterdir()} != {archive.name,manifest.name,"SHA256SUMS"}:
            raise ValueError("Unexpected portable candidate files")
        native.inventory(publish_dir)
        expected=f"{native.sha256(archive)}  {archive.name}\n{native.sha256(manifest)}  {manifest.name}\n"
        if (publish_dir / "SHA256SUMS").read_text(encoding="utf-8")!=expected:
            raise ValueError("Portable checksum roster differs")
    else:
        for path in (archive,manifest):
            facts=path.lstat()
            if not stat.S_ISREG(facts.st_mode) or facts.st_nlink!=1 or getattr(facts,"st_file_attributes",0)&0x400:
                raise ValueError("Linked or special portable publication")
    receipt=native.bounded_json(manifest,4<<20)
    fields={"schema","version","target","archive","archive_sha256","payload","native_candidate_sha256","qualification"}
    if (set(receipt)!=fields or receipt["schema"]!="airk-portable-candidate/1" or receipt["version"]!=version
            or receipt["target"]!=target or receipt["archive"]!=archive.name
            or receipt["native_candidate_sha256"]!=native_manifest_sha256 or receipt["archive_sha256"]!=native.sha256(archive)):
        raise ValueError("Portable candidate identity differs")
    payload=receipt["payload"]
    expected_names={"README.txt","manual.html","payload-inventory.json","portable-manifest.json"}|{"runtime/"+name for name in native_receipt["payload"]}
    if not isinstance(payload,dict) or set(payload)!=expected_names or len(payload)>native.MAX_PAYLOAD_FILES+4:
        raise ValueError("Portable complete roster differs")
    for name,digest in payload.items():
        layouts.safe_name(name)
        if not isinstance(digest,str) or not re.fullmatch(r"[a-f0-9]{64}",digest):
            raise ValueError("Invalid portable file digest")
    if {name.removeprefix("runtime/"):digest for name,digest in payload.items() if name.startswith("runtime/")}!=native_receipt["payload"]:
        raise ValueError("Portable canonical native runtime differs")
    images=("AIRK.app/Contents/MacOS/airk","AIRK.app/Contents/MacOS/airkctl") if target.endswith("apple-darwin") else ("airk.exe","airkctl.exe") if target.endswith("windows-msvc") else ("airk","airkctl")
    with zipfile.ZipFile(archive) as bounded:
        if len(bounded.infolist())!=len(expected_names) or sum(info.file_size for info in bounded.infolist())>native.MAX_PAYLOAD_BYTES+(16<<20):
            raise ValueError("Portable archive exceeds total payload budget")
    native.verify_archive(archive,payload,tuple("runtime/"+name for name in images))
    with zipfile.ZipFile(archive) as zipped:
        for info in zipped.infolist():
            if stat.S_IFMT(info.external_attr>>16)!=stat.S_IFREG:
                raise ValueError("Portable archive includes a non-regular entry")
            expected_mode=0o755 if info.filename in {"runtime/"+image for image in images} else 0o644
            if stat.S_IMODE(info.external_attr>>16)!=expected_mode:
                raise ValueError("Portable file mode differs")
        for name in ("README.txt","manual.html","payload-inventory.json","portable-manifest.json"):
            if zipped.getinfo(name).file_size>4<<20:
                raise ValueError("Portable metadata exceeds bound")
        embedded=json.loads(zipped.read("portable-manifest.json"))
        if embedded!={"schema":"airk-portable/1","version":version,"target":target,"runtime_prefix":"runtime/",
                     "native_manifest_sha256":native_manifest_sha256,"files":{name:digest for name,digest in payload.items() if name!="portable-manifest.json"}}:
            raise ValueError("Portable embedded manifest differs")
        text=instructions(version,target)
        expected_html='<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>AIRK portable manual</title><main><h1>AIRK portable</h1><pre style="white-space:pre-wrap">'+html.escape(text)+'</pre></main></html>\n'
        if zipped.read("README.txt")!=text.encode("utf-8") or zipped.read("manual.html")!=expected_html.encode("utf-8"):
            raise ValueError("Portable manual differs from reviewed instructions")
        inventory=json.loads(zipped.read("payload-inventory.json"))
        file_rows={name:{"sha256":digest,"bytes":zipped.getinfo("runtime/"+name).file_size} for name,digest in native_receipt["payload"].items()}
        expected_inventory={"schema":layouts.INVENTORY_SCHEMA,"version":version,"channel":channel or inventory.get("channel"),"target":target,
            "executables":dict(zip(("airk","airkctl"),images)),"files":file_rows,"admin_assets":native_receipt["admin_assets"],"signing":inventory_signing(target)}
        if inventory.get("channel") not in ("stable","preview") or channel not in (None,"stable","preview") or inventory!=expected_inventory:
            raise ValueError("Portable native inventory differs")
    return receipt,archive


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native-dir",type=Path,required=True)
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--channel",choices=("stable","preview"),default="stable")
    args=parser.parse_args()
    print(json.dumps(package(args.native_dir,args.output,args.channel),indent=2))

if __name__ == "__main__": main()
