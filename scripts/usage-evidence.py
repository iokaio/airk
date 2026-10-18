# SPDX-License-Identifier: Apache-2.0
"""Convert explicit content-free usage counters and compare matched task measurements.

No log discovery, credentials, network requests or agent execution. See docs/usage-evaluation.md.
"""
import argparse
import json
import re
import statistics
from pathlib import Path

LIMIT = 1_000_000_000_000
SESSION_FIELDS = {"model_revision_sha256", "requests", "input_tokens", "cached_input_tokens",
                  "output_tokens", "elapsed_ms", "tool_round_trips"}
CLIENTS = {"unknown", "cli", "vscode", "claude_code", "codex", "gemini", "cursor", "cline", "roo",
           "junie", "copilot_cli", "open_code", "kilo", "amazon_q", "windsurf"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_json(file, cap=1_048_576):
    with Path(file).open("rb") as stream:
        data = stream.read(cap + 1)
    require(len(data) <= cap, "input exceeds byte limit")
    return json.loads(data.decode("utf-8"))


def fields(value, allowed):
    require(isinstance(value, dict) and not (value.keys() - allowed),
            "expected content-free object with supported fields only")
    return value


def number(value):
    require(value is None or type(value) is int and 0 <= value <= LIMIT,
            "counter must be a bounded nonnegative integer or null")
    return value


def hex_id(value, length):
    require(isinstance(value, str) and re.fullmatch(r"[0-9a-fA-F]{%d}" % length, value),
            "invalid opaque hexadecimal identity")
    return value.lower()


def complete_sum(values):
    return None if any(v is None for v in values) else number(sum(values))


def validate_session(session):
    fields(session, SESSION_FIELDS)
    hex_id(session.get("model_revision_sha256"), 64)
    require(type(session.get("requests")) is int and 1 <= session["requests"] <= 1_000_000,
            "requests must be 1..1000000")
    result = {key: number(session.get(key)) for key in SESSION_FIELDS - {"model_revision_sha256"}}
    result["model_revision_sha256"] = session["model_revision_sha256"].lower()
    cached, total = result["cached_input_tokens"], result["input_tokens"]
    require(cached is None or total is not None and cached <= total, "invalid cache split")
    return result


def adapt_usage(kind, records, model, elapsed=None, rounds=None):
    """Each record must be one final, non-overlapping API request's usage object."""
    require(isinstance(records, list) and 1 <= len(records) <= 1000, "supply 1..1000 final usage objects")
    totals, cached, outputs = [], [], []
    for record in records:
        if kind == "openai-responses":
            fields(record, {"input_tokens", "input_tokens_details", "output_tokens", "output_tokens_details", "total_tokens"})
            detail = fields(record.get("input_tokens_details") or {}, {"cached_tokens", "cache_write_tokens"})
            output_detail = fields(record.get("output_tokens_details") or {}, {"reasoning_tokens"})
            for value in output_detail.values():
                number(value)
            total = number(record.get("input_tokens"))
            hit = number(detail.get("cached_tokens"))
            writes = number(detail.get("cache_write_tokens"))
            # Cache-write pricing is not represented by AIRK's two input rates.
            if writes is None or writes > 0:
                hit = None
            number(record.get("total_tokens"))
        else:
            require(kind == "anthropic-messages", "unsupported counter adapter")
            fields(record, {"input_tokens", "output_tokens", "cache_read_input_tokens", "cache_creation_input_tokens", "cache_creation"})
            creation = fields(record.get("cache_creation") or {}, {"ephemeral_5m_input_tokens", "ephemeral_1h_input_tokens"})
            for value in creation.values():
                number(value)
            writes = number(record.get("cache_creation_input_tokens"))
            hit = number(record.get("cache_read_input_tokens"))
            total = complete_sum([number(record.get("input_tokens")), hit, writes])
            if writes is None or writes > 0:
                hit = None
        output = number(record.get("output_tokens"))
        require(hit is None or total is not None and hit <= total, "invalid cache split")
        totals.append(total)
        cached.append(hit)
        outputs.append(output)
    return validate_session({"model_revision_sha256": model, "requests": len(records),
                             "input_tokens": complete_sum(totals), "cached_input_tokens": complete_sum(cached),
                             "output_tokens": complete_sum(outputs), "elapsed_ms": elapsed, "tool_round_trips": rounds})


def measurement(session, event_id, timestamp, client, request_ids):
    require(client in CLIENTS, "unsupported client category")
    require(type(timestamp) is int and 0 <= timestamp <= LIMIT * 1000, "invalid observation time")
    require(len(request_ids) <= 100, "at most 100 AIRK request IDs")
    ids = [hex_id(value, 32) for value in request_ids]
    require(len(set(ids)) == len(ids), "duplicate request ID")
    return {"schema_version": 1, "event_id": hex_id(event_id, 32), "unix_ms": timestamp,
            "client": client, "content_kind": "unknown", "provenance": "client_reported",
            "context": None, "session": session, "paired_task": None, "airk_request_ids": ids}


def paired(with_airk, without_airk, with_result, without_result, attest):
    require(attest, "explicit matched-task and model attestation required")
    a, b = validate_session(with_airk), validate_session(without_airk)
    require(a["model_revision_sha256"] == b["model_revision_sha256"], "paired model revisions differ")
    require(with_result in {"passed", "failed", "unknown"} and without_result in {"passed", "failed", "unknown"}, "invalid task result")
    return {"same_task_and_model": True, "with_airk": a, "without_airk": b,
            "with_airk_result": with_result, "without_airk_result": without_result}


def compare(pairs):
    require(isinstance(pairs, list) and 1 <= len(pairs) <= 1000, "supply 1..1000 matched pairs")
    differences = {key: [] for key in ["input_tokens", "output_tokens", "elapsed_ms", "tool_round_trips"]}
    successful = {key: [] for key in differences}
    quality = {key: 0 for key in ["both_passed", "only_airk_passed", "only_alternative_passed", "both_failed", "unknown"]}
    model = None
    for item in pairs:
        fields(item, {"same_task_and_model", "with_airk", "without_airk", "with_airk_result", "without_airk_result"})
        p = paired(item["with_airk"], item["without_airk"], item["with_airk_result"], item["without_airk_result"], item["same_task_and_model"] is True)
        current = p["with_airk"]["model_revision_sha256"]
        require(model is None or model == current, "compare one model revision at a time")
        model = current
        a, b = p["with_airk_result"], p["without_airk_result"]
        bucket = "unknown" if "unknown" in (a, b) else "both_passed" if a == b == "passed" else "both_failed" if a == b == "failed" else "only_airk_passed" if a == "passed" else "only_alternative_passed"
        quality[bucket] += 1
        for key in differences:
            x, y = p["with_airk"][key], p["without_airk"][key]
            if x is not None and y is not None:
                differences[key].append(y - x)
                if bucket == "both_passed":
                    successful[key].append(y - x)

    def summary(values):
        return {key: {"measured_pairs": len(rows), "missing_pairs": len(pairs) - len(rows),
                      "total_reduction": sum(rows) if rows else None,
                      "median_reduction": statistics.median(rows) if rows else None,
                      "minimum_reduction": min(rows) if rows else None,
                      "maximum_reduction": max(rows) if rows else None} for key, rows in values.items()}
    return {"schema": "airk-paired-evaluation/1", "model_revision_sha256": model, "pairs": len(pairs),
            "quality": quality, "all_pairs": summary(differences), "both_passed_pairs": summary(successful),
            "notice": "Supplied matched-task evidence, not verified causal savings. Positive differences mean less work with AIRK. Both-passed results are a selected subset; inspect failures too."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    adapt = sub.add_parser("adapt", help="Convert an array of final content-free API usage objects")
    adapt.add_argument("--adapter", choices=["openai-responses", "anthropic-messages"], required=True)
    adapt.add_argument("--input", type=Path, required=True)
    adapt.add_argument("--model-revision-sha256", required=True)
    adapt.add_argument("--elapsed-ms", type=int)
    adapt.add_argument("--tool-round-trips", type=int)
    pair = sub.add_parser("pair", help="Package two normalized final session objects for native import")
    pair.add_argument("--with-airk", type=Path, required=True)
    pair.add_argument("--without-airk", type=Path, required=True)
    pair.add_argument("--with-result", choices=["passed", "failed", "unknown"], required=True)
    pair.add_argument("--without-result", choices=["passed", "failed", "unknown"], required=True)
    pair.add_argument("--attest-matched-task-and-model", action="store_true")
    for command in [adapt, pair]:
        command.add_argument("--event-id", required=True, help="Stable random 32-digit hex ID; reuse when retrying this final measurement")
        command.add_argument("--unix-ms", type=int, required=True, help="Original observation time, never a refreshed retry timestamp")
        command.add_argument("--client", choices=sorted(CLIENTS), default="unknown")
        command.add_argument("--airk-request-id", action="append", default=[])
    comparison = sub.add_parser("compare", help="Summarize an array of paired_task objects, preserving failures and missing counters")
    comparison.add_argument("--input", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "compare":
            result = compare(read_json(args.input))
        else:
            session = adapt_usage(args.adapter, read_json(args.input), args.model_revision_sha256, args.elapsed_ms, args.tool_round_trips) if args.command == "adapt" else None
            result = measurement(session, args.event_id, args.unix_ms, args.client, args.airk_request_id)
            if args.command == "pair":
                result["paired_task"] = paired(read_json(args.with_airk, 16384), read_json(args.without_airk, 16384), args.with_result, args.without_result, args.attest_matched_task_and_model)
        encoded = json.dumps(result, indent=2, sort_keys=True)
        require(len(encoded.encode()) <= (16384 if args.command != "compare" else 65536), "output exceeds bound")
        print(encoded)
    except (ValueError, KeyError, TypeError, OSError):
        parser.exit(2, "Invalid or unsupported content-free measurement. Check the documented schema, bounds, model identity and matched-task attestation.\n")


if __name__ == "__main__":
    main()
