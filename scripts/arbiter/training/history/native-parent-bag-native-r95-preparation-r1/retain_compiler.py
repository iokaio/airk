# SPDX-License-Identifier: Apache-2.0
"""Retain actual Cargo JSON artifacts, never select an image by directory order."""
from pathlib import Path
import argparse,hashlib,json,shutil,subprocess
ROOT=Path(__file__).resolve().parents[4]
REQUIRED=["crates/airk-kernel/src/inference_owner/evaluation_tests/native_parent_bag.rs",
          "crates/airk-kernel/src/inference_owner/evaluation_tests/development.rs"]
def sha(path):
    with Path(path).open("rb") as stream:return hashlib.file_digest(stream,"sha256").hexdigest()
def retain(log,receipt,output):
    if output.exists() or not output.resolve().is_relative_to(ROOT/"artifacts/advanced-split/native-harnesses"):
        raise ValueError("fresh private retained artifact directory required")
    actual=json.loads(receipt.read_bytes())
    if actual.get("exit_code")!=0 or any(actual.get(key) is not True for key in
            ["inputs_unchanged","runner_unchanged","runtime_unchanged","command_inputs_unchanged"]):
        raise ValueError("successful unchanged actual Cargo capture required")
    child=(ROOT/REQUIRED[0]).read_text()
    if any(marker not in child for marker in ["AIRK_PARENT_BAG_PRIVATE_BASE","tempdir_in(&base)",
            "parent_bag_private_base_refuses_missing_relative_file_and_alias_before_native_lease"]):
        raise ValueError("actual mounted private-base child is required")
    for name in REQUIRED:
        if actual["groups"]["native"]["files"].get(name)!=sha(ROOT/name):raise ValueError("compiler source snapshot changed")
    records=[]
    for line in log.read_text(encoding="utf8").splitlines():
        try:row=json.loads(line)
        except ValueError:continue
        records.append(row)
    kernels=[row for row in records if row.get("target",{}).get("name")=="airk_kernel" and row.get("profile",{}).get("test") and row.get("executable")]
    retrieval=[row for row in records if row.get("target",{}).get("name")=="airk_retrieval" and "test-support" in row.get("features",[]) and not row.get("profile",{}).get("test")]
    if len(kernels)!=1 or len(retrieval)!=1:raise ValueError("one actual Kernel test image and consumed Retrieval artifact required")
    binary=Path(kernels[0]["executable"]);kernel_dep=binary.with_suffix(".d")
    deps=kernel_dep.read_text().replace("\\","/")
    if any(name not in deps for name in REQUIRED):raise ValueError("actual dep-info omits required native child/control source")
    output.mkdir(parents=True)
    def copy(source,name):
        expected=sha(source);target=output/name;shutil.copyfile(source,target)
        if sha(target)!=expected or sha(source)!=expected:raise ValueError("artifact changed while retaining")
        return {"path":str(target.resolve()),"sha256":expected}
    lock={"binary":copy(binary,"airk-kernel.exe"),"kernel_depinfo":copy(kernel_dep,"airk-kernel.d"),"artifacts":[]}
    for source in map(Path,retrieval[0]["filenames"]):
        if source.suffix in {".rlib",".rmeta"}:lock["artifacts"].append(copy(source,source.name))
    first=Path(retrieval[0]["filenames"][0]);dep=first.parent/(first.stem.removeprefix("lib")+".d")
    lock["retrieval_depinfo"]=copy(dep,"airk-retrieval.d")
    lock["artifacts"].extend([copy(log,"compile.log"),copy(receipt,"compile.json")])
    ort_env=ROOT/"artifacts/advanced-split/ort130-x64-owned-r5/cargo-aggregate-env.json"
    aggregate=Path(json.loads(ort_env.read_bytes())["environment"]["ORT_LIB_LOCATION"])
    linked=[str(path).removeprefix("native=") for row in records if row.get("reason")=="build-script-executed" for path in row.get("linked_paths",[])]
    if not any(Path(path)==aggregate for path in linked):raise ValueError("actual Cargo JSON omits expected static ORT link search")
    lock["artifacts"].extend({"path":str(path),"sha256":sha(path)} for path in
        [ort_env,aggregate/"inputs.rsp",aggregate/"onnxruntime.lib"])
    names=[("parent_bag_identity_gate_retains_all64_including44_refusals",False),
           ("parent_bag_private_base_refuses_missing_relative_file_and_alias_before_native_lease",False),
           ("emit_parent_bag_epoch1_native_manifests",True),
           ("parent_bag_two_families_match_independent_native_bucket_oracles",True),
           ("parent_bag_dev64_questions_use_current_kernel_with_fixed20_quality_mask",True)]
    listed=[]
    for name,ignored in names:
        selector="inference_owner::evaluation_tests::native_parent_bag::"+name
        argv=[str(output/"airk-kernel.exe"),"--exact",selector,*(["--ignored"] if ignored else []),"--list"]
        listing=subprocess.run(argv,cwd=ROOT,capture_output=True,text=True,check=True,timeout=10)
        if listing.stdout.count(selector+": test")!=1:raise ValueError("one actual compiled selected identity required")
        listed.append({"argv":argv,"stdout":listing.stdout})
    (output/"selectors.json").write_text(json.dumps(listed,indent=2)+"\n")
    lock["artifacts"].append({"path":str(output/"selectors.json"),"sha256":sha(output/"selectors.json")})
    (output/"artifact-lock.json").write_text(json.dumps(lock,indent=2)+"\n")
    print(json.dumps({"artifact_lock":str(output/"artifact-lock.json"),"sha256":sha(output/"artifact-lock.json"),"binary":lock["binary"]}))
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument("--log",type=Path,required=True);parser.add_argument("--receipt",type=Path,required=True);parser.add_argument("--output",type=Path,required=True)
    args=parser.parse_args();retain(args.log,args.receipt,args.output)
if __name__=="__main__":main()
