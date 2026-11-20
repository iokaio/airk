# SPDX-License-Identifier: Apache-2.0
"""Measure delivered evidence under the original publisher singleton, for a fully bound pair."""
import argparse
import hashlib
import json
import re
from pathlib import Path
import runpy
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
import prepare_techqa_predispatch_pilot as publisher
from verify_techqa_delivered_evidence import CAMPAIGN, ROOT, returned_interval

JUDGMENT_SHA = "833224dabc575645b5216ffd50e784a026d25c45d2cb89d6a93a076fc647a37f"
RUBRIC_SHA = "3fae6cdf245d9fa0b18f1c4d63e29c135cdfbc873543f08a72f611222022544d"


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream,"sha256").hexdigest()


def read(pin):
    path = Path(pin["path"])
    require(path.is_absolute() and path == path.resolve() and path.is_relative_to(CAMPAIGN)
            and path.stat().st_size <= 256<<20, "invalid private input")
    raw = path.read_bytes()
    require(hashlib.sha256(raw).hexdigest()==pin["sha256"],"input digest mismatch")
    return json.loads(raw)


def bound_body(folder, member):
    name = member["path"]
    require(name == Path(name).name and name.endswith(".txt"),"foreign source path")
    path = folder/name
    require(path.resolve() == path and path.stat().st_size == member["original_bytes"], "original extent changed")
    body = path.read_bytes()
    require(hashlib.sha256(body).hexdigest()==member["original_sha256"], "original source changed")
    return body


def coverage(answer, publication, members, target, body):
    require(isinstance(answer["records"],list) and isinstance(answer["paragraphs"],list),"malformed Answer evidence projection")
    if answer["paragraphs"]:
        record_ids = {record["id"] for record in answer["records"]}
        require(all(isinstance(p["text"],str) and type(p["interpretation"]) is bool and
                    isinstance(p["evidence"],list) and p["evidence"] and set(p["evidence"]) <= record_ids
                    for p in answer["paragraphs"]), "grounded paragraph cites unknown native evidence")
    member_hashes = {m["path"]:m["original_sha256"] for m in members}
    start,end = target["byte_start"],target["byte_end_exclusive"]
    require(0 <= start < end <= len(body) and
            hashlib.sha256(body[start:end]).hexdigest()==target["span_sha256"] and
            hashlib.sha256(body).hexdigest()==member_hashes[target["original_path"]]==target["source_sha256"],
            "publisher range or menu source differs")
    intervals = []
    unknown = 0
    scope = answer["assessment"]["scope"]
    require(scope["publication"] == publication,"Answer publication differs")
    for record in answer["records"]:
        result = record["canonical_result"]
        require(record["scope"]==scope and record["operation"] in ("search","context") and
                result["publication"]==publication and result["scope_observation"]==answer["scope_observation"] and
                isinstance(result["hits"],list), "foreign or unsupported delivered evidence")
        for hit in result["hits"]:
            citation = hit["citation"]
            name = citation["path"]
            require(name in member_hashes and citation["content_hash"]==member_hashes[name],
                    "unfrozen source or delivered revision differs")
            if name == target["original_path"]:
                extent = returned_interval(body,hit)
                if extent is None:
                    unknown += 1
                else:
                    intervals.append({"file":name,"byte_start":extent[0],"byte_end":extent[1]})
    path = ROOT/"tools/retrieval-train/goal_rubric.py"
    require(digest(path)==RUBRIC_SHA,"frozen coverage rule changed")
    goal = {"file":target["original_path"],"byte_start":start,"byte_end":end}
    present = runpy.run_path(str(path))["covered_location"](goal,intervals)
    return {"documented_set_covered":True if present else None if unknown else False,
            "unresolved_delivered_target_hits":unknown,"delivered_target_ranges":intervals}


def menu_identity(members):
    return [[m["publisher_id"],m["original_sha256"]] for m in members]


def initial_search_identity(search):
    hits = search["hits"]
    require(isinstance(hits, list) and all(isinstance(hit, dict) and
            isinstance(hit.get("citation"), dict) and isinstance(hit.get("text"), str)
            for hit in hits), "incomplete initial search opportunity")
    encoded = json.dumps(hits, sort_keys=True, separators=(",", ":"), allow_nan=False)
    return hashlib.sha256(encoded.encode("utf-8")).hexdigest()


def compare(left,right):
    require(left["binding"] == right["binding"],"pair differs in question, ordered scope, input, budget or publication")
    require(left["backend"]=="cuda" and right["backend"] in ("cuda","cpu") and left["precision"]=="float16"
            and right["precision"]=="float32" and left["manifest"]!=right["manifest"],
            "unmatched numerical roles")
    a,b = left["evidence"]["documented_set_covered"],right["evidence"]["documented_set_covered"]
    require(type(a) is bool and type(b) is bool,"unresolved original evidence range")
    return {"fp16":a,"fp32":b,"paired_regression":b and not a,"paired_improvement":a and not b,
            "fp16_trajectory":left["trajectory"],"fp32_trajectory":right["trajectory"]}


def terminal_packet(packet):
    stops = {"covered","clarification_required","no_justified_read","no_progress","budget","deadline","refused","stale","unsupported"}
    require(packet.get("stop") in stops and packet.get("assessment",{}).get("termination")==packet["stop"] and
            isinstance(packet.get("records"),list) and isinstance(packet.get("assessment_history"),list),
            "missing, malformed or nonterminal native Answer")
    return packet["stop"]


def trajectories(records,summary,manifest,log_sha):
    require(len(records)==summary["records"] and summary["last_sequence"]==len(records)
            and [r["sequence"] for r in records]==list(range(1,len(records)+1)), "incomplete native sequence")
    groups = {}
    phases = ["dispatch_admitted","score_returned","job_settled","profile_released","charge_released"]
    bindings = ("observer_id","profile_manifest","scope","task","input_digest","tokens","question_utf8_sha256",
                "original_deadline_unix_ms","charge_identity","charge_bytes")
    for row in records:
        group = groups.setdefault(row["dispatch_identity"],[])
        require(row["profile_manifest"]==manifest and row["task"]=="airk.context_action.v1" and
                len(group)<len(phases) and row["kind"]==phases[len(group)] and
                (not group or all(row[k]==group[0][k] for k in bindings)), "changed or incomplete native dispatch")
        group.append(row)
    work,scopes,attempts = [],[],[]
    questions = set()
    for group in groups.values():
        require(len(group)==5,"native return or settlement missing")
        start,returned,settled,profile,charge = group
        require(settled["return_observed"] is True and returned["return_observed"] is True
                and profile["proof"]==charge["proof"]=="actual_final_reservation_drop_after_native_scorer_and_source_teardown"
                and 0 < start["tokens"] <= 8192 and bool(re.fullmatch("[a-f0-9]{64}",start["input_digest"]))
                and bool(re.fullmatch("[a-f0-9]{64}",start["question_utf8_sha256"])), "uncertain native work or release")
        receipt = returned["work"]
        if receipt is not None:
            require(receipt["input_digest"]==start["input_digest"] and receipt["input_tokens"]==start["tokens"],
                    "native returned work differs from original input")
        if returned["outcome"]=="success":
            require(receipt and returned["finite_raw_scores"] is True and receipt["inference_us"]>0,"incomplete successful return")
            work.append(receipt)
            scopes.append(start["scope"])
        else:
            require(returned["outcome"]=="error" and isinstance(returned["error"],str) and returned["error"],
                    "unrecognized native terminal outcome")
        attempts.append({"input_digest":start["input_digest"],"input_tokens":start["tokens"],"scope":start["scope"],
                         "question":start["question_utf8_sha256"],"outcome":returned["outcome"],"error":returned["error"]})
        questions.add(start["question_utf8_sha256"])
    scores = {"work":work,"scopes":scopes,"compiled_question_sha256":next(iter(questions)) if len(questions)==1 else None,
              "observer_sha256":log_sha}
    return scores,attempts


def raw_fixture(descriptor,run,manifest,plan):
    # A sealed zero-dispatch journal and a real terminal Answer are sufficient
    # even when the older success-only measurement writer produced no summary.
    names = {"ready":"fixture/ready.json","closed":"fixture/closed.json","warm":"fixture/warm-completions.closed.json",
             "warm_log":"fixture/warm-completions.jsonl","observer":"fixture/composed-observer.closed.json",
             "observer_log":"fixture/composed-observer.jsonl","search":"commands/default-search.stdout"}
    for key,leaf in names.items():
        pin = descriptor[key]
        require(Path(pin["path"])==run/leaf and (run/leaf).resolve()==run/leaf and digest(run/leaf)==pin["sha256"],
                "missing or foreign native fixture evidence")
    ready,closed,warm,observer = [read(descriptor[k]) for k in ("ready","closed","warm","observer")]
    rows = (run/names["warm_log"]).read_bytes()
    require(len(rows)==warm["bytes"] and warm["complete"] is True and warm["dropped"]==0 and
            warm["observer_id"]==ready["warm_observer_id"],"incomplete stage activation")
    events = [json.loads(line) for line in rows.splitlines()]
    require(len(events)==warm["records"] and any(e["stage"]=="context" and e["outcome"]=="ready" and
            e["observer_id"]==warm["observer_id"] and e["operation_succeeded"] is True and
            e["active_profile"]==e["profile_manifest"]==manifest for e in events) and
            ready["inputs_sha256"]==plan["inputs"]["sha256"] and any(p["manifest"]==manifest for p in ready["profiles"])
            and closed["shutdown_completed"] is closed["service_lease_reacquired"] is True and
            closed["control_root"]==ready["control_root"],"selected exact stage or original closure missing")
    require(observer["complete"] is True and observer["dropped"]==observer["writer_failed"]==0 and
            (run/names["observer_log"]).stat().st_size==observer["bytes"],"incomplete original native observer")
    require(observer["bytes"]<=64<<20,"observer exceeds original bound")
    records = [json.loads(line) for line in (run/names["observer_log"]).read_bytes().splitlines()]
    scores,attempts = trajectories(records,observer,manifest,descriptor["observer_log"]["sha256"])
    search = read(descriptor["search"])["structuredContent"]
    return scores,attempts,search,closed["instance"]


def arm(descriptor, freeze, role, normal, target, source, question):
    run = Path(descriptor["owned"]["path"]).parent
    require(all(Path(descriptor[k]["path"])==run/leaf for k,leaf in
                (("answer","commands/answer.stdout"),
                 ("command","commands/answer.command.json"),("owned","owned.json"))), "unrelated native outputs")
    answer,command,owned,plan = [read(descriptor[k]) for k in ("answer","command","owned","plan")]
    packet = answer["structuredContent"]
    require(owned["plan_sha256"]==descriptor["plan"]["sha256"] and owned["exit_code"]==0 and
            owned["source_seals_unchanged"] is owned["results_rechecked"] is owned["owned_descendants_settled"] is True and
            owned["owned_job_final_member_pids"]==[] and owned["owned_job_final_active_processes"]==0 and
            owned["owned_job_forced_termination"] is owned["timed_out"] is False, "unsettled native Answer")
    expected = freeze[role]
    backend = plan["backend"]
    require(backend in (("cuda",) if role=="fp16" else ("cpu","cuda")), "unsupported native precision/backend pair")
    manifest,runtime = expected["manifest"],expected["runtime"]
    require(plan["schema"]=="airk.private.paired-compact-answer/1" and plan["manifest"]==manifest
            and plan["service"]==freeze["service"] and plan["cli"]==freeze["cli"],"foreign stage or service")
    scores,attempts,search,instance = raw_fixture(descriptor,run,manifest,plan)
    require(search["semantic_readiness"]["pipeline_digest"]==freeze["search_pipeline"] and
            search["publication"]==normal["publication"] and search["scope_observation"]==normal["scope"],
            "initial search publication or pipeline differs")
    stop = terminal_packet(packet)
    require(("measurement" in descriptor)==(run/"measurement.json").is_file(),"original measurement seal missing or invented")
    if "measurement" in descriptor:
        require(Path(descriptor["measurement"]["path"])==run/"measurement.json","foreign optional measurement")
        m = read(descriptor["measurement"])
        require(m["schema"]=="airk.private.compact-answer-measurement/1" and m["response_sha256"]==descriptor["answer"]["sha256"]
                and m["request_sha256"]==plan["answer"]["sha256"] and m["corpus_sha256"]==plan["corpus"]["sha256"]
                and m["profile_manifest"]==manifest and m["backend"]==backend and m["runtime"]==runtime and m["instance"]==instance
                and m["stop"]==stop and all(m[k]==v for k,v in scores.items()),"optional measurement differs from original delivery")
    require(all(digest(pin["path"])==pin["sha256"] for pin in (plan["service"],plan["cli"])), "foreign binary")
    entry, = [e for e in read(plan["inputs"]) if e["profile_sha256"]==expected["profile_file_sha256"]]
    profile = read({"path":entry["profile"],"sha256":entry["profile_sha256"]})
    require(profile["manifest"]==expected["manifest"] and
            profile["model"]=="arbiter-context-qwen3-17b","different Compact profile")
    for assetrole,field in (("model","graph_sha256"),("external_weights","weights_sha256")):
        asset, = [a for a in profile["assets"] if a["role"]==assetrole]
        require(asset["sha256"]==expected[field],"unapproved graph or weight identity")
    precision = "float16" if role=="fp16" else "float32"
    if backend=="cuda":
        require(profile["runtime"]=="onnxruntime-cuda/1.30.0" and re.fullmatch(re.escape(profile["runtime"])+r"/device-execution/[0-9a-f]{64}",runtime)
                and profile["device_execution"]["precision"]==precision,"registered CUDA precision evidence required")
    else:
        require(runtime==profile["runtime"]=="onnxruntime-cpu/1.30.0" and profile.get("device_execution") is None,
                "wrong original CPU runtime")
    if role=="fp32":
        require(expected["graph_sha256"]=="c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4"
                and expected["weights_sha256"]=="7300a9202a4696f023799985131f6c4599ef9caaaa9b0d5b336f1c229574fbb4","original publisher FP32 graph required")
    request = read(plan["answer"])
    require(type(command["pid"]) is int and command["pid"]>0 and command["exit_code"]==0 and Path(command["argv"][0])==Path(plan["cli"]["path"])
            and command["argv"][-7:]==["tools","call","airk_answer","--index",normal["index_id"],"--args-file",plan["answer"]["path"]]
            and request["question"]==packet["question"]==question and request["response_mode"]=="full"
            and request["include_status"] is True and request["budget"]==freeze["budget"], "different answer input or budget")
    corpus = read(plan["corpus"])["files"]
    members = normal["ordered_members"]
    require([(c["path"],c["sha256"],c["bytes"]) for c in corpus] ==
            [(c["path"],c["original_sha256"],c["original_bytes"]) for c in members],
            "different original source menu, bytes or order")
    require(packet["scope_observation"]==normal["scope"] and packet["assessment"]["scope"]["index_id"]==normal["index_id"]
            and packet["assessment"]["scope"]["publication"]==normal["publication"],"unmatched packet publication")
    unmatched = iter(attempts)
    actions = []
    for event in packet["assessment_history"]:
        result = event["result"]
        require(result["status"] in ("selected","abstained","unavailable") and
                (result["status"]=="unavailable" or isinstance(result.get("advice"),dict)), "invalid delivered assessment")
        actions.append(result)
        advice = result.get("advice")
        if advice is None:
            continue
        row = next((a for a in unmatched if a["input_digest"]==advice["input_digest"]
                    and a["scope"]==advice["scope"] and a["input_tokens"]==advice["observed_input_tokens"]),None)
        require(row is not None and (result["status"]=="unavailable" or row["outcome"]=="success")
                and advice["profile"]==manifest and advice["runtime"]==runtime and
                advice["scope"]==packet["assessment"]["scope"],"delivered action lacks original native return")
    return {"manifest":manifest,"backend":backend,"precision":precision,
            "trajectory":{"stop":stop,"native_attempts":attempts,"actions":actions,"compiled_question":scores["compiled_question_sha256"],
                          "assessment_events":len(packet["assessment_history"]),"record_count":len(packet["records"])},
            "binding":{"request":plan["answer"]["sha256"],"corpus":plan["corpus"]["sha256"],
                       "publication":normal["publication"],"scope":normal["scope"],"budget":request["budget"],
                       "search_pipeline":freeze["search_pipeline"],"initial_members":menu_identity(members),
                       "ordered_search_hits_sha256":initial_search_identity(search)},
            "evidence":coverage(packet,normal["publication"],members,target,source)}


def paired_population(descriptor,freeze):
    require(freeze["native_issuer"]=="private_service_fixture_through_cli",
            "an installed claim needs independently authenticated installed service provenance")
    require(freeze["judgment_sha256"]==JUDGMENT_SHA and freeze["rubric_sha256"]==RUBRIC_SHA
            and freeze["fp16"]["weights_sha256"]!=freeze["fp32"]["weights_sha256"],"foreign frozen evidence or comparison")
    judgment = read({"path":str(CAMPAIGN/"techqa-normal-publisher-judgment-roster-r1.private.json"),"sha256":JUDGMENT_SHA})
    cases = judgment["rows"]
    ids = [r["case"] for r in cases]
    require(len(set(ids))==len(ids)==75 and "DEV_Q002" not in ids and freeze["ordered_cases"]==ids
            and [r["case"] for r in descriptor["pairs"]]==ids and
            all(set(r)=={"case","fp16","fp32"} for r in descriptor["pairs"]), "missing, foreign or unpaired case")
    path = HERE/"audit_techqa_validation_metadata.py"
    require(digest(path)==publisher.READER,"original question reader changed")
    owner = runpy.run_path(str(path))
    results = []
    for truth,inputs in zip(cases,descriptor["pairs"],strict=True):
        case = truth["case"]
        normal = read({"path":str(CAMPAIGN/"techqa-normal-source-full-r1/observations"/(case+".json")),
                       "sha256":truth["normal_observation_sha256"]})
        members = normal["ordered_members"]
        require(normal["case_id"]==case and normal["scope"]==truth["normal_scope"] and normal["publication"]==truth["normal_publication"]
                and normal["visible"]==50 and normal["omitted"]==0 and [m["publisher_id"] for m in members]==truth["original_menu"],
                "normal ordered source menu differs")
        (singleton,), = truth["publisher_acceptable_evidence_sets"]
        member, = [m for m in members if m["path"]==singleton["original_path"]]
        source = bound_body(Path(normal["source_root"]),member)
        question = publisher.original_question(owner,case,truth["original_menu"])
        arms = [arm(inputs[role],freeze,role,normal,singleton,source,question) for role in ("fp16","fp32")]
        results.append({"case":case,"source_component":truth["shared_document_component"],**compare(*arms)})
    count = len(results)
    return {"denominator":count,"fp16_successes":sum(r["fp16"] for r in results),
            "fp32_successes":sum(r["fp32"] for r in results),
            "fp16_rate":sum(r["fp16"] for r in results)/count,"fp32_rate":sum(r["fp32"] for r in results)/count,
            "paired_regressions":sum(r["paired_regression"] for r in results),
            "paired_improvements":sum(r["paired_improvement"] for r in results),"rows":results,
            "native_issuer":"private_service_fixture_through_cli","installed_service_verified":False,
            "interval_computed":False,"release_qualified":False,"judgments":JUDGMENT_SHA,"rubric":RUBRIC_SHA}


def exposed_control():
    from verify_techqa_delivered_evidence import main as original
    previous = original(1024)
    pilot = read({"path":str(CAMPAIGN/"techqa-predispatch-pilot-inputs-r1/pilot.private.json"),
                  "sha256":"d1bc5ae7f75c9a0574eb8c4bba53dcbfa7001927e58c476fb6f4bdd8ed9aadab"})
    raw = read({"path":str(CAMPAIGN/"compact-development-DEV_Q002_EXPOSED-focus1024-run-r1/commands/answer.stdout"),
                "sha256":"82fdd6696a290fea94efa098877c56142e0402a84152998aee265f41b2ec6c75"})["structuredContent"]
    root = Path(pilot["source_root"])
    members = [{"path":doc+".txt","original_sha256":digest(root/(doc+".txt"))} for doc in pilot["publisher_order"]]
    gold = previous["gold"]
    path = gold["file"]
    body = (root/path).read_bytes()
    target = {"original_path":path,"byte_start":gold["byte_start"],"byte_end_exclusive":gold["byte_end"],
              "source_sha256":hashlib.sha256(body).hexdigest(),"span_sha256":hashlib.sha256(body[gold["byte_start"]:gold["byte_end"]]).hexdigest()}
    score = coverage(raw,raw["assessment"]["scope"]["publication"],members,target,body)
    require(score["documented_set_covered"] is previous["publisher_span_covered"],"earlier pilot coverage disagrees")
    return {"exposed_control":"DEV_Q002","arms":1,"context_forwards":0,"paired_cases":0,"evidence":score}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exposed-control",action="store_true")
    for name in ("pairs","pairs-sha256","freeze","freeze-sha256"):
        parser.add_argument("--"+name)
    options = parser.parse_args()
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,"use checked Python -I -B -S")
    pins = [options.pairs,options.pairs_sha256,options.freeze,options.freeze_sha256]
    if options.exposed_control:
        require(not any(pins),"control takes no held-out inputs")
        report = exposed_control()
    else:
        require(all(pins),"original independently pinned preregistration and all paired native receipts required")
        report = paired_population(read({"path":pins[0],"sha256":pins[1]}),read({"path":pins[2],"sha256":pins[3]}))
        report.update(freeze_sha256=pins[3],pairs_sha256=pins[1])
    print(json.dumps(report,sort_keys=True,separators=(",",":")))
