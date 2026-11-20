# SPDX-License-Identifier: Apache-2.0
"""Authenticate the warm timing inputs and check every measured native score word."""
import math
import hashlib
import struct

WIDTHS = (32,128,1024,8192)
RECEIPTS = {
    32:"8c24b283557180bb94880916b8c606af1bc360ea121db7b5851339a564165b8a",
    128:"b79a93321343b9549f61e99cb9f6d8cfe4b71b294769783451827eb3b1b4ad77",
    1024:"a683a652b37ef59c39fdda85364aac4ec892912bfcf269b02e1f49bfdfd6ecb8",
}
CANDIDATE = "147be075a73ff81ca17b6c2fb965ec8f887e10908fbd33cf47a919d1694aa3d9"


def words(values):
    if len(values)!=8 or not all(math.isfinite(v) for v in values):
        raise ValueError("eight finite canonical control scores required")
    return list(struct.unpack("<8I",struct.pack("<8f",*values)))


def inputs(root, reviewed_graph, maximum_receipt_sha, base, exact):
    score_words=[]
    custody=[]
    for width in WIDTHS:
        sha=maximum_receipt_sha if width==8192 else RECEIPTS[width]
        source=exact(root/f"compact-cutlass-width-score-{width}-r1/execution.json",sha,base)
        value=base.read(source["path"])
        owned=value["owned"]
        result=value["result"]
        numeric=value["numerical"]
        if not (value["complete"] and value["mode"]=="score" and value["case"]==str(width)
                and result["model_sha256"]==CANDIDATE and result["typed_graph_sha256"]==reviewed_graph
                and result["score"]["forward_synchronized"] and len(value["kernel_records"])==28
                and numeric["release_numerical_case_pass"] and numeric["matching"]["winner_agrees"]
                and numeric["candidate_scores"]==result["score"]["raw_scores"] and owned["exit_code"]==0
                and owned["owned_descendants_settled"] and not owned["owned_job_forced_termination"]
                and owned["owned_job_final_active_processes"]==0):
            raise ValueError(f"incomplete original protected CUDA T{width} observation")
        score_words.append(words(numeric["candidate_scores"]))
        custody.append(source)
    return score_words,custody


def confirm_journal(events,rows,expiry,env,image_sha):
    if len(events)!=len(rows)*2 or len(events)==0 or expiry!=int(env["AIRK_COMPACT_CUTLASS_EXPIRES_NS"]):
        raise ValueError("incomplete or renewed original timing journal")
    source=hashlib.sha256(env["AIRK_COMPACT_CUTLASS_EXPECTED_WORDS"].encode()).hexdigest()
    pid=events[0]["identity"]["process_id"]
    if type(pid) is not int or pid<=0:
        raise ValueError("invalid native process identifier")
    for index,row in enumerate(rows):
        start,stop=events[2*index:2*index+2]
        ordinal=index if rows[0]["kind"]=="warmup" else index+1
        for event,phase in ((start,"started"),(stop,"returned")):
            identity=event["identity"]
            if not (event["phase"]==phase and event["kind"]==row["kind"] and event["ordinal"]==ordinal
                    and event["clock"]=="windows_performance_counter" and event["operation_input_digest"]==row["input_digest"]
                    and event["expires_monotonic_ns"]==expiry and event["observed_monotonic_ns"]<expiry
                    and identity["process_id"]==pid and identity["native_binary_sha256"]==image_sha
                    and identity["input_sha256"]==source and identity["profile"]==env["AIRK_COMPACT_CUTLASS_TYPED_GRAPH"]):
                raise ValueError("warm operation source, order, clock or deadline differs")
        if not (start["started_monotonic_ns"]==start["observed_monotonic_ns"]==stop["started_monotonic_ns"]
                and stop["observed_monotonic_ns"]>=start["observed_monotonic_ns"]
                and start["native_call_elapsed_us"] is None and stop["native_call_elapsed_us"]==row["native_call_us"]
                and stop["native_success"] is True):
            raise ValueError("missing synchronous native operation closure")


def summarize(mode, rows, expected_words):
    if mode=="pressure-fit":
        schedule=[(8192,0,"sample")]
    elif mode in ("fixed","pressure-fixed"):
        schedule=[(8192,0,"warmup")]+[(8192,0,"sample")]*20
    elif mode=="mixed":
        schedule=[(width,cycle,"sample") for cycle in (1,2,3) for width in (32,128,1024,8192,32)]
    else:
        raise ValueError("unrecognized protected timing schedule")
    durations=[]
    previous=0
    for (width,cycle,kind),row in zip(schedule,rows,strict=True):
        if (row["tokens"],row["cycle"],row["kind"])!=(width,cycle,kind):
            raise ValueError("unexpected timing order")
        t=row["native_call_us"]
        if (words(row["raw_scores"])!=expected_words[WIDTHS.index(width)] or type(t) is not int or not 0<t<=120_000_000
                or not row["forward_synchronized"] or not previous<=row["start_us"]<row["finish_us"]
                or not row["start_utc_ns"]<=row["finish_utc_ns"]):
            raise ValueError("original score words, synchronization or native clock changed")
        previous=row["finish_us"]
        if kind=="sample":
            durations.append(t)
    ordered=sorted(durations)
    return {"samples":len(ordered),"warmup":len(rows)-len(ordered),"p50_us":ordered[math.ceil(len(ordered)*.50)-1],
            "p95_us":ordered[math.ceil(len(ordered)*.95)-1],"max_us":ordered[-1],
            "measurement":"unprofiled synchronized scientific session","service_answer_p95":False}
