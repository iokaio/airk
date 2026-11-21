# SPDX-License-Identifier: Apache-2.0
"""Classify every original-source path omitted from the retained native AWS list."""
import argparse
from collections import Counter
import hashlib
import io
import json
from pathlib import Path
import runpy
import zipfile

HERE = Path(__file__).resolve().parent
k = runpy.run_path(str(HERE / "observe_aws_kernel_store.py"))
normal, n, E = k["normal"], k["n"], k["E"]
require = n["require"]
MENU = normal["menu"]
CENSUS = E / "aws-kernel-census-r1"
POLICY = {
    "inventory.rs": "3cc23f83f2a2e3c0da08fdf33e3d95221633ee263a6c06084bb4df13d2786a0d",
    "msbuild.rs": "1c3d6e10cc5c71ebfecd2bf05c28269a418e879f8f048529484d9231b796bac1",
    "documents.rs": "ac4831357cd085f1013679b158198175d33e156d511a0f5c4cc3b76d6b1df2a2",
    "extraction_jobs.rs": "9a463a4a115957461950d8d7cf80fe533662466e992942c9e3a7ee8b94fe709b",
    "adapters/registry.rs": "fa472772a87342e77ddcf91531d7d8c00a4179b6c8c398a90beec2e9d5c01314",
    "syntax.rs": "322eeedb230bbe2f364e929c81e2a7b39f5259bb4294fb2453ee6b720f6fd005",
}
HEADER = "Microsoft Visual Studio Solution File, Format Version "


def source_type(extension, raw):
    if extension == ".sln":
        text = raw.decode("utf-8").removeprefix("\ufeff")
        require(text.startswith("\n" + HEADER), "different MSBuild failure requires review")
        return "visual_studio_solution", "extraction_failed_unsupported_solution_header"
    try:
        raw.decode("utf-8")
    except UnicodeDecodeError:
        pass
    else:
        raise ValueError("unrecognized UTF-8 omission")
    require(b"\x00" in raw[:8192], "binary evidence differs")
    if extension == ".dll":
        offset = int.from_bytes(raw[60:64], "little")
        require(raw.startswith(b"MZ") and raw[offset:offset+4] == b"PE\0\0"
                and int.from_bytes(raw[offset+22:offset+24], "little") & 0x2000, "expected PE DLL")
        kind = "pe_dll"
    elif extension == ".pdb":
        if raw.startswith(b"Microsoft C/C++ MSF"):
            kind = "msf_debug_database"
        else:
            require(raw.startswith(b"BSJB"), "unknown PDB signature")
            kind = "portable_debug_database"
    elif extension == ".nupkg":
        with zipfile.ZipFile(io.BytesIO(raw)) as package:
            require(any(p.endswith(".nuspec") for p in package.namelist()), "expected NuGet package metadata")
        kind = "nuget_package"
    else:
        signatures = {".psd": (b"8BPS\x00\x01", "photoshop_document"),
                      ".jpg": (b"\xff\xd8\xff", "jpeg_image"), ".fbx": (b"Kaydara FBX Binary", "fbx_binary"),
                      ".mp3": (b"\xff\xfb", "mpeg_layer_iii_audio"), ".ico": (b"\x89PNG\r\n\x1a\n", "png_image")}
        prefix, kind = signatures[extension]
        require(raw.startswith(prefix), "source format signature differs")
    return kind, "unsupported_binary_without_document_extractor"


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output", type=Path, required=True)
    args = p.parse_args()
    require(args.output.parent.resolve() == E and args.output.name.startswith("aws-omission-classification-")
            and not args.output.exists(), "fresh campaign receipt required")
    for name, digest in POLICY.items():
        n["checked"](n["ROOT"] / "crates/airk-ingest/src" / name, digest)
    receipt = {
        "native-list.stdout": "ceb470393d5b6cc3de727d2b7b56d7cac3f54fab4bbbc406e32d8015eeb2ef79",
        "native-list.command.json": "f2bfbf69f5683e25daa78e8bac3676fb61a161b3c0ec29d25184e1bb0e439ba0",
        "owned.json": "b289664c6cd0054b56631d00db3d1d6bddb6aff6cdc43ff8825814fb30a42e4d",
        "store-before.json": "76076f4775362f7d5fedf335c72e6ee49dc2873f8565c87c64d08e4149dd285d",
        "store-after.json": "85dbfd84bf5e5d59ce765c88c9306763c16b97b69ca69eedf66102230718b2f4",
    }
    for name, digest in receipt.items():
        n["checked"](CENSUS / name, digest)
    n["checked"](E / MENU["INVENTORY"], MENU["INVENTORY_SHA"])
    manifest = json.loads((E / MENU["INVENTORY"]).read_bytes())
    corpus, cases, sources = MENU["inputs"](manifest)
    pages = [json.loads(line) for line in (CENSUS / "native-list.stdout").read_bytes().splitlines()]
    require(all(page["publication"]["token"] == k["REVISION"] and page["collection"] == "aws-source-discovery"
                and page["truncated"] == (i + 1 < len(pages)) for i, page in enumerate(pages)), "page sequence changed")
    observed = [path for page in pages for path in page["paths"]]
    require(len(observed) == len(set(observed)) == 35400 and observed == sorted(observed)
            and not (set(observed) - sources.keys()), "original native path census changed")
    visible = set(observed)
    rows = []
    for path in sorted(sources.keys() - visible):
        raw = (E / corpus / path).read_bytes()
        digest = hashlib.sha256(raw).hexdigest()
        require(digest == sources[path]["sha256"], "archived source differs")
        kind, eligibility = source_type(Path(path).suffix, raw)
        rows.append({"path": path, "bytes": len(raw), "sha256": digest, "type": kind, "classification": eligibility})
    solutions = [p for p in visible if p.endswith(".sln")]
    require(len(solutions) == 1, "successful solution control differs")
    positive = solutions[0]
    raw = (E / corpus / positive).read_bytes()
    require(hashlib.sha256(raw).hexdigest() == sources[positive]["sha256"] and
            raw.decode("utf-8").removeprefix("\ufeff").startswith(HEADER), "visible MSBuild source differs")
    gold = [c["source_path"].removeprefix("documents/") for c in cases[0]["cases"]]
    require(len(gold) == 41 and len(set(gold)) == 27 and set(gold) <= visible, "positive visibility differs")
    result = {"revision": k["REVISION"], "source_pack": MENU["PACK_SHA"], "rights": MENU["RIGHTS_SHA"],
              "source_roster": len(sources), "native_listed": len(visible), "native_pages": len(pages), "missing": rows,
              "summary": dict(Counter(row["classification"] for row in rows)), "msbuild_control": positive,
              "cases_with_visible_positive": len(gold), "unique_positive_paths": len(set(gold)),
              "ambiguous_cases_excluded": 24, "r1_receipts": receipt, "policy_sources": POLICY,
              "basis": "source bytes and version-matched ingest rules; native listing checksum, per-file statuses inferred",
              "r1_aggregate_custody_passed": False, "service_access": False, "scored_cases": 0}
    with args.output.open("x", encoding="utf-8", newline="\n") as output:
        json.dump(result, output, sort_keys=True, indent=2)
        output.write("\n")


if __name__ == "__main__":
    main()
