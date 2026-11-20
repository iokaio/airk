# SPDX-License-Identifier: Apache-2.0
"""Attribute original service dispatch intervals to the timed CLI exchange that delivered them."""
import hashlib
import json


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def sha(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def measure(transcript, observer, plan_ids, pass_index):
    require(len(plan_ids) == len(set(plan_ids)) == 4 and pass_index in (0,1), "unknown timing phase")
    journal = [e for e in observer if e["kind"] in ("dispatch_admitted","score_returned")]
    assigned = set()
    results = []
    marks = [i for i,e in enumerate(transcript) if e["kind"] == "timing_exchange"]
    grants = [e for e in transcript if e["kind"] == "window_grant"]
    require([e["question_id"] for e in grants] == plan_ids and len({e["result"]["scope_identity"] for e in grants}) == 4,
            "missing or reused timing grant")
    expect_arms = (["full","local4","local4","full"],["local4","full","full","local4"])[pass_index]
    evidence = [e["evidence"] for e in transcript if e["kind"] == "cal_question"]
    require([e["id"] for e in evidence] == plan_ids, "original question census differs")
    schedule = [(n, k) for n, e in enumerate(evidence) for k in range(2 + len(e["probes"]))]
    require(len(marks) == len(schedule) == len([e for e in transcript if e["kind"] == "native_exchange"]),
            "incomplete original SDK exchange census")
    identities = {(transcript[i]["service_pid"], transcript[i]["service_instance"], transcript[i]["cli_pid"]) for i in marks}
    require(len(identities) == 1, "native service or client changed within pass")
    previous = None
    for i in marks:
        m = transcript[i]
        raw = transcript[i+1]
        require(m["native_delivery"] is True and raw["kind"] == "native_exchange" and raw["argv"][2] == m["tool"],
                "missing original CLI response")
        argv,result = raw["argv"],raw["result"]
        require(sha(json.dumps(argv,separators=(",",":"),ensure_ascii=False)) == m["argv_sha256"] and
                sha(result["stdout"]) == m["stdout_sha256"] and sha(result["stderr"]) == m["stderr_sha256"] and
                result["exitCode"] == m["exit_code"] and result["class"] == m["class"], "timing response changed")
        n = m["ordinal"] - 96
        expected_n, k = schedule[len(results)]
        require(0 <= n < 4 and n == expected_n and m["exchange"] == k + 1 and m["question_id"] == plan_ids[n] and m["arm"] == expect_arms[n]
                and m["pass_index"] == pass_index and m["scope"] == grants[n]["result"]["scope_identity"]
                and type(m["cli_pid"]) is int and m["cli_pid"] > 0, "wrong timing opportunity")
        original = evidence[n]
        probes = original["probes"]
        tool = ("status", "answer")[k] if k < 2 else probes[k-2]["probe"]["tool"]
        require(m["tool"] == tool, "original tool differs")
        reply = json.loads(result["stdout"])
        body = reply.get("structuredContent")
        if body is None:
            require(len(reply["content"]) == 1 and reply["content"][0]["type"] == "text", "missing SDK body")
            body = json.loads(reply["content"][0]["text"])
        delivered = {"refused": False, "body": original["scope"]} if k == 0 else (original["answer"] if k == 1 else probes[k-2]["result"])
        refused = delivered["refused"]
        require(type(refused) is bool and body == delivered["body"] and
                (result["exitCode"], result["class"], reply.get("isError",False)) ==
                ((1,"refused",True) if refused else (0,"success",False)), "original delivery differs")
        if k >= 2:
            require(argv[3] == "--args" and json.loads(argv[4]) == probes[k-2]["probe"]["arguments"], "original probe arguments differ")
        start,end = m["start"],m["end"]
        lo,hi = int(start["monotonic_ns"]),int(end["monotonic_ns"])
        require(hi > lo and start["unix_ms"] <= end["unix_ms"] and (previous is None or lo >= previous),
                "invalid exchange clock")
        by_dispatch = {}
        for e in journal:
            if start["unix_ms"] <= e["unix_ms"] <= end["unix_ms"]:
                key = (e["observer_id"],e["dispatch_identity"],e["kind"])
                require(key not in assigned and e["task"] == "airk.focus_rank.v1", "ambiguous or foreign native interval")
                assigned.add(key)
                by_dispatch.setdefault(key[:2],[]).append(e)
        intervals = []
        infer = 0
        for events in by_dispatch.values():
            require(len(events) == 2 and [e["kind"] for e in events] == ["dispatch_admitted","score_returned"],
                    "partial native numerical interval")
            before,after = events
            require(after["return_observed"] and after["outcome"] == "success" and before["input_digest"] == after["input_digest"]
                    and before["unix_ms"] <= after["unix_ms"],"incomplete native score return")
            intervals.append((before["unix_ms"],after["unix_ms"]))
            infer += after["work"]["inference_us"]
        union_ms = 0
        stop = None
        for begin,finish in sorted(intervals):
            union_ms += max(0,finish-(begin if stop is None else max(stop,begin)))
            stop = finish if stop is None else max(stop,finish)
        total_us = (hi-lo)//1000
        # Native timestamps have millisecond resolution. Keep the residual signed.
        results.append({"ordinal":m["ordinal"],"arm":m["arm"],"exchange":m["exchange"],"tool":m["tool"],
                        "scope":m["scope"],"argv_sha256":m["argv_sha256"],"refused":refused,"cli_elapsed_us":total_us,"outside_exchange_gap_us":None if previous is None else (lo-previous)//1000,
                        "native_focus_calls":len(intervals),"native_inference_us":infer,"native_dispatch_union_ms":union_ms,
                        "other_within_exchange_us":total_us-union_ms*1000,"response_bytes":len(result["stdout"].encode()),
                        "stdout_sha256":m["stdout_sha256"]})
        previous=hi
    finished = [e for e in transcript if e["kind"] == "timing_question_completed"]
    require([e["question_id"] for e in finished] == plan_ids and
            [e["question_id"] for e in transcript if e["kind"] == "window_release"] == plan_ids,
            "incomplete instrumented questions")
    for n,f in enumerate(finished):
        calls = [r for r in results if r["ordinal"] == n+96]
        count = 2 + len(evidence[n]["probes"])
        require(len(calls) == f["exchanges"] == count and [r["exchange"] for r in calls] == list(range(1,count+1)) and
                [r["tool"] for r in calls[:2]] == ["status","answer"] and
                all(r["tool"] in ("context","search") for r in calls[2:]),"missing or changed delivered menu")
    first_ms = transcript[marks[0]]["start"]["unix_ms"]
    last_ms = transcript[marks[-1]]["end"]["unix_ms"]
    in_pass = [e for e in journal if first_ms <= e["unix_ms"] <= last_ms]
    require(len(in_pass) == len(assigned) and len({e["observer_id"] for e in in_pass}) <= 1 and (in_pass or all(e["answer"]["refused"] for e in evidence)),
            "missing native observer or original dispatch outside an exchange")
    return results


def compare_menus(first, second):
    """Keep ordinal, operation and exact original arguments aligned across both passes."""
    keys = ("ordinal","exchange","tool","argv_sha256")
    require(first and [tuple(r[k] for k in keys) for r in first] ==
            [tuple(r[k] for k in keys) for r in second], "delivered timing menus differ")
