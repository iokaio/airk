# SPDX-License-Identifier: Apache-2.0
"""Authenticate the complete pinned Rayon archive and inventory rights-review signals."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import runpy
import tarfile
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener

HERE = Path(__file__).resolve().parent
REVISION = "2de810e97d5ce832ff98023a4a9cf215a86244ea"
URL = f"https://codeload.github.com/rayon-rs/rayon/tar.gz/{REVISION}"
CEILING = 2 << 20


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def inspect(revision, archive):
    expected = {path: (mode, oid, size) for path, mode, oid, size in revision["entries"]}
    seen = set()
    files = []
    prefix = "rayon-" + REVISION
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r|gz") as tar:
        for item in tar:
            if item.name == prefix and item.isdir():
                continue
            path = item.name.removeprefix(prefix + "/")
            if path == item.name or path not in expected or path in seen:
                raise ValueError("unregistered or duplicate archive member")
            seen.add(path)
            mode, oid, size = expected[path]
            if item.isdir() and mode == "040000":
                continue
            if not item.isreg() or mode not in {"100644", "100755"} or item.size != size or size > CEILING:
                raise ValueError("source mode/size differs")
            with tar.extractfile(item) as stream:
                raw = stream.read(size + 1)
            if len(raw) != size or hashlib.sha1(b"blob " + str(size).encode() + b"\0" + raw).hexdigest() != oid:
                raise ValueError("source byte identity differs")
            signals = [line.strip()[:240] for line in raw.decode().splitlines()
                       if re.search(r"(?i)copyright|ported from|copied from|adapted from|derived from|license|attribution|tsplib|waterloo|tsp\.gatech", line)]
            files.append({"path": path, "mode": mode, "oid": oid, "size": size,
                          "sha256": hashlib.sha256(raw).hexdigest(), "rights_review_lines": signals})
    if seen != expected.keys() or len(files) != 207:
        raise ValueError("incomplete source archive")
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--audit", required=True, type=Path)
    args = parser.parse_args()
    owner = runpy.run_path(str(HERE / "contextbench_fd_sources.py"))
    if args.archive.exists() or args.audit.exists() or args.archive.parent != args.audit.parent:
        raise ValueError("need fresh archive and audit in one checked private directory")
    roster = owner["ROSTER"].read_bytes()
    if hashlib.sha256(roster).hexdigest() != owner["ROSTER_SHA"]:
        raise ValueError("source roster changed")
    revision, = [r for r in json.loads(roster)["revisions"] if r["base"] == REVISION and r["repo"] == "rayon-rs/rayon"]
    runpy.run_path(str(HERE / "contextbench_full_tree_inventory.py"))["verify_tree"](revision["entries"], revision["tree"])
    opener = build_opener(ProxyHandler({}), NoRedirect())
    with opener.open(Request(URL, headers={"User-Agent": "AIRK-public-source-audit"}), timeout=25) as response:
        if response.status != 200 or response.geturl() != URL:
            raise ValueError("unexpected source endpoint")
        archive = response.read(CEILING + 1)
    if not 0 < len(archive) <= CEILING:
        raise ValueError("archive exceeds registered bound")
    files = inspect(revision, archive)
    with args.archive.open("xb") as output:
        output.write(archive)
    result = {"base": REVISION, "url": URL, "tree": revision["tree"], "archive_sha256": hashlib.sha256(archive).hexdigest(),
              "roster_sha256": owner["ROSTER_SHA"], "files": files, "rights_review_pending": True}
    with args.audit.open("x", encoding="utf-8") as output:
        json.dump(result, output, indent=2)


if __name__ == "__main__":
    main()
