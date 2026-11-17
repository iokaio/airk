# SPDX-License-Identifier: Apache-2.0
"""Bind existing Root-archived R95 PE to actual Cargo JSON; no test/NN/Cargo execution."""
from pathlib import Path
import hashlib,json,shutil,subprocess

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
ARCHIVE=HERE.parent/"qwen3-challengers-r1/context-native-owned-r1/build-target-evidence-r1"
EXPECTED_IMAGE="c920c02dace133b209a1d6f37e594a39035c02fa1991e4252ed0229486aac08f"
EXPECTED_CARGO="8c97e95f0bb756885482034c8221b8d85e6e68cb91ecbbe27ddcb77867e3949f"
EXPECTED_BUILD="0b138962d6f21951c3c6be81b607ad26125b9a8a299c43f999797f7a7b97859a"
EXPECTED_CRATES={"airk-core","airk-process","airk-ingest","airk-store","airk-retrieval","airk-kernel"}

def sha(path):
    with Path(path).open("rb") as stream:return hashlib.file_digest(stream,"sha256").hexdigest()

def pinned(path,pin):
    if Path(path).is_symlink() or sha(path)!=pin:raise ValueError("actual archived identity drift: "+str(path))
    return {"path":str(Path(path).resolve()),"sha256":pin}

def cargo_rows(path):
    rows=[]
    for line in path.read_text().splitlines():
        try:row=json.loads(line)
        except ValueError:continue
        rows.append(row)
    return rows

def main():
    out=HERE/"materialized-build-r95"
    if out.exists():raise ValueError("fresh materialized binding required")
    evidence=ARCHIVE/"target-evidence.json"
    record=json.loads(evidence.read_bytes())
    entry=[row for row in record["images"] if row.get("artifact_kind")=="airk_kernel"]
    if record.get("complete") is not True or len(entry)!=1:raise ValueError("one actual Root archive entry required")
    entry=entry[0];image=Path(entry["materialized_image"]["path"])
    cargo=ARCHIVE/"r95-airk-kernel-image-cargo.jsonl";build=ARCHIVE/"r95-airk-kernel-image-receipt.json"
    pinned(image,EXPECTED_IMAGE);pinned(cargo,EXPECTED_CARGO);pinned(build,EXPECTED_BUILD)
    if (entry["image_sha256"]!=EXPECTED_IMAGE or entry["materialized_image"]["sha256"]!=EXPECTED_IMAGE
        or entry["materialized_image"]["bytes"]!=image.stat().st_size
        or entry["materialized_image"].get("independent_copy") is not True
        or entry["cargo_json"]["sha256"]!=EXPECTED_CARGO or entry["build_receipt"]["sha256"]!=EXPECTED_BUILD
        or entry["pe"]["machine_u16"]!=0x8664 or entry["pe"]["optional_header_magic_u16"]!=0x20b):
        raise ValueError("Root materialized archive binding differs")
    with image.open("rb") as stream:
        header=stream.read(64)
        if header[:2]!=b"MZ":raise ValueError("actual PE DOS magic differs")
        stream.seek(int.from_bytes(header[60:64],"little"));pe=stream.read(26)
        if pe[:4]!=b"PE\0\0" or int.from_bytes(pe[4:6],"little")!=0x8664 or int.from_bytes(pe[24:26],"little")!=0x20b:
            raise ValueError("actual archived x64 PE differs")
    receipt=json.loads(build.read_bytes())
    if receipt["exit_code"]!=0 or any(receipt.get(key) is not True for key in
        ["inputs_unchanged","runner_unchanged","runtime_unchanged","command_inputs_unchanged"]):
        raise ValueError("actual complete unchanged R95 compiler receipt required")
    rows=cargo_rows(cargo)
    kernels=[row for row in rows if row.get("reason")=="compiler-artifact" and row.get("target",{}).get("name")=="airk_kernel" and row.get("profile",{}).get("test") and row.get("executable")]
    if len(kernels)!=1 or kernels[0]!=entry["actual_cargo_artifact"] or kernels[0]["executable"]!=entry["materialized_image"]["original_path"]:
        raise ValueError("archive must bind exact original actual Cargo artifact without rewriting Cargo JSON")
    consumed={Path(row["manifest_path"]).parent for row in rows if row.get("reason")=="compiler-artifact"
              and row.get("manifest_path") and Path(row["manifest_path"]).is_relative_to(ROOT/"crates")}
    if {path.name for path in consumed}!=EXPECTED_CRATES:raise ValueError("actual compiled Kernel AIRK closure differs")
    source=receipt["groups"]["native"]["files"]
    prefixes=tuple(path.relative_to(ROOT).as_posix()+"/" for path in consumed)
    selected={name:pin for name,pin in source.items() if name.startswith(prefixes)
              or name in {"Cargo.toml","Cargo.lock","rust-toolchain.toml"} or name.startswith((".cargo/","third_party/"))}
    for name,pin in selected.items():pinned(ROOT/name,pin)
    tracked=subprocess.check_output(["git","-c","safe.directory="+ROOT.as_posix(),"ls-files","-z","--cached","--others","--exclude-standard","--",*[str(path) for path in consumed]],cwd=ROOT)
    actual={str(Path(value.decode()).as_posix()) for value in tracked.split(b"\0") if value}
    expected={name for name in source if name.startswith(prefixes)}
    if actual!=expected:raise ValueError("consumed source additions/removals since R95")
    current_changes=[name for name,pin in source.items() if not (ROOT/name).is_file() or sha(ROOT/name)!=pin]
    if any(name in selected for name in current_changes):raise ValueError("changed consumed R95 source cannot be declared unrelated")
    dep=Path(kernels[0]["executable"]).with_suffix(".d")
    required=["crates/airk-kernel/src/inference_owner/evaluation_tests/native_parent_bag.rs","crates/airk-kernel/src/inference_owner/evaluation_tests/development.rs"]
    if any(name not in dep.read_text().replace("\\","/") for name in required):raise ValueError("actual recorded artifact dep-info omits selected owner")
    retrieval=[row for row in rows if row.get("reason")=="compiler-artifact" and row.get("target",{}).get("name")=="airk_retrieval" and "test-support" in row.get("features",[]) and not row.get("profile",{}).get("test")]
    if len(retrieval)!=1:raise ValueError("one actual consumed Retrieval artifact required")
    ort=ROOT/"artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json"
    aggregate=Path(json.loads(ort.read_bytes())["environment"]["ORT_LIB_LOCATION"])
    links=[str(value).removeprefix("native=") for row in rows if row.get("reason")=="build-script-executed" for value in row.get("linked_paths",[])]
    if not any(Path(path)==aggregate for path in links):raise ValueError("actual R95 Cargo static ORT path differs")
    out.mkdir()
    def copy(path,name):
        pin=sha(path);target=out/name;shutil.copyfile(path,target)
        pinned(target,pin);pinned(path,pin)
        return {"path":str(target),"sha256":pin}
    lock={"binary":pinned(image,EXPECTED_IMAGE),"kernel_depinfo":copy(dep,"kernel-current-materialized.d"),"artifacts":[]}
    first=Path(retrieval[0]["filenames"][0]);rd=first.parent/(first.stem.removeprefix("lib")+".d")
    lock["retrieval_depinfo"]=copy(rd,"retrieval-current-materialized.d")
    for name in retrieval[0]["filenames"]:
        path=Path(name)
        if path.suffix in {".rlib",".rmeta"}:lock["artifacts"].append(copy(path,path.name))
    lock["artifacts"] += [pinned(evidence,sha(evidence)),pinned(cargo,EXPECTED_CARGO),pinned(build,EXPECTED_BUILD)]
    lock["artifacts"] += [pinned(path,sha(path)) for path in [ort,aggregate/"inputs.rsp",aggregate/"onnxruntime.lib"]]
    binding={"schema":"airk.private-parent-bag-materialized-r95-binding/1","actual_R95_archive":entry,
        "actual_compile_receipt":str(build),"actual_Cargo_JSON":str(cargo),"consumed_AIRK_crates":sorted(EXPECTED_CRATES),
        "consumed_source_pins":selected,"current_native_changes_outside_consumed_closure":current_changes,
        "binary_source":"Root-preserved materialized PE, verified against existing Root target evidence; original Cargo JSON executable field is retained unchanged",
        "auxiliary_artifacts":"Dep-info and Retrieval rlib/rmeta copied now from original recorded artifact paths with consumed source equality. Their current hashes were not recorded at R95 link time and are not claimed as such.",
        "Cargo_or_test_or_NN_executed":False,"archive_recompiled":False}
    binding_path=out/"archive-binding.json";binding_path.write_text(json.dumps(binding,indent=2)+"\n")
    lock["artifacts"].append(pinned(binding_path,sha(binding_path)))
    for name,pin in selected.items():lock["artifacts"].append(pinned(ROOT/name,pin))
    (out/"artifact-lock.json").write_text(json.dumps(lock,indent=2)+"\n")
    print(json.dumps({"artifact_lock":str(out/"artifact-lock.json"),"sha256":sha(out/"artifact-lock.json"),
        "binary":lock["binary"],"consumed_crates":sorted(EXPECTED_CRATES),"unrelated_current_changes":len(current_changes),"executed":False}))

if __name__=="__main__":main()
