# SPDX-License-Identifier: Apache-2.0
"""Authenticate original Windows installed CLI commands and adjudicate a single frozen Answer."""
import json
from pathlib import Path
import runpy

TIMING = runpy.run_path(str(Path(__file__).resolve().parents[1] /
                            "development_margins/installed_answer_timing.py"))
require = TIMING["require"]


class OriginalCommands:
    """Replay signed input files through the same source-census API used by the live CLI."""

    def __init__(self, meta, image, control, read):
        self.meta = iter(meta)
        self.prefix = [str(image), "--output", "json", "--service-runtime-dir", control]
        self.read_pin = read
        self.previous_end = 0
        self.search = None

    def call(self, label, args):
        pin = next(self.meta, None)
        require(pin and Path(pin["path"]).name == label + ".command.json", "missing original CLI invocation")
        path = Path(pin["path"])
        row = self.read_pin(pin)
        require(row["argv"] == self.prefix + args and row["state"] == "observed" and row["exit_code"] == 0
                and type(row["pid"]) is int and row["pid"] > 0 and self.previous_end <= row["start_ns"] < row["end_ns"]
                and row["elapsed_ns"] == row["end_ns"] - row["start_ns"], "foreign or unfinished CLI invocation")
        self.previous_end = row["end_ns"]
        self.last_stdout_sha256 = row["stdout_sha256"]
        require(Path(row["stdout"]) == path.with_name(label + ".stdout"), "unrelated command output")
        return self.read_pin({"path":row["stdout"], "sha256":row["stdout_sha256"]})

    def tool(self, label, name, index, payload):
        args = json.dumps(payload | {"response_mode":"full", "include_status":True})
        result = TIMING["body"](self.call(label, ["tools","call",name,"--index",index,"--args",args]))
        if name == "airk_search":
            self.search = result
        return result


def installed_arm(descriptor, freeze, role, normal, target, source_bytes, question, scorer):
    read = scorer["read"]
    index,case = normal["index_id"],normal["case_id"]
    expected = freeze[role]
    require(freeze["installed_indexes"][case] == index, "wrong original installed index")
    roles = {"fp16":("cuda","float16"), "fp32":("cpu","float32")}
    backend,precision = roles[role]
    device = {"provider":backend,"context_manifest":expected["manifest"],"runtime":expected["runtime"],
              "model":"context_compact","calibration_sha256":expected["calibration_sha256"]}
    profile = read(expected["profile"])
    require(expected["profile"]["sha256"] == expected["profile_file_sha256"] and profile["qualification"]
            and profile["manifest"] == expected["manifest"] and profile["model"] == "arbiter-context-qwen3-17b"
            and profile["calibration"] == expected["calibration_sha256"],"unqualified or foreign installed profile")
    for asset,field in (("model","graph_sha256"),("external_weights","weights_sha256"),("calibration","calibration_sha256")):
        item, = [x for x in profile["assets"] if x["role"] == asset]
        require(item["sha256"] == expected[field], "installed graph, weights or calibration differ")
    calibration = read(expected["calibration"])
    require(expected["calibration"]["sha256"] == expected["calibration_sha256"] and
            calibration["schema"] == "airk.arbiter.context-action.calibration/2" and calibration["rules"] and
            calibration["compiler"] == profile["compiler"] and calibration["model"] == profile["model"]
            and calibration["score_abi"] == profile["abi"] and
            calibration["task_policy"] == profile["task_policies"]["airk.context_action.v1"], "inoperative calibration")
    if role == "fp32":
        require(profile["runtime"] == expected["runtime"] == "onnxruntime-cpu/1.30.0"
                and not profile.get("device_execution") and expected["graph_sha256"] ==
                "c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4" and expected["weights_sha256"] ==
                "7300a9202a4696f023799985131f6c4599ef9caaaa9b0d5b336f1c229574fbb4", "foreign original FP32 arm")
    else:
        require(profile["device_execution"]["precision"] == precision and profile["runtime"] == "onnxruntime-cuda/1.30.0",
                "wrong CUDA precision or provider")
    live_install = {}
    image,service = TIMING["installed"](freeze,live_install)
    install_record = TIMING["install_record"](freeze)
    install = TIMING["pin"](install_record,freeze["install_record_sha256"])
    require(freeze["cli"]["sha256"] == install["executables"]["airkctl"]["sha256"] and
            image == Path(freeze["cli"]["path"]) and service == freeze["service"]["sha256"] and
            (install_record.parent / install["executables"]["airk"]["path"]).resolve() == Path(freeze["service"]["path"]),
            "private fixture or wrong installed pair")
    control = freeze["installed_control"]
    custody = read(descriptor["owned"])
    directory = Path(descriptor["owned"]["path"]).parent
    if "managed_install" in freeze:
        seal = descriptor["runtime_inspect"]
        require(Path(seal["path"]) == directory/"commands/runtime-inspect.json", "foreign original install inspection")
        check = TIMING["managed"]["same_inspection"]
        check(read(seal),live_install)
    require(custody["child_assigned_before_resume"] is True and custody["exit_code"] == 0 and
            custody["owned_descendants_settled"] is True and custody["owned_job_forced_termination"] is False and
            custody["timed_out"] is False and custody["owned_job_final_active_processes"] == 0 and
            custody["owned_job_final_member_pids"] == [], "original installed observation Job did not settle")
    collector = Path(__file__).resolve().with_name("collect_installed_techqa_answer.py")
    argv = custody["argv"]
    freeze_pin = descriptor["freeze"]
    require(read(freeze_pin) == freeze and Path(argv[0]).is_absolute() and
            scorer["digest"](argv[0]) == freeze["collection_python_sha256"] and scorer["digest"](collector) == freeze["collector_sha256"]
            and argv[1:] == ["-I","-B","-S",str(collector),"--freeze",freeze_pin["path"],"--freeze-sha256",
                             freeze_pin["sha256"],"--case",case,"--role",role,"--directory",str(directory/"commands")],
            "owned process was not the original installed collector")
    require(all(Path(pin["path"]).parent == directory / "commands" for pin in descriptor["commands"]),
            "installed observation was assembled from unrelated command roots")
    cli = OriginalCommands(descriptor["commands"],image,control,read)
    def health(label):
        service_row = cli.call(label,["service","status","--runtime-dir",control])["service"]
        health_row = service_row["health"]
        require(service_row["state"] == "running" and not service_row["explicitly_stopped"] and service_row["pending_lifecycle"] is None and
                health_row["user"] == TIMING["SID"] and health_row["executable_sha256"] == service and health_row["instance"],
                "wrong installed service, identity or lease")
        return health_row["instance"]
    def stages(label):
        return TIMING["obs"]["stages"](cli.call(label,["arbiter","status"]),freeze["installed_indexes"].values(),
                                       device,freeze["focus_manifest"])
    instance = health("service-before")
    active = stages("stage-before")
    members = normal["ordered_members"]
    row = {"case":case,"pipeline":freeze["search_pipeline"],"ordered_source_files":[
        {"file":m["path"],"sha256":m["original_sha256"],"bytes":m["original_bytes"]} for m in members]}
    scope = TIMING["source"](cli,row,index)
    require(scope["scope_observation"] == normal["scope"] and scope["publication"] == normal["publication"],
            "installed menu differs from frozen normal source")
    req = descriptor["request"]
    request = read(req)
    require(req["sha256"] == freeze["installed_request_sha256"][case] and request["question"] == question
            and request["budget"] == freeze["budget"] and request["response_mode"] == "full" and
            request["include_status"] is True, "unfrozen installed Answer request")
    packet = TIMING["body"](cli.call("answer",["tools","call","airk_answer","--index",index,"--args-file",req["path"]]))
    response_sha256 = cli.last_stdout_sha256
    require(stages("stage-after") == active and health("service-after") == instance and next(cli.meta,None) is None,
            "installed stage, service instance or command census changed")
    stop = scorer["terminal_packet"](packet)
    observed = TIMING["obs"]["measure_answer"](packet,scope,index,request,freeze["focus_manifest"],device)
    require(all(event["status"] in ("selected","abstained","unavailable") for event in observed["context"]),
            "unexpected installed assessment")
    # Product status and the delivered native work are public observations. The
    # private fixture's internal dispatch journal is unavailable on this route.
    return {"manifest":expected["manifest"],"backend":backend,"precision":precision,
            "trajectory":{"stop":stop,"delivered_native":observed,"request_sha256":req["sha256"],
                          "response_sha256":response_sha256,
                          "instance":instance,"actions":packet["assessment_history"],"record_count":len(packet["records"])},
            "binding":{"request":req["sha256"],"scope":normal["scope"],"publication":normal["publication"],
                       "budget":request["budget"],"search_pipeline":row["pipeline"],"initial_members":scorer["menu_identity"](members),
                       "ordered_search_hits_sha256":scorer["initial_search_identity"](cli.search)},
            "evidence":scorer["coverage"](packet,normal["publication"],members,target,source_bytes)}
