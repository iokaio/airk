# SPDX-License-Identifier: Apache-2.0
"""Collect original installed CLI commands for one preregistered case in a precreated owned Job directory."""
import argparse
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from installed_techqa_answer import TIMING
import score_techqa_answer_pair as scorer


def collect(freeze, case, role, directory):
    require = scorer.require
    require(freeze["native_issuer"] == "installed_windows_public_cli" and freeze["judgment_sha256"] == scorer.JUDGMENT_SHA
            and freeze["rubric_sha256"] == scorer.RUBRIC_SHA and role in ("fp16","fp32"), "unfrozen installed issuer")
    table = scorer.read({"path":str(scorer.CAMPAIGN/"techqa-normal-publisher-judgment-roster-r1.private.json"),
                         "sha256":scorer.JUDGMENT_SHA})
    cases = [row["case"] for row in table["rows"]]
    require(len(cases) == len(set(cases)) == 75 and cases == freeze["ordered_cases"] and case in cases
            and set(freeze["installed_indexes"]) == set(freeze["installed_request_sha256"]) == set(cases),
            "incomplete frozen roster")
    require(directory.resolve(strict=True) == directory and directory.parent.is_relative_to(scorer.CAMPAIGN)
            and directory.name == "commands" and not any(directory.iterdir()), "fresh guarded command directory required")
    require("managed_install" in freeze, "a Windows campaign collection requires the approved D installation")
    inspection = {}
    image,service_sha = TIMING["installed"](freeze,inspection)
    inspect_path = directory / "runtime-inspect.json"
    TIMING["write"](inspect_path,inspection)
    require(image == Path(freeze["cli"]["path"]) and scorer.digest(image) == freeze["cli"]["sha256"] and
            service_sha == freeze["service"]["sha256"], "foreign installed build")
    control = freeze["installed_control"]
    original = TIMING["CurrentCLI"](image,control,directory)
    pins = []
    def observe(label,args,timeout=120):
        meta = TIMING["CurrentCLI"].call(original,label,args,timeout)
        path = directory / (label + ".command.json")
        pins.append({"path":str(path),"sha256":scorer.digest(path)})
        return meta
    original.call = observe
    def health(label):
        service = original.read(label,["service","status","--runtime-dir",control])["service"]
        require(service["state"] == "running" and service["pending_lifecycle"] is None and not service["explicitly_stopped"]
                and service["health"]["user"] == TIMING["SID"] and service["health"]["executable_sha256"] == service_sha,
                "wrong live installed service")
        return service["health"]["instance"]
    instance = health("service-before")
    exp = freeze[role]
    identity = {"context_manifest":exp["manifest"],"runtime":exp["runtime"],"model":"context_compact",
                "provider":"cuda" if role == "fp16" else "cpu","calibration_sha256":exp["calibration_sha256"]}
    def stage(label):
        return TIMING["obs"]["stages"](original.read(label,["arbiter","status"]),freeze["installed_indexes"].values(),
                                       identity,freeze["focus_manifest"])
    active = stage("stage-before")
    row = table["rows"][cases.index(case)]
    normal = scorer.read({"path":str(scorer.CAMPAIGN/"techqa-normal-source-full-r1/observations"/(case+".json")),
                          "sha256":row["normal_observation_sha256"]})
    index = freeze["installed_indexes"][case]
    require(normal["index_id"] == index and normal["case_id"] == case and normal["visible"] == 50 and normal["omitted"] == 0,
            "foreign native source grant or census")
    menu = {"case":case,"pipeline":freeze["search_pipeline"],"ordered_source_files":[
        {"file":m["path"],"sha256":m["original_sha256"],"bytes":m["original_bytes"]} for m in normal["ordered_members"]]}
    scope = TIMING["source"](original,menu,index)
    require(scope["scope_observation"] == normal["scope"] and scope["publication"] == normal["publication"],
            "wrong native initial publication")
    pin = {"path":freeze["installed_request_paths"][case],"sha256":freeze["installed_request_sha256"][case]}
    request = scorer.read(pin)
    require(request["budget"] == freeze["budget"] and request["response_mode"] == "full" and request["include_status"] is True,
            "changed native opportunity or budget")
    reader = HERE / "audit_techqa_validation_metadata.py"
    require(scorer.digest(reader) == scorer.publisher.READER, "publisher reader changed")
    original_question = scorer.publisher.original_question(scorer.runpy.run_path(str(reader)),case,row["original_menu"])
    require(request["question"] == original_question, "original publisher question differs")
    meta = observe("answer",["tools","call","airk_answer","--index",index,"--args-file",pin["path"]],
                   timeout=request["budget"]["deadline_ms"]/1000)
    require(meta["state"] == "observed" and meta["exit_code"] == 0, "original CLI response unavailable")
    require(stage("stage-after") == active and health("service-after") == instance, "native service or stage changed")
    return {"commands":pins,"request":pin,"runtime_inspect":{"path":str(inspect_path),"sha256":scorer.digest(inspect_path)}}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("freeze","freeze-sha256","case","role","directory"):
        parser.add_argument("--"+name,required=True)
    args = parser.parse_args()
    scorer.require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,"checked Python -I -B -S required")
    manifest = scorer.read({"path":args.freeze,"sha256":args.freeze_sha256})
    print(json.dumps(collect(manifest,args.case,args.role,Path(args.directory)),sort_keys=True))
