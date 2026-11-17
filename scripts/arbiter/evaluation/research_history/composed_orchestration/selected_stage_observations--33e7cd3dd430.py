# SPDX-License-Identifier: Apache-2.0
"""Evaluation bookkeeping over complete Rust-owned native values.

This is not a native schema/validator or a replacement for canonical validation.
Original dictionaries are retained unchanged; only experiment identity, declared
stage state, qualification scope and work-accounting assertions are added here.
"""
CURRENT_MODELS = {"arbiter-focus-qwen3-reranker-06b", "arbiter-context-qwen3-17b", "arbiter-context-qwen3-4b"}

def require(value, reason):
    if not value:
        raise ValueError(reason)

def check_status(status, context_profile=None, focus_profile=None):
    require(status["schema"] == "airk.arbiter.status/2", "native Status2 required")
    for stage, profile in (("context", context_profile), ("focus", focus_profile)):
        config, state = status["configuration"][stage], status[stage]
        require(config["enabled"] == (profile is not None), "stage consent differs from frozen condition")
        require(config["selected_profile"] == state["selected_profile"], "stage selected identity mismatch")
        if profile is not None:
            require(state["selected_profile"] == profile, "selected stage profile differs")
            if state["availability"] == "ready":
                require(state["active_profile"] == profile, "ready stage active identity differs")
    require({row["model"] for stage in ("focus", "context") for row in status[stage]["profiles"]} == CURRENT_MODELS,
            "selected three-model native roster differs")
    return status

def observe(value, tool, context_profile=None, focus_profile=None, generic_read_only=True):
    focus, history, unknown = [], [], False
    focus_tokens, context_tokens = 0, 0
    if tool == "answer":
        require(value["schema"] == "airk.arbiter.context/2", "native Context2 required")
        history = value["assessment_history"]
        require(context_profile is not None or not history, "Context-off condition performed an assessment")
        for event in history:
            result = event["result"]
            unknown |= bool(result.get("work_unknown", False))
            advice = result.get("advice")
            if advice is not None:
                require(advice["profile"] == context_profile, "Context advice profile substitution")
                require(advice["task"] == "airk.context_action.v1", "wrong Context task")
                observed = advice["observed_input_tokens"]
                unknown |= observed is None
                context_tokens += advice["work"]["input_tokens"]
            elif result["status"] == "unavailable":
                unknown |= result["work_unknown"]
                context_tokens += result["work"]["input_tokens"]
            if result["status"] == "selected" and generic_read_only:
                card = result["card"]
                require(card["action"]["kind"] == "read" and card["gap"] == "unsupported"
                        and card["reason"] == "additional_evidence_may_help"
                        and all(row["role"] == "unknown" and row["relevance"] == "uncertain" for row in card["assessments"]),
                        "unqualified semantic Context card selected")
        searches = [r["canonical_result"] for r in value["records"] if r["operation"] == "search"]
    else:
        searches = [value] if tool == "search" else []
    seen = set()
    for search in searches:
        observation = search.get("focus")
        if observation is None:
            continue
        focus.append(observation)
        require(focus_profile is not None, "Focus-off condition emitted Focus observation")
        if observation["kind"] == "not_applied":
            require(observation["work_unknown"] is False, "compact notice cannot conceal unknown work")
            continue
        receipt = observation["receipt"]
        if receipt["identity"] is not None:
            require(receipt["identity"]["profile"] == focus_profile, "Focus profile substitution")
        observed = receipt["observed_input_tokens"]
        unknown |= observed is None
        focus_tokens += receipt["work"]["input_tokens"]
        if receipt["identity"] is not None and (receipt["reserved_input_tokens"] > 0 or observed is None or receipt["scores"]):
            require(receipt["input_digest"] not in seen, "replayed native Focus work")
            seen.add(receipt["input_digest"])
    if tool == "answer":
        # Core owns nested additive accounting: outer Work ALREADY includes both.
        # Never add nested Focus/advice work to this charge again.
        tokens = value["work"]["input_tokens"]
        require(tokens >= focus_tokens + context_tokens, "native aggregate omitted stage work")
        require(tokens <= 2048, "combined native original token cap exceeded")
        coverage = value["assessment"]["coverage"]
    else:
        tokens, coverage = focus_tokens, None
        require(tokens <= 2048, "native Focus original token cap exceeded")
    return {"Context_history": history, "Context_current_coverage": coverage,
            "Focus_observations": focus, "known_input_tokens": tokens,
            "Focus_known_input_tokens": focus_tokens, "Context_known_input_tokens": context_tokens,
            "work_unknown": unknown,
            "qualification":"Generic Read reference-utility scope only; relevance and current coverage are not sufficiency gold."}
