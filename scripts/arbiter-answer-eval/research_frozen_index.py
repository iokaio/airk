# SPDX-License-Identifier: Apache-2.0
"""Admission and before/after evidence for authenticated final-text frozen-index research tasks."""

import hashlib
import stat
from pathlib import Path, PureWindowsPath

import common
import workspace_scope

FREEZE = "f7eb577e2b66501573f1b3e7754aebff64fdaa5b7f06bd190bb146b3c749f11e"


def admit(task_path, task, repo, manifest):
    design = task_path.resolve(strict=True).parent.parent
    freeze = design / "freeze.json"
    freeze_hash = common.sha256_file(freeze).lower()
    if freeze_hash == "d3a7430c3167371830bbd3304f9430e2a7328cfcff4c53b4e5e30a00232c5a30":
        digests = common.read_json(freeze)
        desc = design / "design.json"
        common.require(common.sha256_file(desc).lower() == digests["design.json"].lower(), "research design changed")
        selected = [r for r in common.read_json(desc)["tasks"] if r["id"] == task["id"] and
                    r["repository"] == repo and r["kind"] == task["kind"] == "research"]
        common.require(len(selected) == 1, "expected one authenticated final-text research task")
        name = selected[0]["id"] + ".json"
        common.require(task_path.resolve() == design / "solver" / name, "unexpected solver input path")
        for folder in ("solver", "grading"):
            rel = f"{folder}/{name}"
            common.require(common.sha256_file(design / rel).lower() == digests[rel].lower(), "research input changed")
        native_state = design.parent / "E2E-single-r1/native-state"
        common.require(manifest.resolve(strict=True).parent == native_state.resolve(strict=True) and
                       manifest.name in ("frozen-eligible-serving.json", "frozen-eligible-paid.json"),
                       "unexpected native receipt")
        # The prospective freeze pins the original commit, while the authenticated fixture
        # design pins the native registration for that same repository and commit.
        prior = design.parent / "E2E-design-r1" / "freeze.json"
        common.require(common.sha256_file(prior).lower() == FREEZE, "original source registration freeze changed")
        ids = {r["index_id"] for r in common.read_json(prior)["tasks"]
               if r["repository"] == repo and r["source_commit"] == selected[0]["source_commit"]}
        common.require(len(ids) == 1, "expected one original frozen index for this source revision")
        return {**selected[0], "index_id": ids.pop()}
    common.require(freeze_hash == FREEZE, "unrecognized research design freeze")
    rows = [row for row in common.read_json(freeze)["tasks"] if row["kind"] == "research"]
    selected = [row for row in rows if row["id"] == task["id"] and row["repository"] == repo]
    common.require(len(rows) == 4 and len(selected) == 1 and task["kind"] == "research",
                   "frozen index requires one of the four registered final-text tasks")
    name = selected[0]["id"] + ".json"
    common.require(task_path.resolve() == design / "solver" / name
                   and common.sha256_file(task_path).lower() == selected[0]["solver_sha256"].lower()
                   and common.sha256_file(design / "grading" / name).lower() == selected[0]["grader_sha256"].lower(),
                   "frozen solver or rubric identity differs")
    common.require(manifest.resolve(strict=True) ==
                   (design.parent / "E2E-r1/native-state/frozen-eligible-r3.json").resolve(strict=True),
                   "expected the E2E-r1 native eligible-source receipt")
    return selected[0]


def regular(path, directory=False):
    info = path.lstat()
    common.require(not (getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT)
                   and (stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode)),
                   f"unexpected file type in research workspace: {path}")


def eligible_digest(root, paths):
    digest = hashlib.sha256()
    regular(root, directory=True)
    dirs = {root}
    for name in sorted(paths):
        pieces = name.split("/")
        common.require(not PureWindowsPath(name).drive and "\\" not in name and ":" not in name and
                       all(piece not in ("", ".", "..") and not piece.endswith((" ", ".")) and
                           not PureWindowsPath(piece).is_reserved() for piece in pieces),
                       "unsafe native eligible path")
        path = root
        for part in pieces[:-1]:
            path = path / part
            if path not in dirs:
                regular(path, directory=True)
                dirs.add(path)
        path = path / pieces[-1]
        regular(path)
        digest.update(name.encode("utf-8") + b"\x00" + bytes.fromhex(common.sha256_file(path)))
    return digest.hexdigest()


def tree_digest(root):
    regular(root, directory=True)
    digest = hashlib.sha256()

    def walk(folder):
        for item in sorted(folder.iterdir(), key=lambda p: p.name):
            key = item.relative_to(root).as_posix().encode("utf-8") + b"\x00"
            directory = stat.S_ISDIR(item.lstat().st_mode)
            regular(item, directory)
            if directory:
                digest.update(b"D" + key)
                walk(item)
            else:
                digest.update(b"F" + key + bytes.fromhex(common.sha256_file(item)))

    walk(root)
    return digest.hexdigest()


def public_authority(observation):
    # Each new native grant has its own nonce in the opaque fingerprint.
    # Preserve every public semantic field, including additions to an observation.
    return {key: value for key, value in observation.items() if key != "fingerprint"}


def observe(handle, repo, manifest, baseline, work):
    record = common.read_json(manifest)[repo]
    fixture = common.read_json(handle.path)
    registrations = fixture["indexes"]
    common.require(record["service_instance"] == fixture["instance"] and
                   record["service_sha256"] == fixture["image"]["service"]["sha256"], "frozen service changed")
    index = handle.index_id(repo)
    registered = [r for r in registrations.values() if r["index_id"] == index]
    common.require(len(registered) == 1 and record["index"] == index and record["denies"] == registered[0]["denies"]
                   and common.sha256_file(handle.path) == handle.sha256, "frozen native registration changed")
    source = Path(registered[0]["root"]).resolve(strict=True)
    common.require(Path(record["root"]).resolve(strict=True) == source and
                   Path(record["baseline"]).resolve(strict=True) == baseline and work.resolve(strict=True) == work
                   and all(not work.is_relative_to(p) and not p.is_relative_to(work) for p in (source, baseline)),
                   "frozen native source, baseline or fresh work binding differs")
    metrics = {}
    raw = {}
    paths = workspace_scope.eligible_paths(handle, repo, metrics, observation=raw)
    authority = public_authority(raw)
    common.require(authority.get("schema") == 1 and authority.get("qualifier") == "selected_subset" and
                   authority.get("selection") and [m["index_id"] for m in authority["members"]] == [index] and
                   authority == public_authority(record["scope_observation"]), "frozen public authority changed")
    common.require(paths and len(paths) == record["file_count"] and not metrics["inventory_cached"] and
                   record["publication"] == metrics["publication"],
                   f"frozen inventory differs: count={len(paths)}, publication={metrics.get('publication')}, "
                   f"fingerprint={metrics.get('scope_fingerprint')}")
    by_root = {p: eligible_digest(p, paths) for p in {source, baseline, work}}
    common.require(set(by_root.values()) == {record["digest"].lower()}, "frozen eligible path or source bytes changed")
    tree = tree_digest(work)
    common.require(tree == tree_digest(baseline), "research checkout differs from the full sanitized baseline")
    return {"repo": repo, "index": index, "handle_sha256": handle.sha256, "manifest_sha256": common.sha256_file(manifest),
            "root": str(source), "baseline": str(baseline), "work": str(work), "file_count": len(paths),
            "eligible_digest": by_root[work], "tree_digest": tree, "scope_observation": raw,
            "service_instance": record["service_instance"], "service_sha256": record["service_sha256"], **metrics}


def finish(before, after, tool_uses, changed, answer):
    return (all(after[k] == value for k, value in before.items()
                if k not in ("inventory_seconds", "scope_fingerprint", "scope_observation")) and
            public_authority(after["scope_observation"]) == public_authority(before["scope_observation"]) and
            not changed and not any(tool_uses.get(tool) for tool in ("Edit", "Write")) and bool(answer.strip()))
