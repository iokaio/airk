# SPDX-License-Identifier: Apache-2.0
"""Check inherited notices, file headers and retained source identities for six aligned cases."""

import argparse
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import runpy
import sys
import time
import tomllib


DECLARED_SHA = "a80f26b9026bf0ee3fdf3dc4f9c10523cb6bfa61b734dcd3d61f9b154636c66e"
SPAN_OWNER = "4298c6b464615a305fcbe48e96d5ca92711d4b0a008e7aabbd2f6528e8b79509"
LEGAL = re.compile(r"(?:licen[sc]es?|copying|copyright|notice|patents?|authors|third[-_ ]?party[-_ ]?(?:licen[sc]es?|notices?)|reuse[.]toml|[.]reuse|[.]reuse[.]toml)(?:[._-].*)?$", re.I)
NORMAL = {"100644", "100755"}
DUAL = {"MIT/Apache-2.0", "MIT OR Apache-2.0", "Apache-2.0 OR MIT"}
DISALLOWED_PATHS = {"third_party", "third-party", "vendor", "node_modules", "generated"}


def dependencies():
    path = Path(__file__).with_name("audit_contextbench_confirmation_spans.py")
    if hashlib.sha256(path.read_bytes()).hexdigest() != SPAN_OWNER:
        raise ValueError("span owner changed")
    spans = runpy.run_path(str(path))
    def load(name, pin):
        dep = path.with_name(name)
        if hashlib.sha256(dep.read_bytes()).hexdigest() != pin:
            raise ValueError("dependency changed")
        return runpy.run_path(str(dep))
    return spans, load("screen_repository_confirmation_upstreams.py", spans["SCREEN_SHA"]), load(
        "contextbench_blind_metadata_census.py", spans["OWNER_SHA"])


def lineage_audit(gh, screen, blind, census, repos):
    import pyarrow.parquet as pq

    rows = blind["projected_rows"](Path(census["archive"]["path"]), pq)
    train, _ = blind["json_pin"](Path(census["train_selection"]["path"]), blind["SELECTION_SHA"])
    train_ids = set(train["selected_ids"])
    train_names = {name for row in rows if row["original_inst_id"] in train_ids
                   for name in blind["credible_repository_claims"](row)}
    names = sorted(train_names | repos)
    values, runs = screen["graph_batches"](gh, [(name, name, screen["LINEAGE"]) for name in names], 12)
    chains = {name: screen["lineage"](values[name]) for name in names}
    training_ids = {node["id"] for name in train_names for node in chains[name][0]}
    return {"requests": runs, "train_claims": len(train_names),
            "unresolved_train_claims": sorted(name for name in train_names if not chains[name][1]),
            "repositories": {r: {"lineage": chains[r][0], "resolved": chains[r][1],
                                 "observed_train_family_overlap": any(n["id"] in training_ids for n in chains[r][0])}
                             for r in sorted(repos)}}


def roots(path):
    return {str(p) if str(p) != "." else "" for p in PurePosixPath(path).parents}


def inventory(gh, screen, repo, base, paths):
    # The server's untruncated Git tree supplies every parent/child entry.
    # Filter the returned names in gh before retaining metadata from the selected revision.
    parents = sorted({p for path in paths for p in roots(path)})
    directories = json.dumps(parents)
    names = json.dumps(sorted(paths))
    filt = (f'{directories} as $dirs | {names} as $names | '
            '{sha,truncated,tree:[.tree[] | . as $r | (.path | split("/") | .[:-1] | join("/")) as $d | '
            'select(($names | index($r.path)) != null or ($dirs | index($r.path)) != null or '
            '(($dirs | index($d)) != null and ((.path|split("/")|last|test('
            '"licen|copy|notice|patent|authors|third.party|reuse|^Cargo.toml$|^package.json$";"i"))))) '
            '| {path,mode,type,sha,size}]}')
    commit, st = screen["call_github"](gh, ["--jq", "{sha,tree:.tree.sha}", f"repos/{repo}/git/commits/{base}"])
    if st != "ok" or commit.get("sha") != base or not screen["OID"].fullmatch(commit.get("tree") or ""):
        raise ValueError("pinned commit unresolved")
    root = commit["tree"]
    tree, st = screen["call_github"](gh, ["--jq", filt, f"repos/{repo}/git/trees/{root}?recursive=1"])
    if st != "ok" or tree.get("sha") != root or tree.get("truncated") is not False:
        raise ValueError("complete exact tree unavailable")
    entries = {r["path"]: r for r in tree["tree"]}
    if any(entries.get(p, {}).get("mode") != "040000" for p in parents if p):
        raise ValueError("ancestor is absent or not an ordinary tree")
    lic, status = screen["call_github"](gh, ["--jq", screen["LICENSE_QUERY"], f"repos/{repo}/license?ref={base}"])
    return root, entries, lic if status == "ok" else {"status": status}


def file_signals(raw):
    text = raw.decode("utf-8")
    spdx = sorted(set(re.findall(r"(?im)SPDX-License-Identifier:[ \t]*([^\r\n*]+)", text)))
    head = "\n".join(text.splitlines()[:50])
    return {"spdx": [v.strip() for v in spdx],
            "apache_file_header": bool(re.search(r"Licensed under the Apache License, Version 2.0", head, re.I)),
            "mit_file_header": bool(re.search(r"Use of this source code is governed by an MIT-style license", head, re.I)),
            "copyright_lines": len(re.findall(r"(?im)^.{0,16}(?:copyright|\(c\)).*$", text)),
            "third_party_or_other_license_signal": bool(re.search(
                r"(?i)SPDX-License-Identifier:[^\r\n]*(?:GPL|BSD|MPL)|licensed under (?:the )?(?:GNU|BSD|Mozilla)|"
                r"(?:ported|copied|adapted) from|third.party license", text))}


def private_source_basis(path, root_license, package_license, grants, signals, notices):
    if PurePosixPath(path).name.lower() in {"yarn.lock", "package-lock.json", "cargo.lock"}:
        return "withhold_generated_dependency_inventory"
    if set(PurePosixPath(path).parts) & DISALLOWED_PATHS or signals["third_party_or_other_license_signal"]:
        return "withhold_external_source_review"
    if notices:
        return "withhold_additional_notice_review"
    if signals["spdx"] and not all(x in {"MIT", "Apache-2.0"} | DUAL for x in signals["spdx"]):
        return "withhold_file_spdx_review"
    if root_license == "MIT" and signals["mit_file_header"]:
        return "file_mit_header_and_root"
    if root_license == "Apache-2.0" and signals["apache_file_header"]:
        return "file_apache_header_and_root"
    if signals["apache_file_header"] or signals["mit_file_header"] or signals["spdx"]:
        return "withhold_header_scope_review"
    if package_license in DUAL and {"LICENSE-MIT", "LICENSE-APACHE"} <= grants:
        return "package_dual_declaration_and_both_root_grants"
    if root_license in {"MIT", "Apache-2.0"} and package_license == root_license:
        return "package_and_root_declaration"
    return "withhold_scope_review"


def authenticated(spans, repo, base, entry):
    if entry.get("mode") not in NORMAL or entry.get("type") != "blob":
        raise ValueError("ordinary immutable blob required")
    raw, origin = spans["fetch_blob"]((repo, base, entry["path"]), {"oid": entry["sha"], "byteSize": entry["size"]}, {})
    if raw is None:
        raise ValueError("blob authentication failed: " + origin)
    return raw


def revision(gh, spans, screen, key, inputs, lineage):
    repo, base = key
    root, entries, legal = inventory(gh, screen, repo, base, inputs)
    declared_root = entries.get(legal.get("path")) or {}
    root_spdx = legal.get("spdx") if declared_root.get("sha") and declared_root["sha"] == legal.get("sha") else None
    relevant = {p: ent for p, ent in entries.items() if LEGAL.fullmatch(PurePosixPath(p).name)
                or PurePosixPath(p).name in ("Cargo.toml", "package.json")}
    materials = []
    for path, entry in relevant.items():
        if entry.get("mode") not in NORMAL:
            materials.append(dict(entry, status="manual_directory_or_mode"))
            continue
        raw = authenticated(spans, repo, base, entry)
        text = raw.decode("utf-8")
        m = dict(entry, sha256=hashlib.sha256(raw).hexdigest(), status="authenticated")
        if path.endswith("Cargo.toml"):
            m["package_license"] = tomllib.loads(text).get("package", {}).get("license")
        elif path.endswith("package.json"):
            m["package_license"] = json.loads(text).get("license")
        else:
            m["contains_mit_grant"] = "Permission is hereby granted, free of charge" in text
            m["contains_apache_grant"] = "Apache License" in text and "Version 2.0, January 2004" in text
        materials.append(m)
    out = []
    for path, previous in inputs.items():
        ent = entries.get(path) or {}
        if ent.get("sha") != previous["metadata"]["oid"] or ent.get("size") != previous["metadata"]["byteSize"]:
            raise ValueError("previous blob differs from pinned recursive tree")
        raw = authenticated(spans, repo, base, ent)
        digest = hashlib.sha256(raw).hexdigest()
        if digest != previous["sha256"]:
            raise ValueError("previous source digest differs")
        applicable = [m for m in materials if PurePosixPath(m["path"]).parent in PurePosixPath(path).parents]
        applicable_paths = [m["path"] for m in applicable]
        packages = [m for m in applicable if m.get("package_license")]
        nearest = max(packages, key=lambda m: len(PurePosixPath(m["path"]).parts), default={})
        grants = {m["path"] for m in applicable if (m["path"] == "LICENSE-MIT" and m.get("contains_mit_grant"))
                  or (m["path"] == "LICENSE-APACHE" and m.get("contains_apache_grant"))}
        notices = [m["path"] for m in applicable if LEGAL.fullmatch(PurePosixPath(m["path"]).name)
                   and m["path"] not in grants and m["path"] != legal.get("path")]
        signals = file_signals(raw)
        basis = private_source_basis(path, root_spdx, nearest.get("package_license"), grants, signals, notices)
        if not lineage["resolved"] or lineage["observed_train_family_overlap"]:
            basis = "withhold_repository_lineage"
        out.append(dict(ent, sha256=digest, signals=signals, governing_materials=applicable_paths,
                        additional_notice_paths=notices, nearest_manifest=nearest.get("path"), private_basis=basis,
                        stage_private=not basis.startswith("withhold_")))
    return {"repo": repo, "base": base, "tree": root, "root_license_api": legal,
            "materials": materials, "files": out}


def build(args):
    spans, screen, blind = dependencies()
    screen["pinned_bytes"](args.gh, screen["GH_SHA"], 64 << 20)
    prior, _ = blind["json_pin"](args.directory / "contextbench-14-declared-lines-r1.private.json", DECLARED_SHA)
    census, _ = blind["json_pin"](args.directory.parent / "contextbench-blind-metadata-eligibility-r1.json", spans["CENSUS_SHA"])
    cases = defaultdict(list)
    for r in prior["rows"]:
        cases[r["instance_id"]].append(r)
    aligned = {key: rr for key, rr in cases.items() if all(r["status"] in (
        "exact_declared_lines", "exact_numbered_declared_lines") for r in rr)}
    files = {(r["repo"], r["base"], r["path"]): r for r in prior["files"]}
    groups = defaultdict(dict)
    for rr in aligned.values():
        for r in rr:
            key = r["repo"], r["base"], r["path"]
            groups[key[:2]][key[2]] = files[key]
    if len(aligned) != 6 or sum(map(len, groups.values())) != 14:
        raise ValueError("six fully aligned cases required")
    lineage = lineage_audit(args.gh, screen, blind, census, {k[0] for k in groups})
    def fetch(key):
        return revision(args.gh, spans, screen, key, groups[key], lineage["repositories"][key[0]])
    with ThreadPoolExecutor(max_workers=3) as executor:
        revisions = list(executor.map(fetch, sorted(groups)))
    return {"schema": "airk.private.contextbench-source-rights-and-custody/1", "declared_alignment": DECLARED_SHA,
            "declared_owner_sha256": SPAN_OWNER, "github_cli_sha256": screen["GH_SHA"], "lineage": lineage,
            "cases": {key: {"repo": r[0]["repo"], "base": r[0]["base"],
                            "source_paths": sorted({x["path"] for x in r})} for key, r in aligned.items()},
            "revisions": revisions, "counts": Counter(f["private_basis"] for r in revisions for f in r["files"]),
            "admitted": 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", required=True, type=Path)
    parser.add_argument("--gh", required=True, type=Path)
    args = parser.parse_args()
    if not sys.flags.isolated or not sys.dont_write_bytecode:
        parser.error("use isolated PyArrow Python -I -B")
    started = time.monotonic()
    result = build(args)
    result.update(collector_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  elapsed_seconds=time.monotonic() - started)
    print(json.dumps(result, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
