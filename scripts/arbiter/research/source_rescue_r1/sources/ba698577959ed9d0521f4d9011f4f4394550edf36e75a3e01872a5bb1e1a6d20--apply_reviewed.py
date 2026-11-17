# SPDX-License-Identifier: Apache-2.0
# Private source application only. No build, model, runtime or approval action.
import argparse, base64, ctypes, hashlib, json, msvcrt, os, subprocess, sys
from pathlib import Path
from ctypes import wintypes

PWSH = r"C:\Program Files\PowerShell\7\pwsh.exe"
SID = "S-1-5-21-3678370870-3858642417-826968105-1002"
REPO = Path(r"D:\code\Github\airk")
LIMIT = 2 * 1024 * 1024

def require(value, message):
    if not value:
        raise RuntimeError(message)

def sha(data):
    return hashlib.sha256(data).hexdigest()

def powershell(script, data=""):
    encoded = base64.b64encode(script.encode("utf-16-le")).decode("ascii")
    result = subprocess.run([PWSH, "-NoProfile", "-NonInteractive", "-EncodedCommand", encoded],
                            input=data, text=True, capture_output=True, timeout=30)
    require(result.returncode == 0, "same-user metadata query failed: " + result.stderr[:2000])
    return json.loads(result.stdout)

def identity():
    require(os.name == "nt", "reviewed application requires this Windows host")
    observed = powershell(r"""$ErrorActionPreference='Stop'; $i=[Security.Principal.WindowsIdentity]::GetCurrent(); $session=[Diagnostics.Process]::GetCurrentProcess().SessionId; $humans=@(Get-CimInstance Win32_Process -Filter "name='explorer.exe'" | Where-Object SessionId -eq $session | ForEach-Object {(Invoke-CimMethod -InputObject $_ -MethodName GetOwnerSid).Sid}); [pscustomobject]@{account=$i.Name;sid=$i.User.Value;session=$session;human_sids=$humans}|ConvertTo-Json -Compress""")
    require(observed["sid"] == SID and observed["account"].lower() == r"tyai\tdotn", "effective identity differs from reviewed human")
    require(observed["human_sids"] and set(observed["human_sids"]) == {SID}, "interactive human identity is unknown/different")
    return observed

def safe(path):
    path = Path(os.path.abspath(path))
    require(path.is_relative_to(REPO), "path escapes authorized workspace")
    current = REPO
    require(not REPO.is_symlink(), "workspace alias refused")
    for part in path.relative_to(REPO).parts:
        require(part not in ("", ".", ".."), "invalid relative component")
        current = current / part
        if os.path.lexists(current):
            stat = current.lstat()
            require(not (getattr(stat, "st_file_attributes", 0) & 0x400), "reparse component refused: " + str(current))
    return path

def body(path):
    with open(safe(path), "rb") as file:
        result = file.read(LIMIT + 1)
    require(len(result) <= LIMIT, "bounded source exceeded two MiB")
    return result

def file_id(stat):
    return {"device": str(stat.st_dev), "inode": str(stat.st_ino), "nlink": stat.st_nlink}

def security(paths):
    script = r"""$ErrorActionPreference='Stop'; $paths=[Console]::In.ReadToEnd()|ConvertFrom-Json; $rows=@(foreach($path in $paths){$acl=Get-Acl -LiteralPath $path;[pscustomobject]@{path=$path;owner_sid=$acl.GetOwner([Security.Principal.SecurityIdentifier]).Value;sddl=$acl.Sddl}}); ConvertTo-Json -InputObject $rows -Depth 4 -Compress"""
    rows = powershell(script, json.dumps([str(path) for path in paths]))
    return {str(Path(row["path"])): row for row in rows}

def verify_pin(pin):
    data = body(pin["path"])
    require(sha(data) == pin["sha256"] and len(data) == pin["bytes"], "reviewed input drift: " + pin["path"])
    return data

def snapshot(manifest):
    private = set()
    for package in manifest["input_packets"]:
        verify_pin(package["packet"])
        private.add(safe(package["packet"]["path"]))
    targets = []
    payloads = {}
    for row in manifest["files"]:
        relative = Path(row["path"])
        require(not relative.is_absolute() and relative.parts[0] in ("crates", "packages", "fixtures"), "unexpected target scope")
        target = safe(REPO / relative)
        require(row["kind"] == ("contract" if target.name == "CONTRACT.md" else "consumer"), "contract order label mismatch")
        payloads[row["path"]] = verify_pin(row["payload"])
        private.add(safe(row["payload"]["path"]))
        for origin in row["origins"]:
            verify_pin(origin["input"])
            private.add(safe(origin["input"]["path"]))
        if row["existing_identity"] is None:
            require(not os.path.lexists(target), "new target already exists: " + row["path"])
        else:
            require(target.is_file() and sha(body(target)) == row["preimage_sha256"], "target preimage drift: " + row["path"])
            require(file_id(target.stat()) == {key: row["existing_identity"][key] for key in ("device", "inode", "nlink")}, "target identity drift")
            targets.append(target)
    observed = security(targets + sorted(private))
    for path in private:
        require(observed[str(path)]["owner_sid"] == SID and path.stat().st_nlink == 1, "private input is not independent human-owned source")
    for row in manifest["files"]:
        known = row["existing_identity"]
        if known is not None:
            actual = observed[str(REPO / row["path"])]
            require(known["owner_sid"] == actual["owner_sid"] and known["sddl"] == actual["sddl"], "owner/SDDL drift: " + row["path"])
            require(known["nlink"] == 1, "existing target is not independent")
    return payloads

def write_target(path, data, row):
    # Deny concurrent write/delete while comparing and mutating the exact handle.
    # Existing files are never replaced, renamed, chmodded or re-owned.
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    create = api.CreateFileW
    create.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p,
                       wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
    create.restype = wintypes.HANDLE
    api.CloseHandle.argtypes = [wintypes.HANDLE]
    api.CloseHandle.restype = wintypes.BOOL
    existing = row["existing_identity"] is not None
    handle = create("\\\\?\\" + str(path), 0xC0000000, 1, None, 3 if existing else 1,
                    0x00200000 | 0x80000000, None)
    require(handle != ctypes.c_void_p(-1).value, "cannot exclusively open reviewed target: " + str(ctypes.get_last_error()))
    try:
        fd = msvcrt.open_osfhandle(handle, os.O_RDWR | os.O_BINARY)
    except BaseException:
        api.CloseHandle(handle)
        raise
    with os.fdopen(fd, "r+b", buffering=0) as file:
        before = file_id(os.fstat(file.fileno()))
        if existing:
            require(before == {key: row["existing_identity"][key] for key in before}, "opened target identity changed")
            require(sha(file.read(LIMIT + 1)) == row["preimage_sha256"], "opened preimage changed before effect")
        file.seek(0)
        view = memoryview(data)
        while view:
            written = file.write(view)
            require(written and written > 0, "source write made no progress")
            view = view[written:]
        file.truncate()
        os.fsync(file.fileno())
        require(file_id(os.fstat(file.fileno())) == before, "file identity changed during in-place effect")
        file.seek(0)
        require(sha(file.read(LIMIT + 1)) == row["payload"]["sha256"], "written source differs from reviewed payload")
    return before

def main():
    actor = identity()  # No manifest/source/output access before human verification.
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--manifest-sha256", required=True)
    parser.add_argument("--script-sha256", required=True)
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--receipt")
    args = parser.parse_args()
    require(sha(body(__file__)) == args.script_sha256, "application runner drift")
    raw = body(args.manifest)
    require(sha(raw) == args.manifest_sha256, "approved manifest drift")
    manifest = json.loads(raw)
    require(manifest["schema"] == "airk.private.reviewed-higher-device-application/1" and Path(manifest["repository"]) == REPO, "manifest scope mismatch")
    kinds = [row["kind"] for row in manifest["files"]]
    require(kinds == sorted(kinds, key=lambda kind: 0 if kind == "contract" else 1), "contracts must precede consumers")
    require(len({row["path"] for row in manifest["files"]}) == len(kinds), "duplicate application target")
    payloads = snapshot(manifest)
    if not args.apply:
        print(json.dumps({"audit": "pass", "identity": actor, "files": len(kinds), "contracts": kinds.count("contract"), "applied": False}))
        return
    require(args.receipt is not None, "application needs a fresh private receipt")
    receipt = safe(args.receipt)
    require(receipt.parent == Path(__file__).absolute().parent, "receipt must stay in this private packet")
    require(not receipt.exists(), "receipt already exists")
    journal = receipt.with_suffix(".events.jsonl")
    report = {"scope": "source application only; no native qualification or approval", "identity": actor,
              "manifest_sha256": args.manifest_sha256, "script_sha256": args.script_sha256,
              "complete": False, "applied": [], "created_directories": []}
    # Reserve the two fresh outputs before the first tracked mutation.
    with open(receipt, "x", encoding="utf-8") as record, open(journal, "x", encoding="utf-8") as events:
        def retain(value):
            events.write(json.dumps(value) + "\n")
            events.flush()
            os.fsync(events.fileno())
        try:
            retain({"state": "preflight_complete", "files": len(kinds), "identity": actor})
            for row in manifest["files"]:
                target = safe(REPO / row["path"])
                missing = []
                parent = target.parent
                while not parent.exists():
                    missing.append(parent)
                    parent = parent.parent
                for directory in reversed(missing):
                    os.mkdir(directory)
                    safe(directory)
                    report["created_directories"].append(str(directory))
                retain({"state": "before_write", "path": row["path"]})
                actual_id = write_target(target, payloads[row["path"]], row)
                report["applied"].append({"path": row["path"], "sha256": row["payload"]["sha256"], "identity": actual_id})
                retain({"state": "retained", "path": row["path"], "identity": actual_id})
            paths = [REPO / row["path"] for row in manifest["files"]]
            after = security(paths + [Path(path) for path in report["created_directories"]])
            for row in manifest["files"]:
                target = REPO / row["path"]
                require(sha(body(target)) == row["payload"]["sha256"], "final source drift")
                actual = after[str(target)]
                known = row["existing_identity"]
                if known is not None:
                    require(actual["owner_sid"] == known["owner_sid"] and actual["sddl"] == known["sddl"], "original owner/SDDL was not preserved")
                    require(file_id(target.stat()) == {key: known[key] for key in ("device", "inode", "nlink")}, "original file identity was not preserved")
                else:
                    require(actual["owner_sid"] == SID, "new source owner differs from human")
            for directory in report["created_directories"]:
                require(after[directory]["owner_sid"] == SID, "new directory owner differs from human")
            report["security_after"] = after
            report["complete"] = True
        except BaseException as error:
            report["failure"] = {"type": type(error).__name__, "detail": str(error)}
            raise
        finally:
            record.write(json.dumps(report, indent=2) + "\n")
            record.flush()
            os.fsync(record.fileno())
    print(json.dumps({"complete": True, "files": len(report["applied"]), "receipt": str(receipt)}))

if __name__ == "__main__":
    main()
