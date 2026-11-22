# SPDX-License-Identifier: Apache-2.0
"""The model gates of plan section 12.7: tests 1 to 4, a concurrency driver and the precision check.

Gates (plan 12.7.1 Large against Compact, 12.7.2 Focus only against Focus with
Compact, 12.7.3 FP32 against FP16 per stage, replays only). Arms are spec
entries (``make_loop_spec.py gate``) with an ``arm`` letter and an
``arm_check`` (``loop_reply.check_arm``): F is Focus only, C is Compact on
CUDA, L is Large on CUDA, and the precision arms name a precision.

Subcommands, one output file each (all private, under ``R``):

* ``replays``: tests 1 and 2 without agent sessions. One canonical first Answer
  /3 call per scenario on the live arm (the instruction verbatim, the spec's
  budget), its arm check, its first-reply evidence (essential and supporting
  notes, bytes, sub-needs; ``coverage.first_reply``), where it holds each note
  (``depth.answer_attribution``) and the loop summary. GPU memory is sampled
  during the arm (read-only ``nvidia-smi``) for its peak. Writes
  ``arm-replays.json``.
* ``agreement``: test 1 between two arms' replays. Current decisions are
  aligned by (stage, part, need, round). A pair is ``agree`` or ``differ``
  when both arms were shown the same cards (equal ``roster_digest``),
  ``diverged`` when the rosters differ (an earlier choice or other evidence
  changed what was presented), and ``only_a`` or ``only_b`` when one arm has
  no such decision (not run, or omitted by the frame). Where a pair differs,
  the reference notes favour the arm whose evidence for that part and
  sub-need covers more essential notes, then more supporting notes.
* ``outcomes``: test 3 from a graded pass. Correct finishes, turns, Rubric A
  wins and losses of arm B against arm A (``grade.arm_compare``), Rubric B,
  and the close calls without a second session, written as a jobs file for
  ``run_pass.py --stage second --second-jobs``. Where a second session exists,
  a scenario whose two samples disagree is ``undecided``.
* ``concurrency``: test 4's throughput. The scenarios' canonical first calls at
  2 and 4 concurrent Answers (queueing of 12.4.1; tickets are attached), with
  wall time, Answers per minute, tickets, partials and the GPU memory peak.
* ``performance``: test 4 from the arms' replays, the concurrency runs and the
  switch receipts (``run_plan.py`` ``switch_context``): end-to-end time, model
  time per call, rounds, partials at the deadline, peak memory, warm and
  reload time.
* ``precision``: 12.7.3 between an FP32 and an FP16 arm of one stage. Top-choice
  flips by stage and decision type, which choice the notes favour, essential
  and supporting notes, per-call time and peak memory, and the escalation
  rule: an FP32 arm gets judged sessions only if more than 5% of that stage's
  decisions flip or the essential notes covered differ on any scenario.

What a reply cannot show is stated, not guessed: Focus's own ranking lives in
the decision log's Focus receipts (service custody), so a Focus flip is read
from its consequences (a decision whose presented basis or choice changed, and
a changed evidence digest). Per-stage model time is not in the reply either;
the summary's model timings are summed over receipts.

``python -I -B gate.py replays --spec S --arm C --out <R>/passes/<series>/gate/C``
``python -I -B gate.py agreement --a <dir A> --b <dir B> --out <file>``
``python -I -B gate.py outcomes --pass-dir <R>/passes/<series> --a compact-gpu --b large-gpu --out <file>``
``python -I -B gate.py concurrency --spec S --arm C --levels 2,4 --out <dir>``
``python -I -B gate.py performance --arm-dir C=<dir> --arm-dir L=<dir> [--concurrency C=<dir>] [--switch <receipt>] --out <file>``
``python -I -B gate.py precision --fp32 <dir> --fp16 <dir> --stage compact --out <file>``
"""

from __future__ import annotations

import argparse
import concurrent.futures
import statistics
import subprocess
import sys
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import coverage  # noqa: E402
import depth  # noqa: E402
import evidence  # noqa: E402
import grade  # noqa: E402
import loop_reply  # noqa: E402
import replay  # noqa: E402

GATE_SCHEMA = "airk.private.answer-eval-gate/1"
ESCALATION_FLIP_SHARE = 0.05
PRECISION_STAGES = ("focus", "compact", "large")


# ---------------------------------------------------------------- GPU memory (read-only)


def nvidia_smi_used() -> dict[str, int] | None:
    """Used MiB per GPU, from a read-only ``nvidia-smi`` query; None when it cannot answer."""
    try:
        out = subprocess.run(["nvidia-smi", "--query-gpu=index,memory.used", "--format=csv,noheader,nounits"],
                             capture_output=True, text=True, timeout=20, check=True,
                             creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0)).stdout
        rows = [line.split(",") for line in out.strip().splitlines() if line.strip()]
        return {index.strip(): int(used.strip()) for index, used in rows}
    except (OSError, subprocess.SubprocessError, ValueError):
        return None


def nvidia_smi_memory() -> dict[str, dict[str, int]] | None:
    """Total and used MiB per GPU (read-only ``nvidia-smi``); None when it cannot answer."""
    try:
        out = subprocess.run(["nvidia-smi", "--query-gpu=index,memory.total,memory.used",
                              "--format=csv,noheader,nounits"], capture_output=True, text=True, timeout=20,
                             check=True, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0)).stdout
        rows = [line.split(",") for line in out.strip().splitlines() if line.strip()]
        return {i.strip(): {"total_mib": int(t.strip()), "used_mib": int(u.strip())} for i, t, u in rows}
    except (OSError, subprocess.SubprocessError, ValueError):
        return None


def host_available_bytes() -> int | None:
    """The OS's available physical memory (Windows GlobalMemoryStatusEx, else /proc/meminfo); None if unknown."""
    try:
        if sys.platform == "win32":
            import ctypes

            class Status(ctypes.Structure):
                _fields_ = [("dwLength", ctypes.c_ulong), ("dwMemoryLoad", ctypes.c_ulong),
                            ("ullTotalPhys", ctypes.c_ulonglong), ("ullAvailPhys", ctypes.c_ulonglong),
                            ("ullTotalPageFile", ctypes.c_ulonglong), ("ullAvailPageFile", ctypes.c_ulonglong),
                            ("ullTotalVirtual", ctypes.c_ulonglong), ("ullAvailVirtual", ctypes.c_ulonglong),
                            ("ullAvailExtendedVirtual", ctypes.c_ulonglong)]
            status = Status()
            status.dwLength = ctypes.sizeof(Status)
            return int(status.ullAvailPhys) if ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)) else None
        for line in Path("/proc/meminfo").read_text().splitlines():
            if line.startswith("MemAvailable:"):
                return int(line.split()[1]) * 1024
    except (OSError, ValueError, AttributeError):
        return None
    return None


class MemorySampler:
    """Samples device memory in a thread while an arm runs and keeps the peak per GPU. Device-wide: the peak
    includes Focus and anything else resident, which the report says."""

    def __init__(self, query=nvidia_smi_used, interval_s: float = 0.5):
        self.query, self.interval_s = query, interval_s
        self.peak: dict[str, int] = {}
        self.samples = 0
        self.failures = 0
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None

    def sample(self) -> None:
        used = self.query()
        if used is None:
            self.failures += 1
            return
        self.samples += 1
        for gpu, mib in used.items():
            self.peak[gpu] = max(self.peak.get(gpu, 0), mib)

    def _run(self):
        while not self._stop.is_set():
            self.sample()
            self._stop.wait(self.interval_s)

    def __enter__(self):
        self.sample()
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()
        return self

    def __exit__(self, *exc):
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=30)
        self.sample()
        return False

    def record(self) -> dict:
        return {"peak_used_mib": dict(self.peak), "samples": self.samples, "failures": self.failures,
                "scope": "device-wide (nvidia-smi memory.used)"}


# ---------------------------------------------------------------- spec and arms


def spec_arm(spec: dict, arm: str) -> dict:
    found = [e for e in spec.get("results", []) if e.get("arm") == arm]
    common.require(len(found) == 1, f"the spec has {len(found)} entries for arm {arm!r}")
    entry = dict(found[0])
    if spec.get("answer_defaults") is not None and "answer_defaults" not in entry:
        entry["answer_defaults"] = spec["answer_defaults"]
    entry.setdefault("handle", spec.get("handle"))
    return entry


def spec_scenarios(spec: dict, r_root: Path) -> list[dict]:
    scenarios_dir = Path(spec.get("scenarios_dir") or r_root / "scenarios")
    files = common.scenario_files(scenarios_dir, spec.get("repos", common.REPOS), spec.get("slots", common.SLOTS))
    scenarios = [common.load_scenario(f) for f in files]
    if spec.get("scenario_ids"):
        scenarios = [s for s in scenarios if s["id"] in set(spec["scenario_ids"])]
    if spec.get("freeze"):
        freeze = Path(spec["freeze"])
        frozen = common.read_json(freeze)["files"]
        for scenario in scenarios:
            rel = Path(scenario["_path"]).resolve().relative_to(freeze.resolve().parent).as_posix()
            common.require(frozen.get(rel) == scenario["_sha256"], f"scenario {rel} differs from the freeze")
    common.require(scenarios, "no scenarios")
    return scenarios


def reply_row(scenario: dict, call: dict) -> dict:
    """One arm's first reply on one scenario, as the gate reads it. Evidence text is not copied."""
    body = None
    if call.get("ok") and call.get("result_path") and Path(call["result_path"]).is_file():
        body = common.raw_body(Path(call["result_path"]).read_bytes())
    row = {"scenario_id": scenario["id"], "repo": scenario["repository"], "slot": scenario["slot"],
           "kind": scenario.get("kind"), "ok": bool(call.get("ok")), "error": call.get("error"),
           "elapsed_ms": call.get("elapsed_ms"), "final_attempt_ms": call.get("final_attempt_ms"),
           "bytes": call.get("bytes"), "digest": call.get("digest"), "result_path": call.get("result_path"),
           "result_sha256": call.get("result_sha256"), "tickets": call.get("tickets") or [],
           "typed_retries": call.get("typed_retries"), "v3": loop_reply.is_v3(body)}
    row["first_reply"] = coverage.first_reply(scenario, body, call.get("bytes"))
    if loop_reply.is_v3(body):
        row["loop"] = loop_reply.summary(body)
        row["attribution"] = depth.answer_attribution(scenario, body)
    return row


def arm_replays(spec: dict, arm: str, r_root: Path, out: Path, *, handle=None, backend=None, sampler=None,
                sleep=time.sleep) -> dict:
    """Tests 1 and 2 for one arm: canonical first calls on every scenario, sequentially, with arm checks."""
    entry = spec_arm(spec, arm)
    scenarios = spec_scenarios(spec, r_root)
    if handle is None:
        handle = common.load_handle(common.resolve_handle(entry["handle"], r_root / "fixture"),
                                    accept_queryable=bool(spec.get("accept_queryable")))
    sampler = sampler or MemorySampler()
    rows = []
    began = time.monotonic()
    with sampler:
        for scenario in scenarios:
            result = replay.replay_canonical(scenario, entry["path"], handle, out / scenario["id"],
                                             answer_defaults=entry.get("answer_defaults"),
                                             timeout_s=float(entry.get("call_timeout", 600)), backend=backend,
                                             sleep=sleep)
            row = reply_row(scenario, result["call"])
            body = common.raw_body(Path(row["result_path"]).read_bytes()) if row["ok"] else None
            row["arm_problems"] = loop_reply.check_arm(body, entry.get("arm_check") or {}) if row["ok"] else [
                "call failed"]
            rows.append(row)
            print(f"{arm} {scenario['id']}: essential {len(row['first_reply']['essential_covered'])}/"
                  f"{row['first_reply']['essential_total']}, {row['elapsed_ms']} ms, problems {len(row['arm_problems'])}",
                  flush=True)
    document = {"schema": GATE_SCHEMA, "kind": "arm-replays", "generated": common.now_iso(), "arm": arm,
                "configuration": entry["configuration"], "path": entry["path"], "device": entry.get("device"),
                "stages": entry.get("stages"), "arm_check": entry.get("arm_check"),
                "precision_stage": entry.get("precision_stage"), "answer_defaults": entry.get("answer_defaults"),
                "handle_sha256": getattr(handle, "sha256", None), "coverage_rule": evidence.COVERAGE_RULE,
                "elapsed_s": round(time.monotonic() - began, 1), "memory": sampler.record(), "rows": rows,
                "arm_ok": all(not r["arm_problems"] for r in rows)}
    common.write_json(out / "arm-replays.json", document)
    return document


def load_arm(path: Path) -> dict:
    path = Path(path)
    document = common.read_json(path / "arm-replays.json" if path.is_dir() else path)
    common.require(document.get("schema") == GATE_SCHEMA and document.get("kind") == "arm-replays",
                   f"{path} is not an arm-replays document")
    return document


def _body(row: dict):
    if not row.get("ok") or not row.get("result_path") or not Path(row["result_path"]).is_file():
        return None
    return common.raw_body(Path(row["result_path"]).read_bytes())


# ---------------------------------------------------------------- test 1: decision agreement


def _note_score(spans: list[evidence.Span], scenario: dict) -> tuple[int, int]:
    covered = set(evidence.covered_notes(spans, scenario["notes"]))
    essential = sum(1 for n in scenario["notes"] if n["mark"] == "essential" and n["id"] in covered)
    supporting = sum(1 for n in scenario["notes"] if n["mark"] != "essential" and n["id"] in covered)
    return essential, supporting


def _scope_spans(body, part, need) -> list[evidence.Span]:
    """The evidence behind a decision: its sub-need's, else its part's, else the whole reply's."""
    per_need = loop_reply.sub_need_spans(body)
    if part is not None and need is not None and (part, need) in per_need:
        return per_need[(part, need)]
    if part is not None:
        spans = [s for (p, _n), v in per_need.items() if p == part for s in v]
        if spans:
            return spans
    return evidence.extract(body) if body is not None else []


def favour(scenario: dict, body_a, body_b, part, need) -> str:
    a = _note_score(_scope_spans(body_a, part, need), scenario)
    b = _note_score(_scope_spans(body_b, part, need), scenario)
    return "a" if a > b else "b" if b > a else "neither"


def compare_decisions(scenario: dict, body_a, body_b, basis_is_choice: bool = False) -> dict:
    """Align two replies' current decisions. With ``basis_is_choice`` (the Focus precision check) a changed
    presented basis also counts as a different choice, because Focus's choice is what was presented."""
    da, db = loop_reply.current_decisions(body_a), loop_reply.current_decisions(body_b)
    basis_a, basis_b = _basis_by_key(body_a), _basis_by_key(body_b)
    pairs = []
    for key in sorted(set(da) | set(db), key=lambda k: tuple("" if v is None else str(v) for v in k)):
        a, b = da.get(key), db.get(key)
        stage, part, need, round_ = key
        if a is None or b is None:
            status = "only_a" if b is None else "only_b"
        elif a["roster"] != b["roster"]:
            status = "diverged"
        elif a["chosen"] != b["chosen"] or (basis_is_choice and basis_a.get(key) != basis_b.get(key)):
            status = "differ"
        else:
            status = "agree"
        pair = {"stage": stage, "part": part, "need": need, "round": round_, "status": status,
                "a": None if a is None else {"chosen": a["chosen"], "source": a["source"], "margin": a["margin"]},
                "b": None if b is None else {"chosen": b["chosen"], "source": b["source"], "margin": b["margin"]}}
        if status in ("differ", "diverged"):
            pair["favoured"] = favour(scenario, body_a, body_b, part, need)
        pairs.append(pair)
    return {"pairs": pairs, "counts": _status_counts(pairs)}


def _basis_by_key(body) -> dict:
    out = {}
    if not loop_reply.is_v3(body):
        return out
    for raw in body.get("decisions") or []:
        if isinstance(raw, dict):
            out[loop_reply.decision_key(raw)] = (raw.get("basis") or {}).get("digest")
    return out


def _status_counts(pairs: list[dict]) -> dict:
    counts: dict[str, dict[str, int]] = {}
    for pair in pairs:
        stage = counts.setdefault(str(pair["stage"]), {})
        stage[pair["status"]] = stage.get(pair["status"], 0) + 1
        if pair.get("favoured"):
            stage[f"favoured_{pair['favoured']}"] = stage.get(f"favoured_{pair['favoured']}", 0) + 1
    return counts


def _loop_views(body) -> dict:
    """Per-part views of a reply that test 1 also compares: sub-need texts, coverage verdicts, follow-ups."""
    out = {"parts": [], "sub_needs": {}, "coverage": {}, "follow_ups": {}}
    if not loop_reply.is_v3(body):
        return out
    for part in body.get("parts") or []:
        if not isinstance(part, dict):
            continue
        pid = part.get("id")
        out["parts"].append([(part.get("span") or {}).get("utf8_start"), (part.get("span") or {}).get("utf8_end")])
        out["sub_needs"][pid] = sorted(str(n.get("text")) for n in part.get("sub_needs") or [] if isinstance(n, dict))
        out["coverage"][pid] = [n.get("coverage") for n in part.get("sub_needs") or [] if isinstance(n, dict)]
        out["follow_ups"][pid] = [[f.get("operation"), f.get("target")] for r in part.get("rounds") or []
                                  if isinstance(r, dict) for f in r.get("follow_ups_run") or [] if isinstance(f, dict)]
    return out


def agreement(doc_a: dict, doc_b: dict, scenarios: dict[str, dict], basis_is_choice: bool = False) -> dict:
    by_a = {r["scenario_id"]: r for r in doc_a["rows"]}
    by_b = {r["scenario_id"]: r for r in doc_b["rows"]}
    rows, totals = [], {}
    for sid in sorted(set(by_a) & set(by_b)):
        scenario = scenarios[sid]
        body_a, body_b = _body(by_a[sid]), _body(by_b[sid])
        compared = compare_decisions(scenario, body_a, body_b, basis_is_choice)
        va, vb = _loop_views(body_a), _loop_views(body_b)
        same_split = va["parts"] == vb["parts"]
        fa, fb = by_a[sid]["first_reply"], by_b[sid]["first_reply"]
        score_a = (len(fa["essential_covered"]), len(fa["supporting_covered"]))
        score_b = (len(fb["essential_covered"]), len(fb["supporting_covered"]))
        row = {"scenario_id": sid, "repo": scenario["repository"], "same_split": same_split,
               "same_sub_needs": same_split and va["sub_needs"] == vb["sub_needs"],
               "same_coverage_verdicts": same_split and va["coverage"] == vb["coverage"],
               "same_follow_ups": same_split and va["follow_ups"] == vb["follow_ups"],
               "same_evidence": by_a[sid].get("digest") == by_b[sid].get("digest"),
               "decision_counts": compared["counts"], "pairs": compared["pairs"],
               "omitted_decisions": [(by_a[sid].get("loop") or {}).get("decisions_omitted"),
                                     (by_b[sid].get("loop") or {}).get("decisions_omitted")],
               "scenario_favoured": "a" if score_a > score_b else "b" if score_b > score_a else "neither"}
        rows.append(row)
        for stage, counts in compared["counts"].items():
            bucket = totals.setdefault(stage, {})
            for key, value in counts.items():
                bucket[key] = bucket.get(key, 0) + value
    for counts in totals.values():
        comparable = counts.get("agree", 0) + counts.get("differ", 0)
        counts["agreement_share"] = round(counts.get("agree", 0) / comparable, 4) if comparable else None
    views = ("same_split", "same_sub_needs", "same_coverage_verdicts", "same_follow_ups", "same_evidence")
    return {"schema": GATE_SCHEMA, "kind": "agreement", "generated": common.now_iso(),
            "a": {"arm": doc_a["arm"], "configuration": doc_a["configuration"]},
            "b": {"arm": doc_b["arm"], "configuration": doc_b["configuration"]},
            "scenarios": len(rows), "by_stage": totals,
            "views": {v: sum(1 for r in rows if r[v]) for v in views},
            "scenario_favoured": {k: sum(1 for r in rows if r["scenario_favoured"] == k) for k in ("a", "b", "neither")},
            "rows": rows}


# ---------------------------------------------------------------- test 3: judged sessions


def _verdict_reasons(row_index: dict) -> list[dict]:
    out = []
    for path in row_index.get("judges", []):
        path = Path(path)
        record = common.read_json(path) if path.is_file() else None
        verdict = (record or {}).get("verdict") if (record or {}).get("status") == "valid" else None
        if verdict is None:
            out.append({"judge": path.parent.name, "valid": False})
            continue
        out.append({"judge": path.parent.name, "valid": True,
                    "notes": [{"id": n.get("id"), "verdict": n.get("verdict"), "reason": n.get("reason")}
                              for n in verdict.get("notes") or [] if isinstance(n, dict)],
                    "misleading_followed": verdict.get("misleading_followed"),
                    "misleading_reason": verdict.get("misleading_reason"),
                    "rubric_b": {k: v for k, v in (verdict.get("rubric_b") or {}).items() if isinstance(v, dict)}})
    return out


def outcomes(pass_dir: Path, a_name: str, b_name: str) -> dict:
    pass_dir = Path(pass_dir)
    index = common.read_json(pass_dir / "pass-index.json")
    adj_path = pass_dir / "adjudications.json"
    rows = grade.build_rows(index, common.read_json(adj_path) if adj_path.is_file() else {})
    name = lambda r: f"{r['configuration']}-{r['device']}"  # noqa: E731
    scen = lambda r: f"{r['repo']}-{r['slot']}"  # noqa: E731
    found = {name(r) for r in rows}
    common.require(a_name in found and b_name in found, f"{pass_dir} lacks {a_name} or {b_name}; has {sorted(found)}")
    first = {n: {scen(r): r for r in rows if name(r) == n and r.get("sample", 1) == 1} for n in (a_name, b_name)}
    second = {n: {scen(r): r for r in rows if name(r) == n and r.get("sample", 1) == 2} for n in (a_name, b_name)}
    compared = grade.arm_compare(first[a_name], first[b_name])
    index_rows = {r["key"]: r for r in index["results"]}
    jobs = []
    for row in compared["scenarios"]:
        sid = row["scenario"]
        if sid in second[a_name] or sid in second[b_name]:
            a2 = second[a_name].get(sid, first[a_name].get(sid))
            b2 = second[b_name].get(sid, first[b_name].get(sid))
            row["second_winner"] = grade.rubric_a_winner(a2, b2)
            if row["second_winner"] != row["winner"]:
                row["winner"] = "undecided"
        elif row["close_call"]:
            jobs += [[sid, a_name], [sid, b_name]]
        for side, n in (("a", a_name), ("b", b_name)):
            source = first[n].get(sid)
            row[f"{side}_judging"] = _verdict_reasons(index_rows.get(source["key"], {})) if source else []
    for key in ("b_wins", "b_losses", "ties", "pending"):
        compared[key] = sum(1 for p in compared["scenarios"] if p["winner"] == {"b_wins": "b", "b_losses": "a",
                                                                                   "ties": "tie", "pending": "pending"}[key])
    compared["undecided"] = sum(1 for p in compared["scenarios"] if p["winner"] == "undecided")
    return {"schema": GATE_SCHEMA, "kind": "outcomes", "generated": common.now_iso(), "pass_dir": str(pass_dir),
            "series": index.get("series"), "a": a_name, "b": b_name, **compared, "second_sessions": jobs}


# ---------------------------------------------------------------- test 4: concurrency and performance


def concurrency(spec: dict, arm: str, r_root: Path, out: Path, levels: list[int], *, handle=None, backend=None,
                sampler_factory=MemorySampler, sleep=time.sleep) -> dict:
    """Throughput with N concurrent Answers (plan 12.7.1 test 4): every scenario's canonical first call, N at a
    time. Tickets are attached as in ``replay.issue``, so a queued Answer counts once, with its wait."""
    entry = spec_arm(spec, arm)
    scenarios = spec_scenarios(spec, r_root)
    if handle is None:
        handle = common.load_handle(common.resolve_handle(entry["handle"], r_root / "fixture"),
                                    accept_queryable=bool(spec.get("accept_queryable")))
    runs = []
    for level in levels:
        common.require(level >= 1, "a concurrency level is at least 1")
        sampler = sampler_factory()
        began = time.monotonic()
        with sampler, concurrent.futures.ThreadPoolExecutor(max_workers=level) as pool:
            futures = {pool.submit(replay.replay_canonical, s, entry["path"], handle, out / f"n{level}" / s["id"],
                                   answer_defaults=entry.get("answer_defaults"),
                                   timeout_s=float(entry.get("call_timeout", 600)), backend=backend, sleep=sleep): s
                       for s in scenarios}
            calls = [reply_row(futures[f], f.result()["call"]) for f in concurrent.futures.as_completed(futures)]
        wall_s = time.monotonic() - began
        elapsed = [c["elapsed_ms"] for c in calls if c["ok"] and c["elapsed_ms"] is not None]
        runs.append({"level": level, "answers": len(calls), "ok": sum(1 for c in calls if c["ok"]),
                     "wall_s": round(wall_s, 3),
                     "answers_per_minute": round(len(calls) / wall_s * 60, 3) if wall_s > 0 else None,
                     "elapsed_ms_median": statistics.median(elapsed) if elapsed else None,
                     "elapsed_ms_max": max(elapsed) if elapsed else None,
                     "tickets": sum(len(c["tickets"]) for c in calls),
                     "partials": _partials(calls), "memory": sampler.record(),
                     "calls": sorted(({k: c[k] for k in ("scenario_id", "ok", "elapsed_ms", "final_attempt_ms",
                                                         "bytes", "digest", "tickets")} for c in calls),
                                     key=lambda c: c["scenario_id"])})
    document = {"schema": GATE_SCHEMA, "kind": "concurrency", "generated": common.now_iso(), "arm": arm,
                "configuration": entry["configuration"], "levels": levels, "runs": runs}
    common.write_json(out / "concurrency.json", document)
    return document


def _partials(rows: list[dict]) -> dict:
    found: dict[str, int] = {}
    for row in rows:
        reason = ((row.get("loop") or {}).get("partial") or {}).get("reason")
        if reason:
            found[reason] = found.get(reason, 0) + 1
    return found


def _quantile(values: list[float], q: float):
    if not values:
        return None
    values = sorted(values)
    return values[min(len(values) - 1, max(0, int(round(q * (len(values) - 1)))))]


def arm_performance(doc: dict) -> dict:
    rows = [r for r in doc["rows"] if r["ok"]]
    elapsed = [r["elapsed_ms"] for r in rows if r.get("elapsed_ms") is not None]
    loops = [r["loop"] for r in rows if r.get("loop")]
    inference_us = [(l["model_timings"] or {}).get("inference_us") for l in loops]
    calls = [(l.get("card_calls") or 0) + (l.get("focus_calls") or 0) for l in loops]
    per_call_ms = [i / c / 1000 for i, c in zip(inference_us, calls) if i is not None and c]
    rounds = [l["work"].get("rounds") for l in loops if l["work"].get("rounds") is not None]
    return {"arm": doc["arm"], "configuration": doc["configuration"], "answers": len(doc["rows"]), "ok": len(rows),
            "elapsed_ms_median": statistics.median(elapsed) if elapsed else None,
            "elapsed_ms_p90": _quantile(elapsed, 0.9), "elapsed_ms_max": max(elapsed) if elapsed else None,
            "model_ms_per_call_median": round(statistics.median(per_call_ms), 3) if per_call_ms else None,
            "model_inference_ms_total": round(sum(i for i in inference_us if i) / 1000, 3) if inference_us else None,
            "card_calls_total": sum(l.get("card_calls") or 0 for l in loops),
            "focus_calls_total": sum(l.get("focus_calls") or 0 for l in loops),
            "rounds": {str(k): rounds.count(k) for k in sorted(set(rounds))},
            "partials": _partials(rows), "partial_at_deadline": _partials(rows).get("deadline", 0),
            "tickets": sum(len(r.get("tickets") or []) for r in rows),
            "peak_used_mib": (doc.get("memory") or {}).get("peak_used_mib"),
            "per_stage_model_time": "not in the reply (decision log, service custody)"}


def performance(arms: dict[str, dict], concurrency_docs: dict[str, dict], switches: list[dict]) -> dict:
    out = {"schema": GATE_SCHEMA, "kind": "performance", "generated": common.now_iso(),
           "arms": {name: arm_performance(doc) for name, doc in sorted(arms.items())},
           "concurrency": {name: [{k: run[k] for k in ("level", "answers", "ok", "wall_s", "answers_per_minute",
                                                       "elapsed_ms_median", "elapsed_ms_max", "tickets", "partials")}
                                  | {"peak_used_mib": run["memory"].get("peak_used_mib")} for run in doc["runs"]]
                           for name, doc in sorted(concurrency_docs.items())},
           "switches": [{k: s.get(k) for k in ("from", "to", "release_s", "warm_s", "total_s", "memory_before",
                                               "memory_after", "ok")} for s in switches]}
    return out


# ---------------------------------------------------------------- 12.7.3 precision


def precision(doc32: dict, doc16: dict, stage: str, scenarios: dict[str, dict]) -> dict:
    """FP32 against FP16 for one stage, replays only. Flips are decisions with the same presented cards and a
    different top choice (for Focus, also a different presented basis). Escalation per plan 12.7.3."""
    common.require(stage in PRECISION_STAGES, f"unknown precision stage {stage!r}")
    compared = agreement(doc16, doc32, scenarios, basis_is_choice=(stage == "focus"))
    by_type: dict[str, dict[str, int]] = {}
    favoured = {"fp16": 0, "fp32": 0, "neither": 0}
    for row in compared["rows"]:
        for pair in row["pairs"]:
            bucket = by_type.setdefault(str(pair["stage"]), {"decisions": 0, "flips": 0, "diverged": 0, "unpaired": 0})
            bucket["decisions"] += 1
            if pair["status"] == "differ":
                bucket["flips"] += 1
            elif pair["status"] == "diverged":
                bucket["diverged"] += 1
            elif pair["status"] in ("only_a", "only_b"):
                bucket["unpaired"] += 1
            if pair.get("favoured"):
                favoured[{"a": "fp16", "b": "fp32", "neither": "neither"}[pair["favoured"]]] += 1
    decisions = sum(b["decisions"] for b in by_type.values())
    flips = sum(b["flips"] for b in by_type.values())
    share = flips / decisions if decisions else 0.0
    by16 = {r["scenario_id"]: r for r in doc16["rows"]}
    by32 = {r["scenario_id"]: r for r in doc32["rows"]}
    evidence_rows = []
    for sid in sorted(set(by16) & set(by32)):
        e16, e32 = by16[sid]["first_reply"], by32[sid]["first_reply"]
        evidence_rows.append({"scenario_id": sid, "essential_fp16": e16["essential_covered"],
                              "essential_fp32": e32["essential_covered"],
                              "supporting_fp16": len(e16["supporting_covered"]),
                              "supporting_fp32": len(e32["supporting_covered"]),
                              "essential_differs": sorted(e16["essential_covered"]) != sorted(e32["essential_covered"]),
                              "same_evidence": by16[sid].get("digest") == by32[sid].get("digest")})
    essential_differs = [r["scenario_id"] for r in evidence_rows if r["essential_differs"]]
    reasons = []
    if share > ESCALATION_FLIP_SHARE:
        reasons.append(f"top choice differs on {flips} of {decisions} decisions ({share:.1%} > 5%)")
    if essential_differs:
        reasons.append(f"essential notes covered differ on {len(essential_differs)} scenarios")
    return {"schema": GATE_SCHEMA, "kind": "precision", "generated": common.now_iso(), "stage": stage,
            "fp16": {"arm": doc16["arm"], "configuration": doc16["configuration"],
                     "performance": arm_performance(doc16)},
            "fp32": {"arm": doc32["arm"], "configuration": doc32["configuration"],
                     "performance": arm_performance(doc32)},
            "decisions": decisions, "flips": flips, "flip_share": round(share, 4), "by_decision_type": by_type,
            "favoured": favoured, "evidence": evidence_rows, "essential_differs": essential_differs,
            "escalate": bool(reasons), "escalation_reasons": reasons,
            "arm_ok": bool(doc16.get("arm_ok")) and bool(doc32.get("arm_ok"))}


# ---------------------------------------------------------------- CLI


def _pairs(values: list[str] | None) -> dict[str, Path]:
    out = {}
    for value in values or []:
        name, _, path = value.partition("=")
        common.require(bool(name and path), f"expected NAME=PATH, got {value!r}")
        out[name] = Path(path)
    return out


def _scenario_map(spec_path: Path | None, r_root: Path, docs: list[dict]) -> dict[str, dict]:
    if spec_path is not None:
        return {s["id"]: s for s in spec_scenarios(common.read_json(spec_path), r_root)}
    found = {}
    for path in common.scenario_files(r_root / "scenarios"):
        scenario = common.load_scenario(path)
        found[scenario["id"]] = scenario
    wanted = {r["scenario_id"] for d in docs for r in d["rows"]}
    return {k: v for k, v in found.items() if k in wanted}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--r-root", type=Path, default=common.DEFAULT_R)
    commands = parser.add_subparsers(dest="command", required=True)
    sub = commands.add_parser("replays")
    sub.add_argument("--spec", type=Path, required=True)
    sub.add_argument("--arm", required=True)
    sub.add_argument("--out", type=Path, required=True)
    sub = commands.add_parser("agreement")
    sub.add_argument("--a", type=Path, required=True)
    sub.add_argument("--b", type=Path, required=True)
    sub.add_argument("--spec", type=Path)
    sub.add_argument("--out", type=Path, required=True)
    sub = commands.add_parser("outcomes")
    sub.add_argument("--pass-dir", type=Path, required=True)
    sub.add_argument("--a", required=True, help="result name of arm A, e.g. compact-gpu")
    sub.add_argument("--b", required=True, help="result name of arm B, e.g. large-gpu")
    sub.add_argument("--out", type=Path, required=True)
    sub.add_argument("--jobs-out", type=Path, help="default: <out stem>.second-jobs.json")
    sub = commands.add_parser("concurrency")
    sub.add_argument("--spec", type=Path, required=True)
    sub.add_argument("--arm", required=True)
    sub.add_argument("--levels", default="2,4")
    sub.add_argument("--out", type=Path, required=True)
    sub = commands.add_parser("performance")
    sub.add_argument("--arm-dir", action="append", required=True, help="NAME=<arm replays dir>")
    sub.add_argument("--concurrency", action="append", help="NAME=<concurrency dir>")
    sub.add_argument("--switch", action="append", type=Path, help="a switch_context receipt")
    sub.add_argument("--switch-dir", type=Path, help="a folder of switch_context receipts (*.json)")
    sub.add_argument("--out", type=Path, required=True)
    sub = commands.add_parser("precision")
    sub.add_argument("--fp32", type=Path, required=True)
    sub.add_argument("--fp16", type=Path, required=True)
    sub.add_argument("--stage", required=True, choices=PRECISION_STAGES)
    sub.add_argument("--spec", type=Path)
    sub.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.command == "replays":
        document = arm_replays(common.read_json(args.spec), args.arm, args.r_root, args.out)
        return 0 if document["arm_ok"] else 2
    if args.command == "agreement":
        a, b = load_arm(args.a), load_arm(args.b)
        result = agreement(a, b, _scenario_map(args.spec, args.r_root, [a, b]))
    elif args.command == "outcomes":
        result = outcomes(args.pass_dir, args.a, args.b)
        jobs = args.jobs_out or args.out.with_name(args.out.stem + ".second-jobs.json")
        common.write_json(jobs, {"schema": GATE_SCHEMA, "kind": "second-jobs", "second_sessions": result["second_sessions"]})
    elif args.command == "concurrency":
        concurrency(common.read_json(args.spec), args.arm, args.r_root, args.out,
                    [int(v) for v in args.levels.split(",") if v])
        return 0
    elif args.command == "performance":
        arms = {name: load_arm(path) for name, path in _pairs(args.arm_dir).items()}
        conc = {name: common.read_json(path / "concurrency.json" if path.is_dir() else path)
                for name, path in _pairs(args.concurrency).items()}
        receipts = list(args.switch or [])
        if args.switch_dir is not None and args.switch_dir.is_dir():
            receipts += sorted(args.switch_dir.glob("*.json"))
        result = performance(arms, conc, [common.read_json(p) for p in receipts])
    else:
        d32, d16 = load_arm(args.fp32), load_arm(args.fp16)
        result = precision(d32, d16, args.stage, _scenario_map(args.spec, args.r_root, [d32, d16]))
    common.write_json(args.out, result)
    print(args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
