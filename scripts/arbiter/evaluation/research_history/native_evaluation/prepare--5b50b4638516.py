# SPDX-License-Identifier: Apache-2.0
"""Private feature-selection correction; no public edits, Cargo or NN."""
from pathlib import Path
import difflib,hashlib,json
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]

def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def main():
    focus="crates/airk-kernel/src/arbiter/focus.rs"
    contract="crates/airk-kernel/src/arbiter/CONTRACT.md"
    before={name:(ROOT/name).read_text() for name in [focus,contract]}
    text=before[focus]
    for name in ["observe_optional_history","observe_context_work"]:
        signature="    pub(super) fn "+name
        if text.count(signature)!=1:raise ValueError("exact Context-only method declaration required")
        text=text.replace(signature,'    #[cfg(feature = "local-inference")]\n'+signature)
    signature="    pub(super) fn optional_work_available"
    if text.count(signature)!=1:raise ValueError("exact shared availability accessor required")
    text=text.replace(signature,'    #[cfg(any(feature = "local-inference", test))]\n'+signature)
    addition='''
Context-only history/token accounting mutators compile with `local-inference`,
matching their learned Context actuator callers. The shared availability accessor
also compiles for tests so feature-independent Focus accounting regressions keep
their assertions. A no-default-features production build omits these unused
Context helpers; canonical Focus dispatch, authority and budget behavior do not
change, and no local inference capability is added to the lightweight caller.
'''
    after={focus:text,contract:before[contract]+addition}
    patch=[];files=[]
    for name,text in after.items():
        path=HERE/"source"/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(text)
        patch.extend(difflib.unified_diff(before[name].splitlines(True),text.splitlines(True),fromfile=name,tofile=name))
        files.append({"source":str(path),"target":name,"source_sha256":sha(path),"target_before_sha256":sha(ROOT/name)})
    patch_path=HERE/"focus-context-feature-gates.patch";patch_path.write_text("".join(patch))
    manifest={"schema":"airk.private-kernel-lightweight-focus-feature-gates/1","files":files,
        "patch_sha256":sha(patch_path),"failure_receipt":"artifacts/advanced-split/receipts/final-native-lightweight-r113/lightweight-main.json",
        "rationale":"All three production callers are inside joint.rs's local-inference block. Only optional_work_available has existing unconditional Focus unit-test callers; preserve those unchanged with cfg(any(local-inference,test)).",
        "existing_tests_preserved":["ready_context_retains_capacity_after_focus_reaches_its_cumulative_ceiling","malformed_optional_permutation_is_not_delivered_or_replayed"],
        "no_warning_suppression":True,"wire_digest_or_runtime_behavior_change":False,
        "public_writes":False,"Cargo_or_tests_or_NN_executed":False,
        "mount_constraint":"Root only after active CAL live-source freeze ends"}
    output=HERE/"mount-manifest.json";output.write_text(json.dumps(manifest,indent=2)+"\n")
    print(json.dumps({"manifest":str(output),"sha256":sha(output),"patch_sha256":sha(patch_path),"public_writes":False}))

if __name__=="__main__":main()
