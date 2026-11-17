# SPDX-License-Identifier: Apache-2.0
"""Reproducible embedded-source plan/apply; never executes extracted code.

Apply requires a reviewed plan and external same-human parent/output audits.
"""
import argparse
import ast
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[4]
BASE = Path(__file__).resolve().parent
SCRIPT_KEYS = {"script", "windows_script", "linux_script", "guest_script", "remote_script", "guarded_script", "guarded_receiver_script", "proposed_utf8", "stdin"}


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def pointer(document, location):
    if location == "$":
        return document
    if not location.startswith("$/"):
        raise ValueError("Expected scanner JSON pointer")
    value = document
    for part in location[2:].split("/"):
        part = part.replace("~1", "/").replace("~0", "~")
        value = value[int(part)] if isinstance(value, list) else value[part]
    return value


def candidate(location):
    key = location.rsplit("/", 1)[-1]
    return key in SCRIPT_KEYS or key in {"patch", "new"} or "/new_lines/" in location or bool(re.search(r"/(?:command|arguments)/\d+$", location)) or location == "$/command"


def classify(text, location):
    key = location.rsplit("/", 1)[-1]
    if key == "patch":
        return "patch", ".patch", ""
    if key == "new" or "/new_lines/" in location:
        return "source-fragment", ".source.in", "# "
    if key == "proposed_utf8":
        return "rust-template", ".rs.in", "// "
    try:
        ast.parse(text)
        return "python", ".py", "# "
    except SyntaxError:
        if re.search(r"(?m)^\s*(?:set -|[A-Za-z_]+\s*=\s*\$\(|cd |export |#!/.*sh)", text):
            return "posix-shell", ".sh", "# "
        if "$ErrorActionPreference" in text or "[System." in text or "Get-" in text:
            return "powershell", ".ps1", "# "
        raise ValueError("Unreviewed executable language at " + location)


def current_owners():
    paths = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=True).stdout.decode().split("\0")
    owners = {}
    for name in paths:
        p = ROOT / name
        if not name or not p.is_file() or p.stat().st_size > 16777216:
            continue
        if p.suffix.lower() not in {".py", ".rs", ".in", ".ps1", ".sh", ".patch", ".js", ".ts", ".cjs"}:
            continue
        raw = p.read_bytes()
        owners.setdefault(sha(raw), (name, ""))
        first, separator, body = raw.partition(b"\n")
        if separator and b"SPDX-License-Identifier: Apache-2.0" in first:
            owners.setdefault(sha(body), (name, (first + separator).decode()))
    return owners


def plan(inventory):
    raw = inventory.read_bytes()
    report = json.loads(raw)
    owners = current_owners()
    reachable = set(line.split(" ", 1)[0] for line in subprocess.run(["git", "rev-list", "--objects", "--all"], cwd=ROOT, capture_output=True, text=True, check=True).stdout.splitlines())
    records, exclusions, new = [], [], {}
    for file in report["files"]:
        if file["status"] != "source_indicators" or Path(file["path"]).suffix.lower() != ".json":
            continue
        locations = sorted({row["location"] for row in file["indicators"] if candidate(row["location"])})
        if not locations:
            continue
        container = Path(file["path"])
        data = container.read_bytes()
        if sha(data) != file["sha256"]:
            raise ValueError("Container drift: " + str(container))
        document = json.loads(data)
        upstream_url = document.get("html_url", document.get("url", "")) if isinstance(document, dict) else ""
        for location in locations:
            text = pointer(document, location)
            if not isinstance(text, str):
                raise ValueError("Source indicator is not a string")
            payload = text.encode("utf-8")
            digest = sha(payload)
            identity = dict(container=container.relative_to(ROOT).as_posix(), container_sha256=sha(data), container_bytes=len(data), json_pointer=location[1:], decoded_sha256=digest, decoded_bytes=len(payload))
            if location.endswith("/patch") and upstream_url.startswith(("https://github.com/", "https://api.github.com/repos/")) and "/iokaio/airk/" not in upstream_url:
                exclusions.append(dict(**identity, disposition="upstream_commit_patch", evidence_url=upstream_url, reason="Retained upstream commit API response, not AIRK-authored source"))
                continue
            language, extension, marker = classify(text, location)
            git_blob = hashlib.sha1(b"blob " + str(len(payload)).encode() + b"\0" + payload).hexdigest()
            prefix = ""
            if digest in owners:
                destination, prefix = owners[digest]
                disposition = "existing-current-maintained-owner"
                maintained = (ROOT / destination).read_bytes()
            else:
                if digest not in new:
                    family = "linux" if "linux-" in str(container) else "neo" if "neo-" in str(container) or "guarded_receiver" in location else "patches" if language in {"patch", "source-fragment", "rust-template"} else "validation"
                    name = re.sub(r"[^A-Za-z0-9_.-]", "_", container.stem + "-" + location.rsplit("/", 1)[-1])[:100]
                    destination = (BASE / family / (name + "--" + digest[:12] + extension)).relative_to(ROOT).as_posix()
                    if marker and not (payload.splitlines() and b"SPDX-License-Identifier: Apache-2.0" in payload.splitlines()[0]):
                        prefix = marker + "SPDX-License-Identifier: Apache-2.0" + ("\r\n" if b"\r\n" in payload else "\n")
                    new[digest] = (destination, prefix)
                destination, prefix = new[digest]
                maintained = prefix.encode() + payload
                disposition = "extracted-source"
            records.append(dict(**identity, destination=destination, maintained_sha256=sha(maintained), maintained_bytes=len(maintained), prefix=prefix, language=language, disposition=disposition, exact_git_blob=git_blob, git_blob_reachable_at_plan=git_blob in reachable))
    return dict(schema="airk.embedded-source-migration/1", inventory=dict(path=inventory.relative_to(ROOT).as_posix(), sha256=sha(raw)), files=records, exclusions=exclusions, inspection_gaps=dict(status_counts=dict(__import__("collections").Counter(r["status"] for r in report["files"])), permission_errors=len(report["errors"]), source_indicator_files=sum(r["status"] == "source_indicators" for r in report["files"]), selected_container_count=len({r["container"] for r in records + exclusions}), unselected_indicator_disposition="Remaining documentation, request/result/data fields and snippets require independent provenance classification; not silently excluded", complete_source_coverage=False))


def apply(value):
    # Parent must audit exactly these paths before invoking this action.
    if value.get("schema") != "airk.embedded-source-migration/1":
        raise ValueError("Unexpected source map schema")
    if (BASE / "source-provenance.json").exists():
        raise ValueError("Never overwrite source provenance")
    if any(row.get("disposition") not in {"extracted-source", "existing-current-maintained-owner"} for row in value["files"]):
        raise ValueError("Unknown source disposition")
    seen = set()
    for row in value["files"]:
        source = ROOT / row["container"]
        raw = source.read_bytes()
        if sha(raw) != row["container_sha256"]:
            raise ValueError("Container drift")
        body = pointer(json.loads(raw), "$" + row["json_pointer"]).encode()
        if sha(body) != row["decoded_sha256"]:
            raise ValueError("Decoded source drift")
        target = (ROOT / row["destination"]).resolve()
        payload = row["prefix"].encode() + body
        if len(body) != row["decoded_bytes"] or len(raw) != row["container_bytes"]:
            raise ValueError("Source length drift")
        if sha(payload) != row["maintained_sha256"] or len(payload) != row["maintained_bytes"]:
            raise ValueError("Maintained payload drift")
        if row["disposition"] != "extracted-source":
            if target.read_bytes() != payload:
                raise ValueError("Existing owner drift")
            continue
        if not target.is_relative_to(BASE) or not target.parent.is_dir():
            raise ValueError("Unaudited destination parent")
        payload = row["prefix"].encode() + body
        if sha(payload) != row["maintained_sha256"]:
            raise ValueError("Maintained bytes drift")
        if target in seen:
            continue
        if target.exists():
            raise ValueError("Never overwrite an extraction")
        target.write_bytes(payload)
        seen.add(target)
    (BASE / "source-provenance.json").write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["plan", "apply"])
    parser.add_argument("--inventory", type=Path)
    args = parser.parse_args()
    if args.action == "plan":
        print(json.dumps(plan(args.inventory.resolve())))
    else:
        apply(json.load(sys.stdin))
