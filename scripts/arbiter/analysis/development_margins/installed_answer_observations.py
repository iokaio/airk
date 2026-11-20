# SPDX-License-Identifier: Apache-2.0
"""Verify native product selection and preserve complete Answer and per-stage observations."""
import math
import re


def require(ok, message):
    if not ok:
        raise ValueError(message)


def stages(status, indices, arm, focus):
    require(status["schema"] == "airk.arbiter.status/3" and status.get("initialization") is None,
            "current product device configuration is not initialized")
    config = status["configuration"]
    require(config["revision"] > 0, "missing native configuration revision")
    out = {}
    for stage, model, profile, backend, task in (
        ("focus", "focus", focus, None, "airk.focus_rank.v1"),
        ("context", arm["model"], arm["context_manifest"], "cuda" if arm["provider"] == "cuda" else None, "airk.context_action.v1"),
    ):
        cfg, state = config[stage], status[stage]
        require(cfg["enabled"] and cfg["selected_model"] == state["selected_model"] == model
                and state["queued"] == state["running"] == 0 and state["availability"] == "ready"
                and state["active_profile"] == profile, "product stage is not ready with the requested model")
        require({r["index_id"] for r in cfg["indexes"]} == set(indices) and len(cfg["indexes"]) == len(indices)
                and all(task in r["tasks"] for r in cfg["indexes"]), "product enrollment differs from frozen menus")
        selection = state["selection"]
        choice = selection["selected"]
        require(choice and not selection["operation_failure"] and selection["model"] == model
                and choice["manifest"] == profile and choice["backend"] == backend, "missing or fallback native provider selection")
        variant, = [v for m in state["models"] if m["model"] == model for v in m["variants"] if v["manifest"] == profile]
        require(variant["model_admission"] == "qualified" and variant["availability"] == "ready"
                and variant["execution_identity"] == choice["execution_identity"]
                and variant["backend"] == backend and task in variant["tasks"], "native variant lacks qualified active identity")
        if backend == "cuda":
            require(choice["precision"] == variant["precision"] == "float16" and variant["device_evidence"],
                    "selected CUDA variant lacks qualified FP16 placement")
        out[stage] = {"selected": choice, "variant": variant}
    model, = [m for m in status["context"]["models"] if m["model"] == arm["model"]]
    cpu_options = [v for v in model["variants"] if v["backend"] is None and v["model_admission"] == "qualified"]
    return {"configuration": config, "stages": out, "qualified_same_model_cpu_variants": cpu_options}


def measure_answer(answer, scope, index, request, focus_profile, arm):
    require(answer["question"] == request["question"] and re.fullmatch("[a-f0-9]{32}", answer["request_id"])
            and answer["scope_observation"] == scope["scope_observation"]
            and answer["read_scope"] == {"source": "named", "index_ids": [index]}, "returned Answer has different request or scope")
    partition = answer["assessment"]["scope"]
    require(partition["index_id"] == index and partition["publication"]["token"] == scope["publication"]["token"],
            "original Answer publication differs")
    focus, context = [], []
    for record in answer["records"]:
        data = record["canonical_result"]
        if record["operation"] != "search":
            continue
        event = data["focus"]
        if event["kind"] == "receipt":
            receipt = event["receipt"]
            require(receipt["scope"] == partition and receipt["identity"]["profile"] == focus_profile,
                    "observed Focus receipt has different scope or profile")
        focus.append(event)
    for event in answer["assessment_history"]:
        require(event["partition"] == partition, "assessment escaped authorized publication")
        result = event["result"]
        advice = result.get("advice")
        if advice:
            require(advice["scope"] == partition and advice["profile"] == arm["context_manifest"]
                    and advice["calibration"] == arm["calibration_sha256"] and advice["runtime"] == arm["runtime"],
                    "observed Context profile, calibration, runtime or scope differs")
        context.append({"ordinal": event["ordinal"], "status": result["status"], "work": result.get("work"),
                        "reason": result.get("reason"), "work_unknown": result.get("work_unknown"),
                        "advice": advice, "card": result.get("card")})
    selected = [r for r in context if r["status"] == "selected"]
    def component(work):
        return {key: {"known_us": sum(w[key] for w in work if w.get(key) is not None),
                      "reported": sum(w.get(key) is not None for w in work), "observations": len(work)}
                for key in ("compile_us","inference_us","queue_us","policy_us")}
    focus_work = [f["receipt"]["work"] for f in focus if f["kind"] == "receipt"]
    context_work = [(r["advice"]["work"] if r["advice"] else r["work"]) or {} for r in context]
    return {"request_id": answer["request_id"], "stop": answer["stop"], "work": answer["work"],
            "focus_work": component(focus_work), "context_work": component(context_work),
            "termination": answer["assessment"]["termination"], "focus": focus, "context": context,
            "selected_context": len(selected), "partition": partition}


def native_state(native, elapsed_ns, request):
    stop, termination = native["stop"],native["termination"]
    require(stop in ("covered","no_progress","no_valid_read","budget","deadline","stale","refused","unsupported")
            and termination in ("pending","covered","clarification_required","no_justified_read","no_progress","budget",
                                "deadline","refused","stale","unsupported"), "unknown native Answer disposition")
    limit = request["budget"]["deadline_ms"]
    require(type(elapsed_ns) is int and elapsed_ns >= 0 and type(limit) is int and limit > 0,
            "invalid native Answer clock or deadline")
    if elapsed_ns > limit * 1_000_000 or "deadline" in (stop,termination):
        return "timeout"
    for reason in (stop,termination):
        if reason in ("refused","stale","unsupported","pending"):
            return "native_" + reason
    return "observed"


def summary(rows, order):
    require(len(order) == len(rows) == 20 and [r["case"] for r in rows] == order
            and [r["ordinal"] for r in rows] == list(range(20)), "complete weighted timing denominator required")
    times = [r["elapsed_ns"] for r in rows if r.get("elapsed_ns") is not None]
    require(all(type(t) is int and t >= 0 for t in times), "invalid elapsed time")
    complete = [r for r in rows if r["state"] == "observed"]
    p95 = sorted(times)[math.ceil(.95 * len(order)) - 1] if len(times) == len(order) else None
    overrun = sum(t > 120_000_000_000 for t in times)
    return {"denominator": 20, "complete": len(complete), "timed": len(times), "p95_elapsed_ns": p95,
            "deadline_overruns": overrun,
            "timeout": sum(r["state"] == "timeout" for r in rows), "deferred": sum(r["state"] == "deferred" for r in rows),
            "zero_selected_context": sum(r["native"]["selected_context"] == 0 for r in complete),
            "complete_selected_context": sum(r["native"]["selected_context"] > 0 for r in complete)}
