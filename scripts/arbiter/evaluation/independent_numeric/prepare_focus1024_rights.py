# SPDX-License-Identifier: Apache-2.0
"""Reconcile exact R3 Focus assets with the original approved rights and notice files."""
import argparse
import json
from pathlib import Path

from maintained_helpers import REPOSITORY, private_root, worker


def prepare(work):
    def pin(path, sha):
        path = worker.pinned({"path": str(path), "sha256": sha})
        return {"path": str(path), "bytes": path.stat().st_size, "sha256": sha}

    def read(path, sha=None, sha256=None):
        sha = sha or sha256
        row = pin(path, sha)
        return json.loads(Path(row["path"]).read_bytes())

    original = REPOSITORY / "artifacts/advanced-split/models/selected-model-rights-r1"
    old_sha = "bc057cf75609db78fe41c466636e35acb2902327ce366f15816b28be5a809a19"
    approval = read(original / "root-raw-rights-review.json", "6eea1a372086a976dedb3b73e52c1ec73221a5bd1022e3673738a75591839307")
    if old_sha not in approval["approved_rights_rosters"]:
        raise ValueError("original Focus rights missing")
    model = "arbiter-focus-qwen3-reranker-06b"
    roster = read(original / model / "rights-roster.raw.json", old_sha)
    prior = {r["asset"]["name"]:r for r in roster["assets"]}
    request = read(work / "resource-run-input.json","7f90756550929d44041131762d982ff5646f7927de00ff1afc5451574ee35892")
    profile = read(**request["profile"])
    execution = read(work / "owned-numeric/canonical-execution.json","162f8f1117302469a9f5de39883b5e78a24d10e838b1315b379a8413236eee25")
    if profile["model"] != model or profile["manifest"] != "94c65d0c598b0b110cd25ddeb1c167ae730430197c0d5e6a59d79ee226c17c64":
        raise ValueError("wrong width or manifest")
    extra = []
    def note(path, sha):
        extra.append(pin(path, sha))
        return sha

    campus = work.parent
    export = campus / "focus1024-cpu-export-r2"
    owner_sha = note(export/"owned.json","5a4595b76225b46d395e44f6ced5e1d24ebe8007d31e889a0d7e9851e62713c0")
    graph_sha = note(export/"graph.private.json","1a6afa3ac809453ce03510f00dd191bcd840ab3a6636f78c1c1dfe151181e4d7")
    compiler_sha = note(campus/"focus1024-native-compiler-r1/compiler-receipt.json","6ab96f2f6f919b8d93fb0a19924727015904e7c6fc41a302758e5d3e8035126b")
    bootstrap_sha = note(campus/"focus1024-native-profile-r1/bootstrap-receipt.json","28497081cfe433c56a8d2a7b0c5e5abc092a8739e9a8dd32bb5165d27bc0bab8")
    owner = read(export/"owned.json", owner_sha)
    native = read(work/"source-manifest.json","af837c04fb579b5222f83789c5dd88ecd5afdac474d8a7f8849e6c5d1b2c1246")
    [issuer] = [item for item in native["source_files"] if item["path"].endswith("focus_static1024.rs")]
    issuer_sha = note(Path(issuer["path"]),issuer["sha256"])
    extractors = [note(REPOSITORY/"scripts/arbiter/export/qwen/focus1024"/name,sha)
                  for name,sha in owner["source_sha256"].items()]
    graph = read(export/"graph.private.json", graph_sha)
    widths = read(campus/"focus1024-native-compiler-r1/compiler-receipt.json", compiler_sha)
    boot = read(campus/"focus1024-native-profile-r1/bootstrap-receipt.json", bootstrap_sha)
    if graph["width"] != 1024 or boot["manifest"] != profile["manifest"] or widths["compiler"] != profile["compiler"]:
        raise ValueError("different derivation")
    derivations = {
        "model.onnx": ("model.onnx", [owner_sha,graph_sha,*extractors],
           "Qwen/Qwen3-Reranker-0.6B revision e61197ed45024b0ed8a2d74b80b4d909f1255473. AIRK lifted only static input dimensions, shape constants, positions and mask to 1024 in the prior W8 graph. External weights kept their original digest. The reranker publisher card declares Apache-2.0; its distinct base-model LICENSE remains attributed to the base repository."),
        "compiler.json": ("compiler.json",[compiler_sha,issuer_sha],
           "AIRK native producer validated the original Focus compiler, publisher tokenizer and task policy and issued the 1024 compiler."),
        "focus-relevance-metadata.json":("focus-relevance-metadata.json",[bootstrap_sha,issuer_sha],
           "AIRK Focus bootstrap emitted raw relevance interpretation and yes/no readout metadata for the verified 1024 compiler and model assets. This asset carries no learned probability calibration."),
        "native-runtime1024-fixture.json":("native-runtime512-fixture.json",[owner_sha,graph_sha,bootstrap_sha,*extractors],
           "The 14 publisher/synthetic engineering controls preserve the original 12 cases and add synthetic-513 and synthetic-1024. The reference source and publisher readout are recorded in the actual exporter closure; no adjudicated held-out answer data were added.")
    }
    retained = []
    changed = {}
    for asset in profile["assets"]:
        name = asset["name"]
        file = pin(request["files"][name],asset["sha256"])
        if file["bytes"] != asset["bytes"]:
            raise ValueError("incorrect logical asset size")
        if name in prior and prior[name]["asset"] == asset:
            retained.append(prior[name])
        else:
            old_name, new_basis, description = derivations[name]
            if name == "model.onnx" and graph["graph"] != asset["sha256"]:
                raise ValueError("derived graph identity")
            if name == "native-runtime1024-fixture.json":
                fixture = read(file["path"],asset["sha256"])
                if len(fixture["rows"]) != 14 or [r["id"] for r in fixture["rows"]][-2:] != ["synthetic-513","synthetic-1024"]:
                    raise ValueError("unexpected fixture scope")
            rights = prior[old_name]
            changed[name] = {"asset":asset,"license_spdx":rights["license_spdx"],"provenance":description,
                             "license_basis":list(dict.fromkeys(rights["license_basis"]+new_basis))}
    if len(retained) != 5 or changed.keys() != derivations.keys():
        raise ValueError("unexpected new/reused logical assets")
    by_name = {r["asset"]["name"]:r for r in retained} | changed
    for original_pin in roster["license_and_notice_files"]:
        current = pin(original_pin["path"],original_pin["sha256"])
        if current["bytes"] != original_pin["bytes"]:
            raise ValueError("original notice length changed")
    roster["assets"] = [by_name[a["name"]] for a in profile["assets"]]
    roster["assets_sha256"] = execution["binding"]["assets_sha256"]
    roster["license_and_notice_files"].extend(extra)
    return roster


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work",type=Path,required=True)
    args=parser.parse_args()
    work=private_root(args.work.resolve(strict=True))
    value=prepare(work)
    with (work/"rights-roster.raw.json").open("x",encoding="utf8") as out:
        json.dump(value,out,indent=2)
        out.write("\n")
