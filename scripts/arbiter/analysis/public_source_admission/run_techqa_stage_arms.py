# SPDX-License-Identifier: Apache-2.0
"""Freeze and run the two TechQA stage arms: retrieval alone, then retrieval with Focus.

``--prepare`` seals one request per frozen case, the enrollment scopes and the
measurement rule before any service starts. Run mode owns one private Windows
fixture lifetime for one frozen plan. It indexes that plan's 50-document menus,
sends each sealed request with both optional stages disabled (arm A), records
the model-free Focus pair census, then enables the static 1,024-token Focus
profile on CPU and sends the identical request again (arm B). Context stays
disabled and no device configuration is sent. A fixture catalog holds at most
64 indexes, so the 76 cases are split across two lifetimes of 38.
"""
import argparse
import ctypes
import io
import json
import os
from pathlib import Path
import re
import runpy
import shutil
import subprocess
import sys
import tarfile
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import prepare_techqa_predispatch_pilot as publisher
import run_techqa_predispatch_pilot as pilot

ROOT, CAMPAIGN = pilot.ROOT, pilot.CAMPAIGN
require, pin, read = pilot.require, pilot.pin, pilot.read
SCHEMA = "airk.private.techqa-stage-arms-freeze/1"
EXPOSED = "DEV_Q002"
OWNER_VARIABLE = "AIRK_TECHQA_STAGE_ARMS_OWNER"
PILOT_SHA = "240084bdb6affb292ad8926e4196776f6c9e0823ec9ad068dc64bb3a5fb8ca6d"
GOLD_SHA = "a670e02fe6abdaab184a6950c33d74caf4ef4b972530a1aff300d68dd51b44c8"
ROSTER_SHA = "833224dabc575645b5216ffd50e784a026d25c45d2cb89d6a93a076fc647a37f"
SERVICE_SHA = "77e70341e1da84688c8b16bc992e25b3afed32d2d3e07639a6a5c69176eb0dfa"
CLI_SHA = "8aef48b0574e0876de80f43ff11b802809ba91c7e89bec3da39ccd0cb4810922"
PROFILES_SHA = "8fbde10f3fc8a42d1d05ec0f362a1e0527840ad49fa6686224f1feee4b08373d"
FOCUS = "94c65d0c598b0b110cd25ddeb1c167ae730430197c0d5e6a59d79ee226c17c64"
FOCUS_TASK = "airk.focus_rank.v1"
BUDGET = {"calls": 8, "candidates": 32, "deadline_ms": 120000, "fanout": 8, "input_tokens": 8192,
          "max_bytes": 262144, "rounds": 3}
PROBE = {"query": "source", "limit": 1, "max_bytes": 4096, "full_chunks": True, "response_mode": "full",
         "include_status": True}
ENVIRONMENT = {
    "AIRK_ARBITER_EVALUATION_RESOURCE_CONSENT": "throughput64gib4threads",
    "AIRK_SELECTED_CONTEXT_CAL_OBSERVER": "false",
    "AIRK_NATIVE_DISPATCH_OBSERVER": "true",
    "AIRK_NATIVE_WARM_OBSERVER": "true",
    "AIRK_MCP_RESPONSE_MODE": "full",
}
SECONDS = {"run": 7200, "dry": 1800}
BATCH = 38
# The native configuration emitter reads the whole fixture catalog and refuses
# more than 64 registered indexes, so one fixture lifetime holds at most 64 cases.
CATALOG_INDEXES = 64
FLOOR_BYTES = 100 << 30
RESERVATION_BYTES = 30 << 30
OVERLAY_BYTES = (4 << 30) + (16 << 20)
BUILD_IMAGES = ("cargo.exe", "rustc.exe", "clippy-driver.exe", "rust-lld.exe", "link.exe", "cl.exe")
# Helpers this harness imports or replays. Their digests are sealed in the
# freeze and must be unchanged when a run starts and when it is scored.
SEALED = (
    "scripts/arbiter/analysis/public_source_admission/prepare_techqa_predispatch_pilot.py",
    "scripts/arbiter/analysis/public_source_admission/run_techqa_predispatch_pilot.py",
    "scripts/arbiter/analysis/public_source_admission/audit_techqa_validation_metadata.py",
    "scripts/arbiter/analysis/public_source_admission/score_techqa_answer_pair.py",
    "scripts/arbiter/analysis/public_source_admission/verify_techqa_delivered_evidence.py",
    "scripts/arbiter/analysis/public_source_admission/diagnose_techqa_focus_pair.py",
    "scripts/arbiter/evaluation/run_compact_answer.py",
    "scripts/arbiter/export/qwen/qwen3-reranker06-experiment-r1/focus_reference.py",
    "scripts/provision-static-onnx-runtime.py",
    "tools/retrieval-train/goal_rubric.py",
)
HARNESS = ("run_techqa_stage_arms.py", "score_techqa_stage_arms.py")
# Frozen before arm B is scored. There is no pass line.
RULE = {
    "pass_line": None,
    "statement": "No pass line. Report the paired counts; the user reviews the numbers.",
    "categories": ["both", "a_only", "b_only", "neither", "unresolved", "error"],
    "net_effect": "b_only minus a_only",
    "populations": {
        "by_case": "the 75 publisher-judged cases",
        "by_group": "their 32 document-linked groups",
        "exposed": "DEV_Q002 and its single-case group, reported separately because it informed the Focus width",
    },
    "denominator": ("Every case stays in its population: refusals, deadlines, errors and cases where Focus "
                    "was not applied or failed are counted, never dropped."),
    "arm_coverage": {
        "covered": "the union of delivered byte intervals contains the whole publisher span",
        "partial": "some but not all span bytes were delivered",
        "not_delivered": "no span byte was delivered",
        "unresolved": ("a delivered hit in the publisher document could not be located, or the delivered "
                       "evidence could not be checked against the frozen sources"),
        "cli_error, tool_error, timeout": "the call produced no terminal Answer",
    },
    "category_precedence": [
        "error when either arm has no terminal Answer",
        "unresolved when either arm is unresolved or the arms did not start from the same initial search",
        "otherwise both, a_only, b_only or neither by whether each arm is covered",
    ],
    "terminal_stops": ("A terminal refusal or deadline is scored on the evidence it delivered and its stop "
                       "is reported separately."),
    "pairing": ("Both arms use the same sealed request, index and publication. The initial searches must "
                "hold the same hits and arm B's order before Focus must equal arm A's order."),
    "group_report": ("Each group lists its cases by category and its net effect. Totals give every group "
                     "equal weight and count the groups with a positive, zero and negative net effect."),
    "latency": "median and nearest-rank 95th percentile of the Answer round trip per arm; indicative only",
}
LIMITS = [
    "private evaluation fixture, not the installed product",
    "one service image and CLI, pinned under runtime",
    "four inference threads on one Windows host",
    "unqualified static 1,024-token Focus profile",
    "one publisher span per case; other valid evidence is unjudged",
]


def seals():
    return {name: pin(ROOT / name) for name in SEALED}


def harness():
    return {name: pin(HERE / name) for name in HARNESS}


def bound(path):
    return {"path": str(path), "sha256": pin(path)}


def plan_name(output):
    """The frozen plan a private target name selects: ``dry`` or one fixture lifetime ``run-N``.

    A bare ``run`` names the retired single-lifetime plan of an earlier freeze. It
    stays readable so its closed run can be scored and disposed; it cannot launch.
    """
    match = re.fullmatch(r"techqa-stage-arms-(dry|run(?:-[1-9][0-9]*)?)-r[1-9][0-9]*", output.name)
    require(output.parent == CAMPAIGN and match is not None, "new private stage-arm target required")
    return match.group(1)


def catalog_bound(cases):
    require(0 < len(cases) <= CATALOG_INDEXES and len(set(cases)) == len(cases),
            "one fixture lifetime holds at most 64 indexes; split the cases across lifetimes")
    return cases


def document_groups(menus):
    """Connect cases transitively through shared exact document identifiers."""
    parent = {}

    def find(key):
        parent.setdefault(key, key)
        while parent[key] != key:
            parent[key] = parent[parent[key]]
            key = parent[key]
        return key

    for case, documents in menus.items():
        for document in documents:
            parent[find(("document", document))] = find(("case", case))
    groups = {}
    for case in sorted(menus):
        groups.setdefault(find(("case", case)), []).append(case)
    return sorted(groups.values())


def batches(cases, size=BATCH):
    """Ordered enrollment batches. The native emitter accepts at most 64 roots."""
    ordered = sorted(cases)
    require(ordered and len(set(ordered)) == len(ordered) and 0 < size <= 64, "invalid enrollment batch")
    return [{"name": f"b{number + 1}", "cases": ordered[start:start + size]}
            for number, start in enumerate(range(0, len(ordered), size))]


def request_bytes(template, question):
    require(template["schema"] == "airk.arbiter.context.request/2" and template["budget"] == BUDGET
            and template["response_mode"] == "full" and template["include_status"] is True,
            "native Answer template differs")
    require(isinstance(question, str) and question.strip(), "empty publisher question")
    return publisher.json_bytes(dict(template, question=question))


def build_processes(listing):
    """Count compiler images in a ``tasklist /FO CSV /NH`` listing."""
    counts = {}
    for line in listing.splitlines():
        name = line.split('","', 1)[0].strip('"').lower()
        if name in BUILD_IMAGES:
            counts[name] = counts.get(name, 0) + 1
    return counts


def gpu_process_ids(listing):
    """PIDs from ``nvidia-smi --query-compute-apps=pid,process_name,used_memory --format=csv,noheader``."""
    fields = (row.split(",", 1)[0].strip() for row in listing.splitlines())
    return sorted({int(field) for field in fields if field.isdigit()})


def answer_call(run, folder, label, command):
    """Issue one Answer. A CLI failure, tool error or client timeout is a recorded state, not a retry."""
    try:
        stdout, elapsed = run(command, folder, label)
    except TimeoutError:
        return {"state": "timeout"}
    except ValueError:
        if not (folder / f"{label}.command.json").is_file():
            raise
        return {"state": "cli_error"}
    try:
        failed = read(stdout).get("isError") is True
    except (ValueError, AttributeError):
        failed = True
    return {"state": "tool_error" if failed else "ok", "elapsed_ns": elapsed}


def retained(folder, label):
    files = {"command": folder / f"{label}.command.json", "stdout": folder / f"{label}.stdout",
             "stderr": folder / f"{label}.stderr"}
    return {kind: bound(path) for kind, path in files.items() if path.is_file()}


def memory_headroom():
    class Status(ctypes.Structure):
        _fields_ = [("length", ctypes.c_uint32), ("load", ctypes.c_uint32)] + [
            (name, ctypes.c_uint64) for name in ("total_physical", "available_physical", "total_commit",
                                                 "available_commit", "total_virtual", "available_virtual",
                                                 "extended")]

    status = Status()
    status.length = ctypes.sizeof(status)
    if not ctypes.WinDLL("kernel32", use_last_error=True).GlobalMemoryStatusEx(ctypes.byref(status)):
        raise ctypes.WinError(ctypes.get_last_error())
    return {"available_physical_bytes": status.available_physical,
            "available_commit_bytes": status.available_commit}


def admission(freeze, cap, parent):
    """Disk and memory admission for one fresh three-profile installation."""
    copies = sum(asset["bytes"] for profile in freeze["runtime"]["profiles"] for asset in profile["assets"])
    report = {"independent_asset_bytes": copies, "native_overlay_bytes": OVERLAY_BYTES,
              "reservation_bytes": RESERVATION_BYTES, "required_free_floor_bytes": FLOOR_BYTES,
              "available_bytes": shutil.disk_usage(parent).free, "job_memory_cap_bytes": cap,
              **memory_headroom()}
    require(copies + OVERLAY_BYTES <= RESERVATION_BYTES
            and report["available_bytes"] >= FLOOR_BYTES + RESERVATION_BYTES,
            "insufficient space for the reserved installation above the free-space floor")
    require(min(report["available_physical_bytes"], report["available_commit_bytes"]) >= cap,
            "insufficient memory headroom for the Job cap")
    return report


def dispose_weights(output, freeze):
    """Remove a settled run's installed duplicate weights after authenticating the retained originals."""
    owned, receipt = read(output / "owned.json"), output / "weights-disposed.json"
    require(owned["owned_descendants_settled"] is True and owned["owned_job_final_active_processes"] == 0
            and owned["owned_job_final_member_pids"] == [] and not receipt.exists(),
            "only a settled run whose weights are still installed can be disposed")
    before, rows = shutil.disk_usage(output).free, []
    for profile in freeze["runtime"]["profiles"]:
        weights, = [asset for asset in profile["assets"] if asset["role"] == "external_weights"]
        original = Path(profile["assets_directory"]) / weights["name"]
        installed = output / "fixture/control/arbiter-v3/assets" / profile["manifest"] / weights["name"]
        require(not original.is_relative_to(output) and original.stat().st_size == weights["bytes"]
                and pin(original) == weights["sha256"], "retained original weights differ from the roster profile")
        rows.append((installed, original, weights))
    # Every original and every installed file is authenticated before the first
    # removal. An installed file must be an independent, byte-identical copy.
    for installed, _, weights in rows:
        require(installed.stat().st_nlink == 1 and installed.stat().st_size == weights["bytes"]
                and pin(installed) == weights["sha256"], "installed weights are not the verified duplicate")
    removed = []
    for installed, original, weights in rows:
        installed.unlink()
        removed.append({"removed": str(installed), "retained_original": str(original), "bytes": weights["bytes"],
                        "sha256": weights["sha256"]})
    result = {"schema": "airk.private.techqa-stage-arms-weights-disposed/1", "run": str(output), "files": removed,
              "reclaimed_bytes": sum(row["bytes"] for row in removed), "free_bytes_before": before,
              "free_bytes_after": shutil.disk_usage(output).free}
    with receipt.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(result, stream, indent=2, sort_keys=True)
        stream.write("\n")
    return result


def dev_member(owner):
    """Read the pinned publisher DEV member once."""
    started = time.monotonic()

    def deadline():
        require(time.monotonic() - started < 600, "original source read expired")

    archive = owner["ARCHIVE"]
    publisher.check_file(archive, owner["ARCHIVE_SHA"])
    with archive.open("rb") as stream:
        with tarfile.open(fileobj=owner["DeadlineStream"](stream, deadline), mode="r|gz") as tar:
            for member in tar:
                if member.name != publisher.DEV:
                    continue
                require(member.isfile() and 0 < member.size <= owner["MAX_MEMBER_BYTES"], "invalid DEV member")
                with tar.extractfile(member) as data:
                    raw = data.read(member.size + 1)
                require(len(raw) == member.size and publisher.sha(raw) == publisher.DEV_SHA,
                        "publisher DEV member differs")
                return raw, deadline
    raise ValueError("publisher DEV member missing")


def source_closure(root, ordered, documents):
    expected = {key + ".txt": documents[key]["sha256"] for key in ordered}
    require(len(expected) == len(ordered) == 50 and {entry.name for entry in root.iterdir()} == expected.keys(),
            "source closure differs: " + root.name)
    for name, digest in expected.items():
        pin(root / name, digest)


def runtime_pins(descriptor):
    """Service, CLI, the three-profile roster and the model-free Focus and MiniLM inputs."""
    service = CAMPAIGN / "cal100-fused-candidate-launch-r1/airk-service-fused-search.exe"
    roster = CAMPAIGN / "cal100-focus1024-launch-r1/inputs.json"
    pin(service, SERVICE_SHA)
    pin(roster, PROFILES_SHA)
    cli = descriptor["runtime_inputs"]["cli"]
    minilm = descriptor["runtime_inputs"]["minilm"]
    require(cli["sha256"] == CLI_SHA and Path(minilm["path"]).name == "model.json", "CLI or MiniLM pin differs")
    for asset in (cli, minilm):
        pin(Path(asset["path"]), asset["sha256"])
    for part in descriptor["minilm_assets"]:
        pin(Path(minilm["path"]).parent / part["name"], part["sha256"])
    profiles = []
    for entry in read(roster):
        pin(Path(entry["profile"]), entry["profile_sha256"])
        profile = read(Path(entry["profile"]))
        require(profile["qualification"] is None and profile["runtime"] == "onnxruntime-cpu/1.30.0",
                "roster profile is not an unqualified CPU profile")
        profiles.append({"manifest": profile["manifest"], "model": profile["model"],
                         "profile": {"path": entry["profile"], "sha256": entry["profile_sha256"]},
                         "assets_directory": entry["assets"],
                         "assets": [{key: asset[key] for key in ("name", "role", "bytes", "sha256")}
                                    for asset in profile["assets"]]})
    focus, = [profile for profile in profiles if profile["manifest"] == FOCUS]
    selected = read(Path(focus["profile"]["path"]))
    require(selected["max_tokens"] == 1024 and selected["max_candidates"] == 4
            and selected["cpu_fit"]["intra_threads"] == 4 and selected["tasks"] == [FOCUS_TASK],
            "Focus profile limits differ")
    parts = {asset["role"]: asset for asset in focus["assets"]}
    files = {role: Path(focus["assets_directory"]) / parts[role]["name"] for role in ("tokenizer", "compiler")}
    for role, path in files.items():
        pin(path, parts[role]["sha256"])
    return {
        "service": {"path": str(service), "sha256": SERVICE_SHA, "bytes": service.stat().st_size},
        "cli": cli, "profiles_roster": {"path": str(roster), "sha256": PROFILES_SHA}, "profiles": profiles,
        "focus_manifest": FOCUS, "focus_max_tokens": selected["max_tokens"],
        "focus_max_candidates": selected["max_candidates"],
        "focus_tokenizer": {"path": str(files["tokenizer"]), "sha256": parts["tokenizer"]["sha256"]},
        "focus_compiler": {"path": str(files["compiler"]), "sha256": parts["compiler"]["sha256"]},
        "minilm": minilm, "minilm_assets": descriptor["minilm_assets"],
        "minilm_pipeline": descriptor["required_minilm_pipeline"], "native_selector": publisher.SELECTOR,
        "image_scope": "Results are scoped to this service image and CLI; no Cargo build was made for them.",
    }


def prepare(destination):
    require(destination.parent == CAMPAIGN and re.fullmatch(r"techqa-stage-arms-inputs-r[1-9][0-9]*", destination.name)
            and destination.resolve(strict=True) == destination and not any(destination.iterdir()),
            "fresh checked stage-arm input destination required")
    sources = seals()
    full = CAMPAIGN / "techqa-full-menu-corpus-r1/manifest.private.json"
    gold_path = CAMPAIGN / "techqa-exact-gold-source-corpus-r1/manifest.private.json"
    roster_path = CAMPAIGN / "techqa-normal-publisher-judgment-roster-r1.private.json"
    for path, expected in ((full, publisher.MENU), (gold_path, GOLD_SHA), (roster_path, ROSTER_SHA)):
        publisher.check_file(path, expected)
    menu, gold, roster = (json.loads(path.read_bytes()) for path in (full, gold_path, roster_path))
    menus, documents = menu["ordered_menus"], menu["documents"]
    judged = {row["case"]: row for row in roster["rows"]}
    spans = {case["selector"]["question_id"]: case for case in gold["cases"]}
    cases = sorted(menus)
    require(len(cases) == 76 and len(judged) == len(roster["rows"]) == 75 and EXPOSED not in judged
            and set(judged) | {EXPOSED} == set(cases) == set(spans) and menu["pilot_case"] == EXPOSED
            and roster["excluded_exposed_singleton"] == EXPOSED, "frozen TechQA population differs")
    menu_freeze = CAMPAIGN / "techqa-original-dev-ordered-menu-freeze-r1.json"
    require(gold["ordered_menu_freeze_sha256"] == menu["ordered_menu_freeze_sha256"], "menu freeze lineage differs")
    publisher.check_file(menu_freeze, menu["ordered_menu_freeze_sha256"])
    pin(pilot.INPUT / "pilot.private.json", pilot.DESCRIPTOR)
    descriptor = read(pilot.INPUT / "pilot.private.json")
    require(descriptor["question_id"] == EXPOSED and descriptor["original_answer_budget"] == BUDGET
            and descriptor["question_framing"] == "title_double_newline_text"
            and descriptor["full_menu_manifest_sha256"] == publisher.MENU
            and descriptor["required_minilm_pipeline"] == publisher.PIPELINE, "sealed pilot descriptor differs")
    template_pin = descriptor["runtime_inputs"]["answer"]
    publisher.check_file(Path(template_pin["path"]), template_pin["sha256"])
    template = read(Path(template_pin["path"]))
    runtime = runtime_pins(descriptor)

    groups = document_groups(menus)
    names = {case: f"G{number + 1:02}" for number, group in enumerate(groups) for case in group}
    components = {tuple(sorted(row["shared_document_component"])) for row in judged.values()}
    require(len(groups) == 33 and [EXPOSED] in groups
            and {tuple(group) for group in groups if group != [EXPOSED]} == components,
            "document-linked groups differ from the judgment roster")

    reader_path = HERE / "audit_techqa_validation_metadata.py"
    publisher.check_file(reader_path, publisher.READER)
    owner = runpy.run_path(str(reader_path))
    raw, deadline = dev_member(owner)
    requests = destination / "requests"
    requests.mkdir()
    roots, rows = {}, []
    for case in cases:
        exposed = case == EXPOSED
        fixture = CAMPAIGN / ("techqa-normal-source-pilot-r2" if exposed else "techqa-normal-source-full-r1")
        root, observed = fixture / "sources" / case, fixture / "observations" / (case + ".json")
        source_closure(root, menus[case], documents)
        observation_sha = pin(observed, None if exposed else judged[case]["normal_observation_sha256"])
        observation = read(observed)
        members = observation["ordered_members"]
        require(observation["case_id"] == case and Path(observation["source_root"]) == root
                and observation["visible"] == 50 and observation["omitted"] == 0
                and [(member["publisher_id"], member["path"], member["original_sha256"]) for member in members]
                == [(key, key + ".txt", documents[key]["sha256"]) for key in menus[case]]
                and (exposed or judged[case]["original_menu"] == menus[case]),
                "normal source observation differs from the frozen menu")
        if exposed:
            span, document = spans[case]["source_span"], spans[case]["document"]
            target = {"original_path": document["path"], "publisher_id": spans[case]["publisher_document_id"],
                      "byte_start": span["byte_start"], "byte_end_exclusive": span["byte_end_exclusive"],
                      "span_sha256": span["span_sha256"], "source_sha256": document["sha256"]}
        else:
            (target,), = judged[case]["publisher_acceptable_evidence_sets"]
        body = (root / target["original_path"]).read_bytes()
        start, end = target["byte_start"], target["byte_end_exclusive"]
        require(target["original_path"] == target["publisher_id"] + ".txt" and target["publisher_id"] in menus[case]
                and publisher.sha(body) == target["source_sha256"] and 0 <= start < end <= len(body)
                and publisher.sha(body[start:end]) == target["span_sha256"], "publisher span differs from its source")
        reader = owner["MetadataReader"](io.BytesIO(raw), deadline)
        question = publisher.selected_question(reader, raw, case, menus[case])
        require(reader.finish(len(raw))["sha256"] == publisher.DEV_SHA, "publisher parser did not consume exact member")
        encoded = request_bytes(template, question)
        with (requests / (case + ".json")).open("xb") as stream:
            stream.write(encoded)
        roots[case] = root
        rows.append({"case": case, "exposed": exposed, "group": names[case], "source_root": str(root),
                     "observation": {"path": str(observed), "sha256": observation_sha},
                     "request": {"path": str(requests / (case + ".json")), "sha256": publisher.sha(encoded)},
                     "question_sha256": publisher.sha(question.encode()),
                     "target": {key: target[key] for key in ("original_path", "publisher_id", "byte_start",
                                                             "byte_end_exclusive", "span_sha256", "source_sha256")}})
    by_case = {row["case"]: row for row in rows}
    require(by_case[EXPOSED]["request"]["sha256"] == descriptor["private_files"]["request.private.json"]
            and by_case[EXPOSED]["question_sha256"] == descriptor["question_sha256"],
            "reconstructed exposed request differs from its sealed pilot request")

    def scope(name, members):
        data = publisher.json_bytes({"source_roots": [str(roots[case]) for case in members]})
        path = destination / f"scopes-{name}.private.json"
        with path.open("xb") as stream:
            stream.write(data)
        return {"path": str(path), "sha256": publisher.sha(data)}

    full_plan = [dict(batch, scope=scope(batch["name"], catalog_bound(batch["cases"]))) for batch in batches(cases)]
    dry = scope("dry", [EXPOSED])
    freeze = {
        "schema": SCHEMA,
        "role": "TechQA stage comparison: retrieval alone (arm A) against retrieval with Focus (arm B)",
        "arms": {"A": {"focus": False, "context": False},
                 "B": {"focus": True, "context": False, "focus_profile": FOCUS, "device": "CPU; none configured"}},
        "inputs": {"menu_manifest": {"path": str(full), "sha256": publisher.MENU},
                   "gold_manifest": {"path": str(gold_path), "sha256": GOLD_SHA},
                   "judgment_roster": {"path": str(roster_path), "sha256": ROSTER_SHA},
                   "ordered_menu_freeze": {"path": str(menu_freeze), "sha256": menu["ordered_menu_freeze_sha256"]},
                   "pilot_descriptor": {"path": str(pilot.INPUT / "pilot.private.json"), "sha256": pilot.DESCRIPTOR},
                   "archive_sha256": owner["ARCHIVE_SHA"], "dev_member_sha256": publisher.DEV_SHA,
                   "answer_template": template_pin},
        "runtime": runtime, "environment": ENVIRONMENT, "budget": BUDGET,
        "question_framing": descriptor["question_framing"], "sources": sources, "harness_at_freeze": harness(),
        "cases": rows, "groups": [{"group": names[group[0]], "cases": group, "exposed": group == [EXPOSED]}
                                  for group in groups],
        # One plan per fixture lifetime, each inside the 64-index catalog bound. Both arms of
        # a case run in the same lifetime. The dry plan repeats the enable, disable and enable
        # cycle on the exposed case only.
        "plans": {**{f"run-{number + 1}": {"seconds": SECONDS["run"], "batches": [batch]}
                     for number, batch in enumerate(full_plan)},
                  "dry": {"seconds": SECONDS["dry"], "batches": [{"name": "b1", "cases": [EXPOSED], "scope": dry},
                                                                 {"name": "b2", "cases": [], "scope": dry}]}},
        "measurement_rule": RULE, "evidence_limits": LIMITS, "judged": False, "release_qualified": False,
    }
    encoded = publisher.json_bytes(freeze)
    with (destination / "freeze.private.json").open("xb") as stream:
        stream.write(encoded)
    print(json.dumps({"cases": len(rows), "judged_cases": len(judged), "groups": len(groups),
                      "batches": [len(batch["cases"]) for batch in full_plan],
                      "freeze_sha256": publisher.sha(encoded)}))


def validate(freeze):
    """Re-pin every frozen input. Raises when anything changed since the freeze."""
    require(freeze["schema"] == SCHEMA and freeze["sources"] == seals() and freeze["budget"] == BUDGET
            and freeze["environment"] == ENVIRONMENT and freeze["measurement_rule"]["pass_line"] is None,
            "freeze, sealed helper or measurement rule differs")
    for name in ("menu_manifest", "gold_manifest", "judgment_roster", "ordered_menu_freeze", "pilot_descriptor",
                 "answer_template"):
        pin(Path(freeze["inputs"][name]["path"]), freeze["inputs"][name]["sha256"])
    runtime = freeze["runtime"]
    require(runtime_pins(read(Path(freeze["inputs"]["pilot_descriptor"]["path"]))) == runtime,
            "runtime pins differ from the freeze")
    menu = read(Path(freeze["inputs"]["menu_manifest"]["path"]))
    cases = {row["case"] for row in freeze["cases"]}
    require(len(cases) == len(freeze["cases"]) == 76, "frozen case roster differs")
    for row in freeze["cases"]:
        source_closure(Path(row["source_root"]), menu["ordered_menus"][row["case"]], menu["documents"])
        for name in ("observation", "request"):
            pin(Path(row[name]["path"]), row[name]["sha256"])
    for plan in freeze["plans"].values():
        require(0 < plan["seconds"] <= 21600 and plan["batches"], "invalid frozen plan")
        catalog_bound(sorted({case for batch in plan["batches"] for case in batch["cases"]}))
        for batch in plan["batches"]:
            pin(Path(batch["scope"]["path"]), batch["scope"]["sha256"])
            require(set(batch["cases"]) <= cases and len(read(Path(batch["scope"]["path"]))["source_roots"]) <= 64,
                    "frozen enrollment batch differs")
    return {"cases": len(cases), "sources": freeze["sources"]}


def execute(native, output, freeze, freeze_pin, plan):
    validate(freeze)
    retry = pilot.FROZEN_RETRY if pilot.FROZEN_RETRY.is_file() else HERE / "native_cohort_source_reads.py"
    pin(retry, pilot.RETRY_SHA)
    repeat = runpy.run_path(str(retry))["publication_read"]
    runtime = freeze["runtime"]
    frozen = {row["case"]: row for row in freeze["cases"]}
    by_root = {row["source_root"]: row["case"] for row in freeze["cases"]}
    service, cli_image, focus = Path(runtime["service"]["path"]), Path(runtime["cli"]["path"]), runtime["focus_manifest"]
    order = catalog_bound(sorted({case for batch in plan["batches"] for case in batch["cases"]}))
    work, logs, stores, drafts = (output / name for name in ("fixture", "commands", "stores", "semantic-drafts"))
    logs.mkdir()
    stores.mkdir()
    control = work / "control"
    env = {key: value for key, value in os.environ.items() if not key.upper().startswith("AIRK_")}
    env.update(freeze["environment"])
    env.update(AIRK_ARBITER_EVALUATION_INPUTS=runtime["profiles_roster"]["path"],
               AIRK_ARBITER_EVALUATION_INPUTS_SHA256=runtime["profiles_roster"]["sha256"],
               AIRK_ARBITER_EVALUATION_WORK=str(work), AIRK_ARBITER_EVALUATION_SECONDS=str(plan["seconds"]))
    require(Path(env["TEMP"]) == Path(env["TMP"]) == output.with_name(output.name + "-temp"),
            "child temporary files must stay beside the private run")
    deadline = time.monotonic() + plan["seconds"]
    base = [cli_image, "--output", "json", "--service-runtime-dir", control]
    indexed, phases, gpu, enrollments = {}, [], [], []

    def run(command, folder, label, **extra):
        return native["run_logged"](command, folder, label, env | extra, deadline)

    def cli(folder, label, *arguments):
        return read(run(base + list(arguments), folder, label)[0])

    def phase(name):
        phases.append({"name": name, "unix_ms": int(time.time() * 1000)})

    def observe_gpu(label, pid=None):
        """Record the GPU process list. The fixture service must never appear in it."""
        tool = shutil.which("nvidia-smi")
        row = {"label": label, "observed": False, "service_pid": pid}
        if tool is not None:
            try:
                listing = run([tool, "--query-compute-apps=pid,process_name,used_memory", "--format=csv,noheader"],
                              logs, "gpu-" + label)[0].read_text(encoding="utf-8", errors="replace")
            except (OSError, TimeoutError, ValueError):
                pass
            else:
                listed = gpu_process_ids(listing)
                row.update(observed=True, listed_processes=len(listed), service_listed=pid in listed)
        gpu.append(row)
        require(not row.get("service_listed"), "fixture service appeared as a GPU process; CPU-only run stopped")

    def sample_builds():
        try:
            listing = subprocess.run(["tasklist", "/FO", "CSV", "/NH"], capture_output=True, text=True, timeout=30,
                                     env=env, stdin=subprocess.DEVNULL, creationflags=subprocess.CREATE_NO_WINDOW)
        except (OSError, subprocess.TimeoutExpired):
            return None
        return build_processes(listing.stdout) if listing.returncode == 0 else None

    def settled(folder, label, process, availability=None):
        """Bounded observation of the actual stage state; optional work must be idle before the next call."""
        require(process.poll() is None, "private service exited during the arms")
        for sample in range(240):
            status = cli(folder, f"{label}-{sample}", "arbiter", "status-legacy")
            if (all(status[stage]["queued"] == status[stage]["running"] == 0 for stage in ("focus", "context"))
                    and availability in (None, status["focus"]["availability"])):
                return status
            time.sleep(0.5)
        raise TimeoutError("optional stage work did not settle")

    def index_case(case, emitted):
        row = frozen[case]
        folder, store = logs / case, stores / case
        folder.mkdir()
        store.mkdir()
        request = read(Path(row["request"]["path"]))
        require(emitted | {"question": request["question"]} == request, "sealed request differs from native template")
        setup = run(base + ["--root", row["source_root"], "--data-dir", store, "library", "setup",
                            "--name", "techqa-stage-" + case, "--watch", "off", "--apply"], folder, "setup")[0]
        with setup.open("rb") as stream:
            draft, applied = map(json.loads, stream)
        require("draft" in draft and "receipt" in applied, "native setup draft or receipt missing")
        project = applied["receipt"]["request"]["registration"]["binding"]
        index_id = applied["receipt"]["index_id"]

        def job_cli(label, *arguments):
            return cli(folder, label, *arguments)

        if not indexed:
            job = job_cli("install-minilm", "--service-project", project, "configure",
                          "--file", drafts / "minilm-install.json")["job"]
            native["wait_job"](job_cli, job["id"], "configure")
        job = job_cli("index", "--service-project", project, "index")["job"]
        native["wait_job"](job_cli, job["id"], "index")
        observation_deadline = min(deadline, time.monotonic() + pilot.BUDGET)
        calls = 0

        def status_call(label, *arguments):
            nonlocal calls
            require(calls < 240, "native status observation count exceeded")
            calls += 1
            return read(native["run_logged"](base + list(arguments), folder, label, env, observation_deadline)[0])

        def status_cli(label, *arguments):
            return repeat(status_call, read, folder, label, arguments, observation_deadline)

        status = native["wait_semantic"](status_cli, index_id)
        require(status["files"] == 50 and status["read_scope"] == {"source": "named", "index_ids": [index_id]}
                and status["scope_observation"]["selection"]["owner"] == "agent", "native source count or scope differs")
        token = status["publication"]["token"]
        search = native["tool_body"](job_cli("pipeline", "tools", "call", "airk_search", "--index", index_id,
                                             "--args", json.dumps(PROBE)))
        readiness = search["semantic_readiness"]
        require(readiness["pipeline_digest"] == runtime["minilm_pipeline"] and readiness["state"] == "ready"
                and search["publication"]["token"] == token and search["execution"]["incomplete"] is False
                and search["execution"]["served_mode"] in ("hybrid", "vector"), "MiniLM publication differs")
        indexed[case] = {"case": case, "index_id": index_id, "binding": project, "publication_token": token,
                         "files": status["files"], "served_mode": search["execution"]["served_mode"],
                         "pipeline_digest": readiness["pipeline_digest"], "arms": {}}

    def answer(case, arm, process, **binding):
        folder, label = logs / case, "answer-" + arm.lower()
        builds, started = sample_builds(), int(time.time() * 1000)
        call = answer_call(run, folder, label, base + ["tools", "call", "airk_answer", "--index",
                                                       indexed[case]["index_id"], "--args-file",
                                                       frozen[case]["request"]["path"]])
        if call["state"] != "ok":
            settled(folder, label + "-settle", process)
        indexed[case]["arms"][arm] = {**call, **retained(folder, label), **binding,
                                      "started_unix_ms": started, "build_processes_before": builds}

    def configure_focus(batch, enabled, process, ready):
        prefix = ("enable" if enabled else "disable") + "-focus-" + batch["name"]
        scope = batch["scope"]
        expected = sorted(indexed[by_root[root]]["index_id"] for root in read(Path(scope["path"]))["source_roots"])
        run(native["ignored"](service, "emit_private_arbiter_configuration"), logs, "emit-" + prefix,
            AIRK_ARBITER_EVALUATION_SCOPES=scope["path"], AIRK_ARBITER_EVALUATION_SCOPES_SHA256=scope["sha256"],
            AIRK_ARBITER_EVALUATION_PROFILE=focus, AIRK_ARBITER_EVALUATION_STAGE="focus",
            AIRK_ARBITER_EVALUATION_ENABLED="true" if enabled else "false",
            AIRK_ARBITER_EVALUATION_CONTROL_PREFIX=prefix)
        request = read(work / (prefix + ".configure.json"))
        require(request["stage"] == "focus" and request["enabled"] is enabled and request["selected_profile"] == focus
                and sorted(entry["index_id"] for entry in request["indexes"]) == expected
                and all(entry["tasks"] == [FOCUS_TASK] for entry in request["indexes"]),
                "emitted Focus enrollment differs from the frozen batch")
        applied = cli(logs, "configure-" + prefix, "arbiter", "configure", "--legacy",
                      "--file", work / (prefix + ".configure.json"))
        revision = applied["configuration"]["revision"]
        warm = pilot.wait_stage(native, work, process, ready, "focus", focus, revision, deadline) if enabled else None
        state = settled(logs, "status-" + prefix, process, "ready" if enabled else "disabled")
        stage, configuration = state["focus"], state["configuration"]
        require(configuration["revision"] == revision and configuration["focus"]["enabled"] is enabled
                and not configuration["context"]["enabled"] and state["context"]["availability"] == "disabled"
                and sorted(entry["index_id"] for entry in configuration["focus"]["indexes"]) == expected
                and (stage["availability"] == "ready" and stage["active_profile"] == stage["selected_profile"] == focus
                     if enabled else stage["availability"] == "disabled"),
                "Focus stage state differs from the accepted configuration")
        enrollments.append({"batch": batch["name"], "enabled": enabled, "revision": revision, "indexes": len(expected),
                            "scope_sha256": scope["sha256"], "request": bound(work / (prefix + ".configure.json")),
                            "warm_sequence": None if warm is None else warm["sequence"]})
        return revision

    observe_gpu("before-service")
    with (output / "service.log").open("xb") as log:
        command = [service, "--ignored", "--exact", runtime["native_selector"], "--test-threads", "1"]
        with subprocess.Popen(command, cwd=ROOT, env=env, stdout=log, stderr=log, stdin=subprocess.DEVNULL,
                              creationflags=subprocess.CREATE_NO_WINDOW) as process:
            try:
                phase("service-started")
                native["wait_observed"](output, lambda: True if (work / "ready.json").is_file() else None,
                                        process, deadline, "stage-arm fixture readiness")
                ready = read(work / "ready.json")
                require(ready["health"]["process_id"] == process.pid
                        and ready["health"]["executable_sha256"] == runtime["service"]["sha256"]
                        and ready["inputs_sha256"] == runtime["profiles_roster"]["sha256"]
                        and control.samefile(ready["control_root"])
                        and sorted(entry["manifest"] for entry in ready["profiles"])
                        == sorted(entry["manifest"] for entry in runtime["profiles"]),
                        "fresh stage-arm fixture identity differs")
                observe_gpu("ready", process.pid)
                phase("index")
                run(native["ignored"](service, "emit_private_selected_vector_configuration"), logs, "emit-minilm",
                    AIRK_SELECTED_MINILM_DIRECTORY=str(Path(runtime["minilm"]["path"]).parent),
                    AIRK_SELECTED_VECTOR_DRAFT_OUTPUT=str(drafts))
                emitted = read(work / "context-request.json")
                for case in order:
                    index_case(case, emitted)
                idle = cli(logs, "stages-before-arm-a", "arbiter", "status-legacy")
                require(all(idle[stage]["availability"] == "disabled" and not idle["configuration"][stage]["enabled"]
                            and idle[stage]["queued"] == idle[stage]["running"] == 0 for stage in ("focus", "context")),
                        "both optional stages must be disabled for arm A")
                phase("arm-a")
                for case in order:
                    answer(case, "A", process, configuration_revision=idle["configuration"]["revision"])
                phase("focus-census")
                census = output / "focus-pair-census.private.json"
                run([sys.executable, "-I", "-B", HERE / "score_techqa_stage_arms.py", "--focus-census", "--run", output,
                     "--freeze", freeze_pin["path"], "--freeze-sha256", freeze_pin["sha256"], "--output", census],
                    logs, "focus-census")
                phase("arm-b")
                for number, batch in enumerate(plan["batches"]):
                    revision = configure_focus(batch, True, process, ready)
                    observe_gpu("focus-ready-" + batch["name"], process.pid)
                    for case in batch["cases"]:
                        answer(case, "B", process, batch=batch["name"], configuration_revision=revision)
                    observe_gpu("after-" + batch["name"], process.pid)
                    if number + 1 < len(plan["batches"]):
                        configure_focus(batch, False, process, ready)
                phase("closure")
                final = bound(run(base + ["arbiter", "status-legacy"], logs, "final-status")[0])
                state = read(Path(final["path"]))
                require(all(state[stage]["queued"] == state[stage]["running"] == 0 for stage in ("focus", "context")),
                        "optional stage work was still queued or running before shutdown")
            finally:
                if process.poll() is None and (work / "ready.json").is_file():
                    run(base + ["service", "shutdown"], logs, "shutdown")
            require(process.wait(timeout=max(0, min(pilot.BUDGET, deadline - time.monotonic()))) == 0,
                    "native fixture did not settle")
    phase("closed")
    closed, warm, observer = (read(work / name) for name in ("closed.json", "warm-completions.closed.json",
                                                             "composed-observer.closed.json"))
    require(closed["instance"] == ready["health"]["instance"] and closed["shutdown_completed"] is True
            and closed["service_lease_reacquired"] is True, "private lease was not naturally released")
    require(warm["complete"] is True and warm["dropped"] == 0 and warm["observer_id"] == ready["warm_observer_id"]
            and observer["complete"] is True and observer["dropped"] == observer["writer_failed"] == 0,
            "native warm or dispatch journal did not close completely")
    journal = work / "composed-observer.jsonl"
    require(journal.stat().st_size == observer["bytes"] <= 64 << 20, "dispatch journal size differs")
    with journal.open("rb") as stream:
        dispatches = [json.loads(line) for line in stream]
    require(len(dispatches) == observer["records"]
            and all(row["task"] == FOCUS_TASK and row["profile_manifest"] == focus for row in dispatches),
            "dispatch journal holds work outside the Focus arm")
    validate(freeze)
    native["write_json"](output / "capture.private.json", {
        "schema": "airk.private.techqa-stage-arms-capture/1", "freeze": freeze_pin, "plan": plan_name(output),
        "fixture_instance": closed["instance"], "service_pid": process.pid,
        "service": runtime["service"], "cli": runtime["cli"], "image_scope": runtime["image_scope"],
        "cases": [indexed[case] for case in order], "enrollments": enrollments, "focus_census": bound(census),
        "final_status": final, "phases": phases, "gpu": gpu,
        "closure": {name: bound(work / name) for name in ("ready.json", "closed.json", "warm-completions.closed.json",
                                                         "warm-completions.jsonl", "composed-observer.closed.json",
                                                         "composed-observer.jsonl")},
        "focus_dispatches": sum(row["kind"] == "dispatch_admitted" for row in dispatches),
        "dispatch_records": len(dispatches), "temporary_directory": env["TEMP"],
        "evidence_limits": freeze["evidence_limits"], "judged": False, "release_qualified": False,
    })


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepare", action="store_true", help="seal the requests, scopes and rule; starts no service")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--freeze", type=Path)
    parser.add_argument("--freeze-sha256")
    parser.add_argument("--check-only", action="store_true", help="re-pin the frozen inputs without a launch")
    parser.add_argument("--dispose-weights", action="store_true",
                        help="remove a settled run's installed duplicate weights after rehashing the originals")
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    require(os.name == "nt" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use checked Windows Python -I -B -S")
    pin(HERE / "run_techqa_predispatch_pilot.py", PILOT_SHA)
    if args.prepare:
        require(args.output is not None and args.freeze is None and not args.worker and not args.check_only,
                "prepare takes only a fresh checked input destination")
        prepare(args.output)
        return
    require(args.freeze is not None and re.fullmatch(r"[0-9a-f]{64}", args.freeze_sha256 or "")
            and args.freeze.parent.parent == CAMPAIGN and args.freeze.name == "freeze.private.json",
            "pinned stage-arm freeze required")
    freeze_pin = {"path": str(args.freeze), "sha256": pin(args.freeze, args.freeze_sha256)}
    freeze = read(args.freeze)
    pin(pilot.NATIVE, pilot.NATIVE_SHA)
    pin(ROOT / "scripts/provision-static-onnx-runtime.py", pilot.OWNER_SHA)
    native = runpy.run_path(str(pilot.NATIVE))
    owner = native["owner_module"]()
    if args.check_only:
        require(args.output is None and not args.worker, "check-only takes no execution target")
        print(json.dumps({"verified": freeze_pin["sha256"], **validate(freeze), "runtime_started": False}))
        return
    output = owner.plain(args.output)
    kind = plan_name(output)
    plan = freeze["plans"][kind]
    handler = Path(__file__).resolve()
    if args.dispose_weights:
        require(not args.worker and read(output / "owned.json")["freeze"] == freeze_pin,
                "disposal needs the settled run of this freeze")
        print(json.dumps(dispose_weights(output, freeze)))
        return
    if args.worker:
        require(os.environ.get(OWNER_VARIABLE) == str(output), "original owner required")
        with native["capped_worker_job"](output):
            execute(native, output, freeze, freeze_pin, plan)
        return
    temporary = output.with_name(output.name + "-temp")
    require(not output.exists() and not temporary.exists(), "fresh stage-arm and temporary targets required")
    validate(freeze)
    admitted = admission(freeze, native["HOST_BYTES"], output.parent)
    print(json.dumps({"admission": admitted}), flush=True)
    output.mkdir()
    temporary.mkdir()
    before = harness()
    result = owner.run_evaluation_fixture(
        [sys.executable, "-I", "-B", "-S", handler, "--output", output, "--freeze", args.freeze,
         "--freeze-sha256", freeze_pin["sha256"], "--worker"], ROOT, output / "outer.log", plan["seconds"],
        os.environ | {OWNER_VARIABLE: str(output), "TEMP": str(temporary), "TMP": str(temporary)})
    result.update(plan=kind, freeze=freeze_pin, admission=admitted, harness_sha256=before,
                  harness_matches_freeze=before == freeze["harness_at_freeze"], support_sha256=freeze["sources"],
                  free_bytes_after=shutil.disk_usage(output.parent).free)
    result["source_seals_unchanged"] = (harness() == before and seals() == freeze["sources"]
                                        and pin(pilot.NATIVE) == pilot.NATIVE_SHA)
    native["write_json"](output / "owned.json", result)
    require(owner.successful(result) and result["source_seals_unchanged"]
            and result["owned_job_final_active_processes"] == 0 and result["owned_job_final_member_pids"] == []
            and (output / "capture.private.json").is_file(), "stage-arm run did not settle")


if __name__ == "__main__":
    main()
