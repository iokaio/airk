# SPDX-License-Identifier: Apache-2.0
"""Prepare review copies only. Never mount source, invoke Cargo or execute NN."""
from pathlib import Path
import difflib
import hashlib
import json

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
FROZEN=HERE.parent/"native-parent-bag-export-source-r1"

def sha(path):
    with Path(path).open("rb") as stream:return hashlib.file_digest(stream,"sha256").hexdigest()

def replace_once(text, old, new):
    if text.count(old)!=1:raise ValueError("nonunique source amendment: "+old[:80])
    return text.replace(old,new)

def main():
    paths=[ROOT/"crates/airk-kernel/src/inference_owner/evaluation_tests/native_parent_bag.rs",
           ROOT/"crates/airk-kernel/src/inference_owner/evaluation_tests/development.rs"]
    before=[path.read_text() for path in paths]
    if before[0].count("tempfile::tempdir()?")!=3:raise ValueError("expected three new child control allocations")
    after=[before[0].replace("tempfile::tempdir()?","private_control()?")+"\n"+(HERE/"private_control.rs.fragment.txt").read_text(),
           replace_once(before[1],"let control = tempfile::tempdir()?;","let control = super::native_parent_bag::private_control()?;")]
    for name in ["emit_parent_bag_epoch1_native_manifests", "parent_bag_two_families_match_independent_native_bucket_oracles",
                 "parent_bag_dev64_questions_use_current_kernel_with_fixed20_quality_mask"]:
        signature="fn "+name+"() -> Result<()> {"
        after[0]=replace_once(after[0],signature,signature+'\n    private_base(std::env::var_os("AIRK_PARENT_BAG_PRIVATE_BASE").as_deref())?;')
    signature="fn native_dev64_questions_use_current_kernel_with_fixed20_quality_mask() -> Result<()> {"
    after[1]=replace_once(after[1],signature,signature+'\n    super::native_parent_bag::private_base(std::env::var_os("AIRK_PARENT_BAG_PRIVATE_BASE").as_deref())?;')
    for path,text in zip(paths,after):
        (HERE/(path.name+".proposed.txt")).write_text(text,encoding="utf8")
    patch="".join("".join(difflib.unified_diff(old.splitlines(True),new.splitlines(True),
            fromfile=str(path.relative_to(ROOT)),tofile=str(path.relative_to(ROOT))))
            for path,old,new in zip(paths,before,after))
    (HERE/"native-private-base.diff").write_text(patch,encoding="utf8")
    original=(FROZEN/"run_native.py").read_text()
    loader='''def load_anchor_support(packet,pins):
    source=Path(__file__).with_name("anchor_runtime.py").resolve()
    if pins.get(str(Path(__file__).resolve()))!=sha(Path(__file__).resolve()) or pins.get(str(source))!=sha(source):
        raise ValueError("reviewed native launcher/anchor source changed")
    spec=importlib.util.spec_from_file_location("_parent_bag_anchor",source)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    module.verified_sources(packet,pins)
    return module

'''
    runner=replace_once(original,"def main():",loader+"def main():")
    runner=replace_once(runner,'    for name,pin in pins.items():\n',
        '''    if packet["limits"]!={"seconds":1800,"host_bytes":8589934592,"max_job_processes":2}:raise ValueError("original native resource limits required")
    anchor_support=load_anchor_support(packet,pins);anchor=anchor_support.anchor_path(packet,a.stage)
    anchor_support.require_fresh(anchor)
    environment["AIRK_PARENT_BAG_PRIVATE_BASE"]=str(anchor)
    for name,pin in pins.items():
''')
    runner=replace_once(runner,'    if not a.execute_reviewed:print(json.dumps({"preflight_passed":True,"argv":argv,"stage_started":False}));return',
        '    if not a.execute_reviewed:print(json.dumps({"preflight_passed":True,"argv":argv,"private_anchor":str(anchor),"anchor_created":False,"stage_started":False}));return')
    runner=replace_once(runner,'    spec=importlib.util.spec_from_file_location("_owned_parent_bag_native",owner);',
        '''    try:
        anchor_before=anchor_support.create(packet,pins,anchor)
    except BaseException as error:
        (work/"execution.json").write_text(json.dumps({"complete":False,"native_started":False,
            "failure":{"type":type(error).__name__,"message":str(error)},"packet_sha256":a.packet_sha256,
            "stage":a.stage,"release_qualified":False},indent=2)+"\\n")
        raise
    (work/"anchor-before.json").write_text(json.dumps(anchor_before,indent=2)+"\\n")
    spec=importlib.util.spec_from_file_location("_owned_parent_bag_native",owner);''')
    runner=replace_once(runner,'        members,active,total=native.job_snapshot(api,job)',
        '''        anchor_support.require_same(anchor_before,anchor_support.observe(packet,pins,anchor))
        members,active,total=native.job_snapshot(api,job)''')
    native_call='    with (work/"stdout.log").open("xb") as log,(work/"stdin.txt").open("rb") as stdin:owned=native.run_windows_owned_training(argv,ROOT,log,1800,env,stdin,8589934592,before_resume=suspended)'
    runner=replace_once(runner,native_call,'''    owned=None;anchor_after=None;failure=None
    try:
        with (work/"stdout.log").open("xb") as log,(work/"stdin.txt").open("rb") as stdin:
            owned=native.run_windows_owned_training(argv,ROOT,log,1800,env,stdin,8589934592,before_resume=suspended)
    except BaseException as error:
        failure={"type":type(error).__name__,"message":str(error)}
    try:
        anchor_after=anchor_support.observe(packet,pins,anchor)
        anchor_support.require_same(anchor_before,anchor_after)
    except BaseException as error:
        failure={"native_error":failure,"anchor_error":{"type":type(error).__name__,"message":str(error)}}
    if failure is not None:
        (work/"execution.json").write_text(json.dumps({"owned":owned,"complete":False,"failure":failure,
            "native_completion_unknown":owned is None,"anchor_before":anchor_before,"anchor_after":anchor_after,
            "packet_sha256":a.packet_sha256,"stage":a.stage,"release_qualified":False},indent=2)+"\\n")
        raise RuntimeError("native stage or actual private-anchor proof failed; retained evidence is not a pass")''')
    runner=replace_once(runner,'"owned":owned,"elapsed_seconds":',
        '"owned":owned,"anchor_before":anchor_before,"anchor_after":anchor_after,"anchor_unchanged":anchor_before==anchor_after,"elapsed_seconds":')
    (HERE/"run_native.py").write_text(runner,encoding="utf8")
    (HERE/"launcher-amendment.diff").write_text("".join(difflib.unified_diff(original.splitlines(True),runner.splitlines(True),fromfile="frozen/run_native.py",tofile="reviewed-private/run_native.py")),encoding="utf8")
    report={"scope":"private review copies only; no public mount/Cargo/NN/anchor creation",
        "public_source_before":{str(path.relative_to(ROOT)):sha(path) for path in paths},
        "frozen_runner_sha256":sha(FROZEN/"run_native.py"),"proposed_runner_sha256":sha(HERE/"run_native.py"),
        "proposed_sources":{path.name:sha(HERE/(path.name+".proposed.txt")) for path in paths}}
    (HERE/"amendment-review.json").write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps(report))

if __name__=="__main__":main()
