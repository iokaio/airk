# SPDX-License-Identifier: Apache-2.0
"""Bind the two original fd issue requests to complete independently indexed base trees."""
import argparse
import hashlib
from pathlib import Path
import runpy

import pyarrow.parquet as parquet


HERE = Path(__file__).resolve().parent
base = runpy.run_path(str(HERE.parent / "development_margins/capture_inputs.py"))
archive_owner = runpy.run_path(str(HERE / "contextbench_blind_metadata_census.py"))
focus = runpy.run_path(str(HERE.parent / "development_margins/focus1024.py"))
require, load, digest, encode = (base[k] for k in ("require", "load", "digest", "encode"))
CAMPAIGN = base["CAMPAIGN"]
TREE_ROOT = CAMPAIGN / "contextbench-fd-whole-r1/sources"
TREE_SHA = "0d8dec6fa1f7b2c16ea100e066936d4e84f19e8fd2a1b3bc40db0f731e7e0d8e"
SELECT_SHA = "51adc46cedf7f9d8803a91e790dffae5c2cfbabcc2c9018bae9221ae67dc722b"
CASES = {
    "sharkdp__fd-1394": ("93cdb2628e89dd5831eee22b8df697aea00eca3b", "bf1eb2967b589e26b5ad3a184d5d5e13936deff501d5d1cbd45952aa68b09ef7"),
    "sharkdp__fd-1162": ("cbd11d8a45dc80392c5f1be9679051085e6a3376", "8f7074c1907d7b4e8a3c12a4a2667e6b23149a4cf55a7c294efcf466cb0425d8"),
}
FIELDS = ["original_inst_id", "instance_id", "repo", "base_commit"]


def selected_requests():
    selection = load(CAMPAIGN / "repository-confirmation-multilang-r1/contextbench-14-selection-r1.private.json", SELECT_SHA)
    ids = {r["original_inst_id"]: {k: r[k] for k in FIELDS} for r in selection["rows"] if r["original_inst_id"] in CASES}
    require(ids.keys() == CASES.keys(), "the selected issue pair differs")
    archive = (HERE.parents[3] / "artifacts/advanced-split/models/actual-composed-service-r2/public-human-label-admission-r1"
               / "contextbench-current-blind-train-r1/current-full-release.private-quarantine.parquet")
    with archive_owner["regular"](archive).open("rb") as source:
        before = archive_owner["archive_state"](source)
        require((hashlib.file_digest(source, "sha256").hexdigest(), before[2]) == archive_owner["RELEASE"], "archive pin differs")
        source.seek(0)
        columns = FIELDS + ["problem_statement"]
        table = parquet.read_table(source, columns=columns, filters=[("original_inst_id", "in", sorted(ids))], use_threads=False)
        require(table.column_names == columns and table.num_rows == len(ids) and archive_owner["archive_state"](source) == before,
                "filtered issue projection differs")
        rows = table.to_pylist()
    require({r["original_inst_id"] for r in rows} == ids.keys(), "selected issue set differs")
    for row in rows:
        key = row["original_inst_id"]
        require({k: row[k] for k in FIELDS} == ids[key] and row["base_commit"] == CASES[key][0]
                and row["repo"] == "sharkdp/fd" and row["problem_statement"].strip(), "issue and immutable base disagree")
    return rows


def build():
    trees = load(TREE_ROOT / "manifest.private.json", TREE_SHA)["revisions"]
    shared, template = base["settings"]()
    result = {}
    for row in selected_requests():
        case = row["original_inst_id"]
        revision, receipt_sha = CASES[case]
        tree, = [entry for entry in trees if entry["base"] == revision]
        source_root = TREE_ROOT / revision
        corpus = {f["path"]: f for f in tree["files"]}
        require(len(corpus) == 48 and set(corpus) == {p.relative_to(source_root).as_posix() for p in source_root.rglob("*") if p.is_file()},
                "immutable source population differs")
        for path, info in corpus.items():
            base["pin"](source_root / path, info["sha256"])
        native = load(CAMPAIGN / ("fd-" + revision[:8] + "-native-source-reads-r1/native-reads.private.json"), receipt_sha)
        census, = native["cases"]
        paths = census["native_inventory_order"]
        require(native["base"] == revision and native["git_tree"] == tree["tree"] and census["source_files"] == 48
                and len(paths) == 47 and len(set(paths)) == 47 and set(corpus) - set(paths) == {"doc/logo.png"},
                "native corpus coverage differs")
        question = row["problem_statement"]
        files = {"request.private.json": encode(dict(template, question=question)),
                 "source-scopes.private.json": encode({"source_roots": [str(source_root)]})}
        descriptor = dict(shared, schema="airk.private.fd-exposed-repository-pilot/1", question_id=case,
                          identity={k: row[k] for k in FIELDS}, question_sha256=digest(question.encode()),
                          archive_sha256=archive_owner["RELEASE"][0], selection_sha256=SELECT_SHA,
                          source_root=str(source_root), source_manifest_sha256=TREE_SHA, git_tree=tree["tree"],
                          native_inventory_sha256=receipt_sha, complete_source_paths=sorted(corpus), publisher_order=paths,
                          source_order_role="complete native file inventory at the immutable base",
                          case_role="previously exposed repository engineering", source_projection=FIELDS + ["problem_statement"],
                          private_files={name: digest(data) for name, data in files.items()})
        rebound = focus["bind"](descriptor, digest(encode(descriptor)))
        files["pilot.private.json"] = encode(rebound)
        result[case] = files
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-only", action="store_true")
    args = parser.parse_args()
    output = CAMPAIGN / "contextbench-fd-issue-inputs-r1"
    for case, files in build().items():
        target = output / case
        require(target.resolve(strict=True) == target and (args.check_only or not any(target.iterdir())), "fresh checked output required")
        for name, data in files.items():
            if args.check_only:
                base["pin"](target / name, digest(data))
            else:
                with (target / name).open("xb") as stream:
                    stream.write(data)
        print(case, digest(files["pilot.private.json"]))