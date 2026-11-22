# SPDX-License-Identifier: Apache-2.0
"""Split uncommitted working-tree work into cumulative per-series patch sets.

Evaluation tooling, never product. Each series of the Answer evaluation lands
as one commit holding exactly that series' code. This tool reads a spec of
file and hunk assignments, builds the cumulative state of every series in a
temporary index, writes one binary-safe LF patch per series and a manifest,
and verifies the result. The last series always equals the working tree, with
CRLF line endings normalized to LF.

It never touches the real index or the working tree: the status read uses
GIT_OPTIONAL_LOCKS=0, and every index operation uses GIT_INDEX_FILE under the
output folder. It does write blobs into the repository's object database.

Run: python -I -B scripts/arbiter-answer-eval/commit_split/split.py \
       --repo <checkout> --spec <spec.json> --private-root <folder> --out <folder>
       [--export-check <empty folder>]
"""

from __future__ import annotations

import argparse
import difflib
import fnmatch
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

SPEC_SCHEMA = "airk.answer-eval.commit-split-spec/1"
MANIFEST_SCHEMA = "airk.answer-eval.commit-split-manifest/1"


class SplitError(Exception):
    pass


class Git:
    def __init__(self, repo: Path):
        self.repo = repo

    def run(self, *args: str, inp: bytes | None = None, env: dict | None = None,
            ok: bool = False, cwd: Path | None = None, git_dir: bool = True) -> bytes | None:
        e = dict(os.environ)
        e["GIT_OPTIONAL_LOCKS"] = "0"
        if env:
            e.update(env)
        cmd = ["git", "-c", "core.autocrlf=false", "-c", "core.eol=lf"]
        if git_dir:
            cmd += ["-C", str(self.repo)]
        p = subprocess.run(cmd + list(args), input=inp, capture_output=True, env=e, cwd=cwd)
        if p.returncode:
            if ok:
                return None
            raise SplitError("git %s failed: %s" % (" ".join(args), p.stderr.decode(errors="replace").strip()))
        return p.stdout


def sha(data: bytes | None) -> str | None:
    return hashlib.sha256(data).hexdigest() if data is not None else None


def lf(data: bytes) -> bytes:
    return data.replace(b"\r\n", b"\n")


def changes(base: bytes, final: bytes):
    a = base.splitlines(keepends=True)
    b = final.splitlines(keepends=True)
    ops = difflib.SequenceMatcher(None, a, b, autojunk=False).get_opcodes()
    return a, b, [o for o in ops if o[0] != "equal"]


def from_final(base: bytes, final: bytes, rule: dict) -> bytes:
    """`final` with change blocks `revert` (indices of the base->final change
    blocks, in order) restored to the base text, the final lines in `drop`
    (1-based inclusive ranges, inside changed text) removed, then the exact
    text replacements of `replace` ([old, new, expected count]) applied."""
    revert = set(rule.get("revert", ()))
    a, b, blocks = changes(base, final)
    if revert - set(range(len(blocks))):
        raise SplitError("revert index out of range (%d change blocks)" % len(blocks))
    dropped: set[int] = set()
    for lo, hi in rule.get("drop", ()):
        dropped.update(range(lo - 1, hi))
    changed = {j for i, (_, _, _, j1, j2) in enumerate(blocks) if i not in revert for j in range(j1, j2)}
    if not dropped <= changed:
        raise SplitError("drop range outside changed lines")
    out: list[bytes] = []
    pos = 0
    for i, (_, i1, i2, j1, j2) in enumerate(blocks):
        out += a[pos:i1]
        out += a[i1:i2] if i in revert else [b[j] for j in range(j1, j2) if j not in dropped]
        pos = i2
    out += a[pos:]
    text = b"".join(out)
    for old, new, count in rule.get("replace", ()):
        old_b, new_b = old.encode(), new.encode()
        if text.count(old_b) != count:
            raise SplitError("replace %r: found %d, expected %d" % (old[:40], text.count(old_b), count))
        text = text.replace(old_b, new_b)
    for forbidden in rule.get("absent", ()):
        if forbidden.encode() in text:
            raise SplitError("text %r still present" % forbidden)
    return text


class Splitter:
    def __init__(self, repo: Path, spec: dict, private_root: Path | None, out: Path):
        if spec.get("schema") != SPEC_SCHEMA:
            raise SplitError("spec schema must be " + SPEC_SCHEMA)
        self.git = Git(repo)
        self.repo = repo
        self.spec = spec
        self.series: list[str] = spec["series"]
        if len(self.series) < 1 or len(set(self.series)) != len(self.series):
            raise SplitError("series must be a non-empty list of distinct names")
        self.private_root = private_root
        self.out = out
        self.index = out / "split-index.tmp"

    def head(self, path: str) -> bytes | None:
        return self.git.run("show", "HEAD:" + path, ok=True)

    def final(self, path: str) -> bytes | None:
        file = self.repo / path
        return lf(file.read_bytes()) if file.exists() else None

    def changed_paths(self) -> list[tuple[str, bool]]:
        raw = self.git.run("status", "--porcelain=v1", "-uall", "-z", "--no-renames")
        paths = []
        for item in raw.decode().split("\0"):
            if item:
                path = item[3:]
                crlf = (self.repo / path).exists() and b"\r\n" in (self.repo / path).read_bytes()
                paths.append((path, crlf))
        return sorted(paths)

    def rule(self, path: str) -> dict | None:
        if path in self.spec.get("excluded", {}):
            return {"excluded": self.spec["excluded"][path]}
        if path in self.spec.get("files", {}):
            return self.spec["files"][path]
        for pattern, rule in self.spec.get("globs", {}).items():
            if fnmatch.fnmatchcase(path, pattern):
                return rule
        return None

    def root_file(self, name: str, path: str) -> bytes:
        root = self.spec.get("roots", {}).get(name)
        if root is None or self.private_root is None:
            raise SplitError("root %r needs the spec root and --private-root" % name)
        if isinstance(root, str):
            root = {"dir": root}
        rel = path.replace("/", "__") if root.get("flatten") else path
        file = self.private_root / root["dir"] / rel
        if not file.is_file():
            raise SplitError("root %s has no %s" % (name, rel))
        return file.read_bytes()

    def source(self, value, path: str, previous: bytes | None, final: bytes | None) -> bytes | None:
        if value == "prev":
            return previous
        if value == "HEAD":
            return self.head(path)
        if value == "final":
            return final
        if isinstance(value, dict) and "root" in value:
            return self.root_file(value["root"], path)
        if isinstance(value, dict) and "from_final" in value:
            if final is None:
                raise SplitError("from_final on a deleted file")
            return from_final(self.head(path) or b"", final, value["from_final"])
        raise SplitError("unknown source %r for %s" % (value, path))

    def states(self) -> tuple[dict, dict]:
        states: dict[str, dict[str, bytes | None]] = {s: {} for s in self.series}
        manifest: dict = {"files": {}, "excluded": {}, "crlf_normalized": [], "unassigned": []}
        last = self.series[-1]
        for path, crlf in self.changed_paths():
            rule = self.rule(path)
            if rule is None:
                manifest["unassigned"].append(path)
                continue
            if "excluded" in rule:
                manifest["excluded"][path] = rule["excluded"]
                continue
            if crlf:
                manifest["crlf_normalized"].append(path)
            head, final = self.head(path), self.final(path)
            entry = {"head_sha256": sha(head), "final_sha256": sha(final), "reason": rule.get("reason", "")}
            if "series" in rule:
                if rule["series"] not in self.series:
                    raise SplitError("%s: unknown series %s" % (path, rule["series"]))
                start = self.series.index(rule["series"])
                for i, s in enumerate(self.series):
                    states[s][path] = final if i >= start else head
                entry["series"] = rule["series"]
            elif "states" in rule:
                unknown = set(rule["states"]) - set(self.series)
                if unknown or last in rule["states"]:
                    raise SplitError("%s: states may name only series before %s" % (path, last))
                previous = head
                for s in self.series[:-1]:
                    previous = states[s][path] = self.source(rule["states"].get(s, "prev"), path, previous, final)
                states[last][path] = final
                entry["states_sha256"] = {s: sha(states[s][path]) for s in self.series}
                entry["hunks"] = rule.get("hunks", {})
            else:
                raise SplitError("%s: rule needs series or states" % path)
            manifest["files"][path] = entry
        if manifest["unassigned"]:
            raise SplitError("unassigned paths: " + ", ".join(manifest["unassigned"]))
        return states, manifest

    def build(self) -> dict:
        self.out.mkdir(parents=True, exist_ok=True)
        states, manifest = self.states()
        env = {"GIT_INDEX_FILE": str(self.index)}
        git = self.git
        try:
            git.run("read-tree", "HEAD", env=env)
            trees = {"HEAD": git.run("write-tree", env=env).decode().strip()}
            previous = trees["HEAD"]
            for s in self.series:
                git.run("read-tree", previous, env=env)
                info = []
                for path, content in states[s].items():
                    present = git.run("ls-files", "--", path, env=env).strip() != b""
                    if content is None:
                        if present:
                            info.append("0 %s\t%s" % ("0" * 40, path))
                        continue
                    blob = git.run("hash-object", "-w", "--no-filters", "--stdin", inp=content).decode().strip()
                    mode = (git.run("ls-tree", "HEAD", "--", path) or b"").decode().split()
                    info.append("%s %s\t%s" % (mode[0] if mode else "100644", blob, path))
                if info:
                    git.run("update-index", "--index-info", inp=("\n".join(info) + "\n").encode(), env=env)
                trees[s] = previous = git.run("write-tree", env=env).decode().strip()
        finally:
            if self.index.exists():
                self.index.unlink()
        manifest["schema"] = MANIFEST_SCHEMA
        manifest["head"] = git.run("rev-parse", "HEAD").decode().strip()
        manifest["series"] = self.series
        manifest["trees"] = trees
        manifest["patches"] = {}
        previous = trees["HEAD"]
        for s in self.series:
            patch = git.run("diff", "--binary", "--full-index", "--no-renames", "--no-color",
                            "--no-ext-diff", previous, trees[s])
            (self.out / (s + ".patch")).write_bytes(patch)
            names = git.run("diff", "--name-only", "--no-renames", previous, trees[s]).decode().split()
            manifest["patches"][s] = {"sha256": sha(patch), "bytes": len(patch), "files": len(names), "tree": trees[s]}
            previous = trees[s]
        manifest["verification"] = self.verify(trees)
        (self.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")
        return manifest

    def verify(self, trees: dict) -> dict:
        """Reapply every patch to a fresh temporary index and compare trees;
        whitespace-check each step; refuse a CR byte in any patch."""
        env = {"GIT_INDEX_FILE": str(self.index)}
        git = self.git
        try:
            git.run("read-tree", "HEAD", env=env)
            previous = trees["HEAD"]
            for s in self.series:
                patch = self.out / (s + ".patch")
                if b"\r" in patch.read_bytes():
                    raise SplitError("%s.patch holds a CR byte" % s)
                if patch.stat().st_size:
                    git.run("apply", "--cached", "--whitespace=nowarn", str(patch), env=env)
                tree = git.run("write-tree", env=env).decode().strip()
                if tree != trees[s]:
                    raise SplitError("reapplied %s gives %s, expected %s" % (s, tree, trees[s]))
                check = git.run("diff", "--check", previous, tree, ok=True)
                if check is None:
                    raise SplitError("git diff --check failed for series %s" % s)
                previous = tree
        finally:
            if self.index.exists():
                self.index.unlink()
        return {"reapplied_on_temporary_index": True, "diff_check": "clean", "patches_lf_only": True}

    def export_check(self, folder: Path, manifest: dict) -> dict:
        """git archive HEAD into `folder`, apply each patch in order outside the
        repository, compare each step byte for byte with its tree, compare the
        last step with the working tree, then delete `folder`."""
        if folder.exists() and any(folder.iterdir()):
            raise SplitError("export folder must be empty")
        folder.mkdir(parents=True, exist_ok=True)
        git = self.git
        result: dict = {}
        try:
            archive = git.run("archive", "--format=tar", "HEAD")
            with tempfile.TemporaryFile(dir=self.out) as handle:
                handle.write(archive)
                handle.seek(0)
                with tarfile.open(fileobj=handle) as tar:
                    tar.extractall(folder, filter="tar")
            env = {"GIT_CEILING_DIRECTORIES": str(folder.parent)}
            for s in self.series:
                patch = self.out / (s + ".patch")
                if patch.stat().st_size:
                    git.run("apply", "--whitespace=nowarn", str(patch), env=env, cwd=folder, git_dir=False)
                mismatches = self.compare_tree(folder, manifest["trees"][s])
                if mismatches:
                    raise SplitError("export after %s differs: %s" % (s, mismatches[:5]))
                result[s] = "matches tree"
            identical = crlf_only = 0
            for path in manifest["files"]:
                live = self.repo / path
                built = folder / path
                live_bytes = live.read_bytes() if live.exists() else None
                built_bytes = built.read_bytes() if built.exists() else None
                if live_bytes == built_bytes:
                    identical += 1
                elif live_bytes is not None and built_bytes == lf(live_bytes):
                    crlf_only += 1
                else:
                    raise SplitError("export differs from the working tree: " + path)
            result["working_tree"] = {"identical": identical, "crlf_only": crlf_only}
        finally:
            shutil.rmtree(folder)
        return result

    def compare_tree(self, folder: Path, tree: str) -> list[str]:
        listing = self.git.run("ls-tree", "-r", "-z", tree).decode().split("\0")
        expected = {}
        for line in listing:
            if line:
                meta, path = line.split("\t", 1)
                expected[path] = meta.split()[2]
        found = set()
        bad = []
        for file in folder.rglob("*"):
            if file.is_file():
                path = file.relative_to(folder).as_posix()
                found.add(path)
                data = file.read_bytes()
                blob = hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest()
                if expected.get(path) != blob:
                    bad.append(path)
        bad += sorted(set(expected) - found)
        return bad


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--spec", required=True, type=Path)
    parser.add_argument("--private-root", type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--export-check", type=Path)
    args = parser.parse_args(argv)
    spec = json.loads(args.spec.read_text(encoding="utf-8"))
    splitter = Splitter(args.repo.resolve(), spec, args.private_root, args.out)
    try:
        manifest = splitter.build()
        if args.export_check:
            manifest["verification"]["export"] = splitter.export_check(args.export_check, manifest)
            (args.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")
    except SplitError as error:
        print("commit split failed: %s" % error, file=sys.stderr)
        return 1
    summary = {"patches": manifest["patches"], "crlf_normalized": manifest["crlf_normalized"],
               "excluded": manifest["excluded"], "export": manifest["verification"].get("export")}
    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
