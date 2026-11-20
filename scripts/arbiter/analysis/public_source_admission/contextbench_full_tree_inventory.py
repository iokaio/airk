# SPDX-License-Identifier: Apache-2.0
"""Freeze complete repository tree metadata for the six ContextBench revision roots."""
import argparse
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import re
import runpy
import time


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
RIGHTS = ROOT / ("artifacts/v190/final-plan/exec-20261116T052942Z/repository-confirmation-multilang-r1/"
                 "contextbench-six-source-rights-r1.private.json")
RIGHTS_SHA = "34d01f50c1e6f4892c4ffcd54243a32af09abc122406a86a9ff5c2e03795a98a"
MODES = {"040000", "100644", "100755", "120000", "160000"}


def verify_tree(rows, root_oid):
    entries = {}
    children = defaultdict(list)
    for path, mode, oid, size in rows:
        parts = path.split("/")
        if (not path or path in entries or any(part in ("", ".", "..") for part in parts)
                or "\0" in path or mode not in MODES or not re.fullmatch("[0-9a-f]{40}", oid)):
            raise ValueError("invalid or duplicate Git entry")
        if mode in {"100644", "100755", "120000"} and (type(size) is not int or size < 0):
            raise ValueError("missing blob length")
        entries[path] = (mode, oid)
        parent, _, name = path.rpartition("/")
        children[parent].append((name.encode("utf-8"), mode, bytes.fromhex(oid)))
    directories = {path: oid for path, (mode, oid) in entries.items() if mode == "040000"}
    directories[""] = root_oid
    if not children.keys() <= directories.keys():
        raise ValueError("tree parent is absent")
    for directory, oid in directories.items():
        siblings = sorted(children[directory], key=lambda item: item[0] + (b"/" if item[1] == "040000" else b""))
        body = b"".join(mode.lstrip("0").encode() + b" " + name + b"\0" + digest
                        for name, mode, digest in siblings)
        frame = b"tree " + str(len(body)).encode() + b"\0" + body
        if hashlib.sha1(frame).hexdigest() != oid:
            raise ValueError(f"incomplete or altered Git tree at {directory!r}")
    return dict(Counter(mode for mode, _ in entries.values()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gh", required=True, type=Path)
    args = parser.parse_args()
    screen = runpy.run_path(str(HERE / "screen_repository_confirmation_upstreams.py"))
    raw = RIGHTS.read_bytes()
    if hashlib.sha256(raw).hexdigest() != RIGHTS_SHA or hashlib.sha256(args.gh.read_bytes()).hexdigest() != screen["GH_SHA"]:
        raise ValueError("source revision roster or GitHub CLI identity differs")
    revisions = json.loads(raw)["revisions"]
    if len(revisions) != 6 or len({r["base"] for r in revisions}) != 6:
        raise ValueError("expected six separate revision roots")

    def collect(revision):
        repo, base, tree = (revision[k] for k in ("repo", "base", "tree"))
        response, status = screen["call_github"](args.gh, ["--jq",
            "{sha,truncated,rows:[.tree[]|[.path,.mode,.sha,.size]]}", f"repos/{repo}/git/trees/{tree}?recursive=1"])
        if status != "ok" or response["sha"] != tree or response["truncated"] is not False:
            raise ValueError(f"complete upstream inventory unavailable: {repo} {status}")
        rows = response["rows"]
        counts = verify_tree(rows, tree)
        return {"repo": repo, "base": base, "tree": tree, "counts_by_mode": counts,
                "ordinary_file_bytes": sum(size for _, mode, _, size in rows if mode in {"100644", "100755"}),
                "entries": rows}

    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=2) as pool:
        result = list(pool.map(collect, revisions))
    print(json.dumps({"source_roster_sha256": RIGHTS_SHA, "source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                      "elapsed_seconds": time.monotonic() - started, "revisions": result,
                      "native_candidate_menus": 0}, ensure_ascii=True, separators=(",", ":")))


if __name__ == "__main__":
    main()
