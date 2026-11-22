# SPDX-License-Identifier: Apache-2.0
"""Orchestrate one pass: sessions, coverage, judging and grading (plan 3.3-3.5, 4, 5.2).

The pass spec (JSON) names the series, the scenario set and one entry per
result::

    {"series": "01-baseline",
     "scenarios_dir": "<R>/scenarios", "repos": ["airk", "bookbuilder"], "slots": [...],
     "scenario_ids": ["airk-C1", ...],         # optional subset (e.g. a pilot)
     "freeze": "<R>/scenario-freeze.json",     # optional: refuse scenarios that differ from the freeze
     "pilot": false,                           # true only for plumbing pilots (never judged evidence)
     "accept_queryable": false,                # pilot specs only: accept a queryable, not yet ready, handle
     "prompts_freeze": "<R>/design/prompts-freeze.json",   # required unless pilot; refuses a mismatch
     "handle": "<handle.json>",                # default handle for entries that name none
     "settle_repos": ["airk"],                 # indexes the settled guard checks (default: the scenarios')
     "concurrency": 4, "judge_concurrency": 4, "claude": null,
     "agent_model": "claude-haiku-5-5", "agent_effort": "medium",    # defaults; an entry may set model/effort
     "judge_model": "claude-sonnet-5-5", "judge_effort": "medium", "judges": ["j1", "j2"],
     "prior_series": [],                       # earlier passes whose sessions may be reused
     "results": [
       {"configuration": "parse", "path": "parse", "device": "none", "handle": "<handle.json>"},
       {"configuration": "search", "path": "search", "device": "none", "handle": "..."},
       {"configuration": "noModel", "path": "noModel", "device": "none", "handle": "...", "answer_defaults": {}},
       {"configuration": "focus", "path": "focus", "device": "cpu", "handle": "...", "slots": ["C1", ...],
        "reuse": true}
     ]}

Entries run in spec order; within an entry, scenarios run with bounded
parallelism (default 4). Before a session, the path's canonical first call is
replayed against the entry's handle and its evidence digest stored in
``digests.json``. If an earlier entry of this pass with the same tool set, or
the same result in a ``prior_series`` pass, has a completed session whose
digest is identical, the session is reused, not run (plan 3.3), and the index
records ``reused_from``. A discarded session (a tool outside the wrapper) is
rerun once, keeping the discarded attempt. Failures and timeouts are results.

Settled index (decision 1): before sessions or digests, every index the pass
reads must report semantic readiness ``ready``, served mode ``hybrid`` and no
index or vectors job queued or running (``settled.py``). The publication token
taken at the pass's first start must equal the token at every later start and
at every end. A changed token writes ``INVALID.json`` and the pass is invalid.
A pilot spec records the same snapshots without refusing.

Frozen harness (decision 5): a non-pilot spec names ``prompts_freeze``; the
agent and judge prompts and the harness file hashes must match it.

Everything resumes: a completed session, a valid verdict or a stored digest is
not redone. Sessions live in ``<R>/sessions/<series>/``; the pass index,
digests, judging, ``results.csv`` and ``summary.json`` in ``<R>/passes/<series>/``.

Stages: ``sessions``, ``coverage``, ``judge``, ``grade``, ``all`` (default), and
``second`` (run second sessions for the close calls listed in summary.json,
for the Answer configuration and the better of Parse and Search, then
coverage, judge and grade again). ``--only`` limits sessions to named results
(e.g. ``focus-cpu``), so a pass can follow fixture lifetimes.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import sys
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import coverage  # noqa: E402
import grade  # noqa: E402
import judge  # noqa: E402
import prompts_freeze  # noqa: E402
import replay  # noqa: E402
import run_session  # noqa: E402
import settled  # noqa: E402


class Pass:
    def __init__(self, spec: dict, r_root: Path, *, session_runner=None, judge_runner=None, digest_fn=None,
                 settle_fn=None, check_prompts: bool = True):
        self.spec = spec
        self.series = spec["series"]
        self.pilot = bool(spec.get("pilot"))
        common.require(self.pilot or not spec.get("accept_queryable"),
                       "accept_queryable is allowed in pilot specs only (decision 1)")
        for entry in spec.get("results", []):
            entry.setdefault("handle", spec.get("handle"))
            # Decision 5 (22 Nov 2026): a change set's Answer budget is a spec field; entries inherit it.
            if spec.get("answer_defaults") is not None and "answer_defaults" not in entry:
                entry["answer_defaults"] = spec["answer_defaults"]
            common.require(entry["handle"], f"entry {entry.get('configuration')} names no handle")
        self.prompts = None
        if spec.get("prompts_freeze"):
            self.prompts = prompts_freeze.verify(Path(spec["prompts_freeze"])) if check_prompts else None
        else:
            common.require(self.pilot, "a judged pass spec must pin prompts_freeze (decision 5)")
        self.settle_fn = settle_fn or self._settle
        self.r_root = Path(r_root)
        self.pass_dir = self.r_root / "passes" / self.series
        self.sessions_root = self.r_root / "sessions"
        self.pass_dir.mkdir(parents=True, exist_ok=True)
        scenarios_dir = Path(spec.get("scenarios_dir") or self.r_root / "scenarios")
        files = common.scenario_files(scenarios_dir, spec.get("repos", common.REPOS), spec.get("slots", common.SLOTS))
        self.scenarios = [common.load_scenario(f) for f in files]
        if spec.get("scenario_ids"):
            self.scenarios = [s for s in self.scenarios if s["id"] in set(spec["scenario_ids"])]
        common.require(self.scenarios, f"no scenarios under {scenarios_dir}")
        if spec.get("freeze"):
            # Frozen inputs (plan 4.5): every scenario must match the freeze file byte for byte.
            freeze_path = Path(spec["freeze"])
            frozen = common.read_json(freeze_path)["files"]
            for scenario in self.scenarios:
                rel = Path(scenario["_path"]).resolve().relative_to(freeze_path.resolve().parent).as_posix()
                common.require(frozen.get(rel) == scenario["_sha256"], f"scenario {rel} differs from the freeze")
            self.freeze = {"path": str(freeze_path), "sha256": common.sha256_file(freeze_path)}
        else:
            self.freeze = None
        self.lock = threading.Lock()
        self.digests_path = self.pass_dir / "digests.json"
        self.digests = common.read_json(self.digests_path) if self.digests_path.is_file() else {}
        self.session_runner = session_runner or run_session.run
        self.judge_runner = judge_runner or judge.run_judge
        self.digest_fn = digest_fn or self._digest
        self.claude = spec.get("claude") or None
        self.log_path = self.pass_dir / "pass-log.jsonl"

    # ------------------------------------------------------------ naming

    @staticmethod
    def result_name(entry: dict) -> str:
        return run_session.result_name(entry["configuration"], entry.get("device", "none"))

    def key(self, scenario: dict, entry: dict, sample: int = 1) -> str:
        suffix = "" if sample == 1 else f".s{sample}"
        return f"{scenario['repository']}-{scenario['slot']}/{self.result_name(entry)}{suffix}"

    def entry_scenarios(self, entry: dict) -> list[dict]:
        slots = entry.get("slots")
        repos = entry.get("repos")
        return [s for s in self.scenarios if (not slots or s["slot"] in slots) and (not repos or s["repository"] in repos)]

    def session_dir(self, scenario: dict, entry: dict, sample: int = 1, series: str | None = None) -> Path:
        return run_session.session_dir(self.sessions_root, series or self.series, scenario["repository"],
                                       scenario["slot"], self.result_name(entry), sample)

    def agent_model(self, entry: dict) -> str:
        return entry.get("model") or self.spec.get("agent_model") or common.AGENT_MODEL

    def agent_effort(self, entry: dict) -> str | None:
        return entry.get("effort") or self.spec.get("agent_effort") or common.AGENT_EFFORT

    def judges(self) -> list[str]:
        return list(self.spec.get("judges") or ["j1", "j2"])

    def check_image(self, entry: dict) -> None:
        """Refuse an image the spec does not allow (``require_images``: labels). Primary agent, 22 Nov 2026:
        an index with denies must run on an image with the filtered_paths fix, which images.json cannot show
        by itself, so the spec names the allowed images."""
        allowed = self.spec.get("require_images")
        if not allowed:
            return
        handle = common.load_handle(self.handle_of(entry), accept_queryable=bool(self.spec.get("accept_queryable")))
        label = (common.image_identity(common.read_json(handle.path).get("image")) or {}).get("label")
        common.require(label in allowed, f"image {label!r} is not one of the spec's require_images {allowed}")

    def check_lifetime(self, entry: dict) -> None:
        """The lifetime's service settings an entry needs (change set 2, 22 Nov 2026): the search switch
        (``require_search``) and the Answer layers per configuration (``layers_by_configuration``)."""
        raw = common.read_json(self.handle_of(entry))
        env = raw.get("service_environment") or {}
        if self.spec.get("require_search") is not None:
            live = env.get("AIRK_ARBITER_EVALUATION_SEARCH") or "none"
            common.require(live == self.spec["require_search"],
                           f"{self.result_name(entry)} needs search {self.spec['require_search']!r}, the lifetime has {live!r}")
        wanted = (self.spec.get("layers_by_configuration") or {}).get(entry["configuration"])
        if wanted is not None:
            live = env.get("AIRK_ARBITER_EVALUATION_ANSWER_LAYERS")
            common.require(live == wanted, f"{self.result_name(entry)} needs layers {wanted!r}, the lifetime has {live!r}")

    def check_stages(self, entry: dict) -> None:
        """Refuse to run an entry whose declared stages (``"stages": {"focus": ..., "context": ...}``) are not live."""
        if "stages" not in entry:
            return
        handle = common.load_handle(self.handle_of(entry), accept_queryable=bool(self.spec.get("accept_queryable")))
        live = handle.stages()
        wanted = {"focus": None, "context": None, **entry["stages"]}
        common.require(live == wanted, f"{self.result_name(entry)} needs stages {wanted}, the fixture has {live}")

    # ------------------------------------------------------------ settled index (decision 1)

    def handle_of(self, entry: dict) -> Path:
        """The entry's handle; ``@newest`` resolves to the newest fixture lifetime at each use."""
        return common.resolve_handle(entry["handle"], self.r_root / "fixture")

    def _settle(self, handle_path: str, repo: str) -> dict:
        handle = common.load_handle(Path(handle_path), accept_queryable=bool(self.spec.get("accept_queryable")))
        return settled.snapshot(handle, repo)

    def settle_targets(self, entries: list[dict]) -> list[tuple[str, str]]:
        repos = self.spec.get("settle_repos") or sorted({s["repository"] for s in self.scenarios})
        handles = sorted({str(self.handle_of(e)) for e in entries})
        return [(h, r) for h in handles for r in repos]

    def guard(self, phase: str, entries: list[dict]) -> list[dict]:
        """Snapshot every index at ``start`` or ``end``; refuse unsettled starts and changed tokens."""
        start_path = self.pass_dir / "settled-start.json"
        starts = common.read_json(start_path) if start_path.is_file() else {}
        snaps, problems = [], []
        for handle_path, repo in self.settle_targets(entries):
            snap = self.settle_fn(handle_path, repo)
            # A start may meet the service briefly unavailable (scoped_connection_unavailable while a stage
            # warms, cs1-airk 22 Nov 2026): the start snapshot is taken again, up to start_attempts times, and
            # sessions still start only on a settled snapshot.
            attempts = 1
            while phase == "start" and not (snap.get("settled") and snap.get("publication_token"))                     and attempts < int(self.spec.get("start_attempts", 6)):
                with self.lock:
                    common.append_jsonl(self.pass_dir / "settled.jsonl", {**snap, "phase": "start-retry"})
                time.sleep(float(self.spec.get("start_retry_seconds", 20)))
                snap = self.settle_fn(handle_path, repo)
                attempts += 1
            snap["phase"] = phase
            snaps.append(snap)
            with self.lock:
                common.append_jsonl(self.pass_dir / "settled.jsonl", snap)
            # Decision 1: sessions start only on a settled index; after them, the check is that the
            # publication token is unchanged. An end snapshot that is not settled (e.g. its one-hit probe
            # search timed out while a Large CPU call held the service) is recorded, not a refusal.
            if not snap.get("settled") and phase == "start":
                problems.append(f"{repo} not settled at {phase}: {snap.get('problems')}")
            elif not snap.get("settled"):
                self.log(event="end-snapshot-not-settled", repo=repo, problems=snap.get("problems"))
            if not snap.get("publication_token"):
                problems.append(f"{repo} has no publication token at {phase}")
            first = starts.get(repo)
            # Only a settled snapshot with a token can be the pass's reference start (a refused start is not).
            if first is None and phase == "start" and snap.get("settled") and snap.get("publication_token"):
                starts[repo] = snap
                common.write_json(start_path, starts)
            elif first is not None and snap.get("publication_token"):
                # A snapshot without a token is unsettled (refused above), not evidence of a changed token.
                comparison = settled.compare(first, snap)
                if not comparison["unchanged"]:
                    self.invalidate(comparison, phase)
                    problems.append(f"{repo} publication token changed ({phase})")
        if problems and not self.pilot:
            raise common.HarnessError("; ".join(problems))
        if problems:
            self.log(event="settle-problems-recorded-pilot", phase=phase, problems=problems)
        return snaps

    def invalidate(self, comparison: dict, phase: str) -> None:
        path = self.pass_dir / "INVALID.json"
        record = common.read_json(path) if path.is_file() else {"reason": "publication token changed", "changes": []}
        record["changes"].append({**comparison, "phase": phase, "at": common.now_iso()})
        common.write_json(path, record)

    def log(self, **event) -> None:
        with self.lock:
            common.append_jsonl(self.log_path, {"at": common.now_iso(), **event})

    # ------------------------------------------------------------ digests and reuse

    def _digest(self, scenario: dict, entry: dict) -> dict:
        handle = common.load_handle(self.handle_of(entry), accept_queryable=bool(self.spec.get("accept_queryable")))
        out = self.pass_dir / "digests" / f"{scenario['repository']}-{scenario['slot']}" / self.result_name(entry)
        result = replay.replay_canonical(scenario, entry["path"], handle, out,
                                         answer_defaults=entry.get("answer_defaults"),
                                         timeout_s=float(entry.get("call_timeout", 300)))
        return {"digest": result["first_digest"], "elapsed_ms": result["call"]["elapsed_ms"],
                "bytes": result["call"]["bytes"], "readiness": handle.readiness(),
                "ok": result["call"]["ok"], "handle_sha256": handle.sha256,
                "first_call_yield": result["coverage"]["first_call_yield"], "at": common.now_iso()}

    def digest(self, scenario: dict, entry: dict) -> dict:
        key = self.key(scenario, entry)
        with self.lock:
            if key in self.digests:
                return self.digests[key]
        value = self.digest_fn(scenario, entry)
        with self.lock:
            if value.get("ok"):
                self.digests[key] = value
                common.write_json(self.digests_path, self.digests)
            else:
                # Pilot finding: a failed canonical call (e.g. a publication change during indexing) has no
                # evidence to compare; it is logged, never cached, and blocks reuse for this session.
                common.append_jsonl(self.pass_dir / "digest-failures.jsonl", {"key": key, **value})
        return value

    def reuse_source(self, scenario: dict, entry: dict, digest: dict) -> dict | None:
        if not entry.get("reuse", True) or not digest.get("ok") or not digest.get("digest"):
            return None
        tools = common.PATH_TOOLS[entry["path"]]
        agent = (self.agent_model(entry), self.agent_effort(entry))
        candidates = []
        for earlier in self.spec["results"]:
            if earlier is entry or earlier.get("_origin") is entry.get("_origin", entry):
                break
            # The same tools and the same agent: a session run by another agent model is never reused.
            if (not earlier["path"].startswith("@") and common.PATH_TOOLS[earlier["path"]] == tools
                    and (self.agent_model(earlier), self.agent_effort(earlier)) == agent):
                candidates.append((self.series, earlier, self.digests))
        for prior in self.spec.get("prior_series", []):
            path = self.r_root / "passes" / prior / "digests.json"
            if path.is_file():
                candidates.append((prior, entry, common.read_json(path)))
        for series, other, digests in candidates:
            other_key = self.key(scenario, other)
            stored = digests.get(other_key)
            if not stored or stored.get("digest") != digest["digest"]:
                continue
            directory = self.session_dir(scenario, other, 1, series)
            record_path = directory / "session.json"
            record = common.read_json(record_path) if record_path.is_file() else {}
            same_agent = (record.get("model_requested"), record.get("effort_requested")) == agent
            if record.get("status") == "complete" and same_agent:
                origin = self._reused_origin(series, other_key)
                return {"key": origin[1], "series": origin[0], "session_dir": str(directory)}
        return None

    def _reused_origin(self, series: str, key: str) -> tuple[str, str]:
        """Follow a reuse chain to the session that actually ran."""
        index_path = self.r_root / "passes" / series / "reuse.json"
        reuse = common.read_json(index_path) if index_path.is_file() else {}
        if series == self.series:
            reuse = {**reuse, **self.reuse_map()}
        return (series, key) if key not in reuse else (reuse[key]["series"], reuse[key]["key"])

    def reuse_map(self) -> dict:
        path = self.pass_dir / "reuse.json"
        return common.read_json(path) if path.is_file() else {}

    def record_reuse(self, key: str, source: dict) -> None:
        with self.lock:
            current = self.reuse_map()
            current[key] = source
            common.write_json(self.pass_dir / "reuse.json", current)

    # ------------------------------------------------------------ stages

    # ------------------------------------------------------------ resolved paths

    def better_baseline(self, scenario: dict) -> str:
        """The better of Search and Parse for a scenario, from this pass's graded results.csv (grade.better_of)."""
        path = self.pass_dir / "results.csv"
        common.require(path.is_file(), "grade the pass before resolving @better-of-search-parse")
        adj_path = self.pass_dir / "adjudications.json"
        rows = [r for r in grade.build_rows(self.index(), common.read_json(adj_path) if adj_path.is_file() else {})
                if (r["repo"], r["slot"]) == (scenario["repository"], scenario["slot"]) and r.get("sample", 1) == 1
                and r["configuration"] in ("search", "parse")]
        for row in rows:
            common.require(row["adjudicated_grade"] is not None, f"{row['configuration']} of {scenario['id']} is pending")
        common.require(len(rows) == 2, f"search and parse are not both graded for {scenario['id']}")
        return grade.better_of(rows)["configuration"]

    def concrete(self, entry: dict, scenario: dict) -> dict:
        """An entry whose path is a marker (``@better-of-search-parse``) resolved for one scenario."""
        if not entry["path"].startswith("@"):
            return entry
        common.require(entry["path"] == "@better-of-search-parse", f"unknown path marker {entry['path']}")
        resolved = self.better_baseline(scenario)
        return {**entry, "path": resolved, "resolved_from": entry["path"], "_origin": entry}

    def run_one(self, scenario: dict, entry: dict, sample: int = 1) -> dict:
        entry = self.concrete(entry, scenario)
        key = self.key(scenario, entry, sample)
        if sample == 1:
            existing = self.reuse_map().get(key)
            if existing:
                return {"key": key, "reused_from": existing}
            directory = self.session_dir(scenario, entry)
            done = directory / "session.json"
            if done.is_file() and common.read_json(done).get("status") in ("complete", "failed", "timeout"):
                return {"key": key, "status": common.read_json(done).get("status"), "resumed": True}
            digest = self.digest(scenario, entry)
            source = self.reuse_source(scenario, entry, digest)
            if source:
                self.record_reuse(key, source)
                self.log(event="reused", key=key, source=source)
                return {"key": key, "reused_from": source}
        else:
            done = self.session_dir(scenario, entry, sample) / "session.json"
            if done.is_file() and common.read_json(done).get("status") in ("complete", "failed", "timeout"):
                return {"key": key, "status": common.read_json(done).get("status"), "resumed": True}
        kwargs = dict(scenario=scenario, path=entry["path"], handle=self.handle_of(entry), series=self.series,
                      sessions_root=self.sessions_root, configuration=entry["configuration"],
                      device=entry.get("device", "none"), sample=sample,
                      catalog=Path(entry["catalog"]) if entry.get("catalog") else None,
                      answer_defaults=entry.get("answer_defaults"), claude=self.claude,
                      model=self.agent_model(entry), effort=self.agent_effort(entry),
                      timeout_s=float(entry.get("session_timeout", 1800)),
                      call_timeout_s=float(entry.get("call_timeout", 300)),
                      max_budget_usd=entry.get("max_budget_usd"),
                      accept_queryable=bool(self.spec.get("accept_queryable")))
        record = self.session_runner(**kwargs)
        directory = self.session_dir(scenario, entry, sample)
        if record.get("status") == "discarded" and not directory.with_name(directory.name + ".attempt-1").exists():
            self.log(event="discarded", key=key, reasons=record.get("audit", {}).get("discard_reasons"))
            record = self.session_runner(**kwargs, rerun=True)  # plan 3.5: discarded and rerun, once
        self.log(event="session", key=key, status=record.get("status"), cost_usd=record.get("cost_usd"))
        return {"key": key, "status": record.get("status")}

    def check_names(self, only: set[str] | None) -> None:
        """A named result that the spec does not have is an error: a filter that selects nothing is no pass."""
        if only:
            known = {self.result_name(e) for e in self.spec["results"]}
            unknown = sorted(only - known)
            common.require(not unknown, f"unknown result names {unknown}; the spec has {sorted(known)}")

    def sessions(self, only: set[str] | None = None) -> None:
        self.check_names(only)
        workers = int(self.spec.get("concurrency", 4))
        entries = [e for e in self.spec["results"] if not only or self.result_name(e) in only]
        if not entries:
            return
        for entry in entries:
            self.check_image(entry)
            self.check_lifetime(entry)
            self.check_stages(entry)
        self.guard("start", entries)
        try:
            for entry in entries:
                with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
                    futures = [pool.submit(self.run_one, s, entry) for s in self.entry_scenarios(entry)]
                    for future in concurrent.futures.as_completed(futures):
                        future.result()
        finally:
            self.guard("end", entries)

    def second_sessions(self, only: set[str] | None = None) -> list[str]:
        self.check_names(only)
        summary_path = self.pass_dir / "summary.json"
        common.require(summary_path.is_file(), "grade the pass before running second sessions")
        summary = common.read_json(summary_path)
        by_name = {self.result_name(e): e for e in self.spec["results"]}
        by_id = {f"{s['repository']}-{s['slot']}": s for s in self.scenarios}
        jobs = set()
        for name, config in summary["configurations"].items():
            for row in config["scenarios"]:
                if row["scenario"] in config["close_calls"]:
                    jobs.add((row["scenario"], name))
                    if row.get("best_path"):
                        jobs.add((row["scenario"], row["best_path"]))
        scheduled = []
        workers = int(self.spec.get("concurrency", 4))
        with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
            futures = []
            for scenario_id, name in sorted(jobs):
                entry = by_name.get(name)
                if entry is None or scenario_id not in by_id or (only and name not in only):
                    continue
                self.check_stages(entry)
                scheduled.append(self.key(by_id[scenario_id], entry, 2))
                futures.append(pool.submit(self.run_one, by_id[scenario_id], entry, 2))
            for future in concurrent.futures.as_completed(futures):
                future.result()
        path = self.pass_dir / "second-sessions.json"
        previous = set(common.read_json(path)) if path.is_file() else set()
        common.write_json(path, sorted(previous | set(scheduled)))
        return scheduled

    def index(self) -> dict:
        reuse = self.reuse_map()
        extra = set(common.read_json(self.pass_dir / "second-sessions.json")) if (
            self.pass_dir / "second-sessions.json").is_file() else set()
        results = []
        for entry in self.spec["results"]:
            for scenario in self.entry_scenarios(entry):
                for sample in (1, 2):
                    key = self.key(scenario, entry, sample)
                    if sample == 2 and key not in extra:
                        continue
                    row = {"key": key, "repo": scenario["repository"], "slot": scenario["slot"],
                           "kind": scenario["kind"], "scenario_id": scenario["id"],
                           "configuration": entry["configuration"], "path": self._row_path(entry, scenario),
                           "arm": entry.get("arm"), "agent_model": self.agent_model(entry),
                           "agent_effort": self.agent_effort(entry), "timing_only": bool(entry.get("timing_only")),
                           "device": entry.get("device", "none"), "sample": sample}
                    if key in reuse:
                        source = reuse[key]
                        row["reused_from"] = source["key"] if source["series"] == self.series else (
                            f"{source['series']}:{source['key']}")
                        row["session_dir"] = source["session_dir"]
                        source_judging = self.r_root / "passes" / source["series"] / "judging" / source["key"]
                        row["coverage"] = str(Path(source["session_dir"]) / "coverage.json")
                        row["judges"] = [str(source_judging / j / "verdict.json") for j in ("j1", "j2")]
                    else:
                        directory = self.session_dir(scenario, entry, sample)
                        row["session_dir"] = str(directory)
                        row["coverage"] = str(directory / "coverage.json")
                        row["judges"] = [str(self.pass_dir / "judging" / key / j / "verdict.json") for j in ("j1", "j2")]
                    results.append(row)
        index = {"schema": "airk.private.answer-eval-pass-index/1", "series": self.series,
                 "generated": common.now_iso(), "freeze": self.freeze, "prompts_freeze": self.prompts,
                 "pilot": self.pilot, "results": results}
        # Reused rows point at a key; grade.py reads the source's files through the row itself.
        for row in results:
            if row.get("reused_from") and ":" in row["reused_from"]:
                row["reused_from_external"] = True
        common.write_json(self.pass_dir / "pass-index.json", index)
        return index

    def _row_path(self, entry: dict, scenario: dict) -> str:
        if not entry["path"].startswith("@"):
            return entry["path"]
        record = self.session_dir(scenario, entry) / "session.json"
        return common.read_json(record)["path"] if record.is_file() else entry["path"]

    def coverage(self) -> None:
        by_id = {s["id"]: s for s in self.scenarios}
        for row in self.index()["results"]:
            directory = Path(row["session_dir"])
            if row.get("reused_from") or not (directory / "session.json").is_file():
                continue
            coverage.for_session(by_id[row["scenario_id"]], directory)

    def judge(self) -> None:
        by_id = {s["id"]: s for s in self.scenarios}
        jobs = []
        for row in self.index()["results"]:
            if row.get("reused_from"):
                continue
            record_path = Path(row["session_dir"]) / "session.json"
            if not record_path.is_file() or common.read_json(record_path).get("status") != "complete":
                continue
            for name in self.judges():
                jobs.append((by_id[row["scenario_id"]], row, name))
        workers = int(self.spec.get("judge_concurrency", 4))
        with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
            futures = [pool.submit(self.judge_runner, scenario=s, session_dir=Path(row["session_dir"]), key=row["key"],
                                   out_dir=self.pass_dir / "judging" / row["key"], judge=name, claude=self.claude,
                                   model=self.spec.get("judge_model") or common.JUDGE_MODEL,
                                   effort=self.spec.get("judge_effort") or common.JUDGE_EFFORT)
                       for s, row, name in jobs]
            for future in concurrent.futures.as_completed(futures):
                record = future.result()
                self.log(event="judge", key=record.get("key"), judge=record.get("judge"), status=record.get("status"),
                         cost_usd=record.get("cost_usd"))

    def grade(self) -> dict:
        index = self.index()
        adj_path = self.pass_dir / "adjudications.json"
        rows = grade.build_rows(index, common.read_json(adj_path) if adj_path.is_file() else {})
        common.write_text(self.pass_dir / "results.csv", grade.csv_text(rows))
        summary = grade.summarize(rows, self.series)
        invalid = self.pass_dir / "INVALID.json"
        summary["invalid"] = common.read_json(invalid) if invalid.is_file() else None
        summary["pilot"] = self.pilot
        summary["prompts_freeze"] = self.prompts
        summary["timing_only"] = sorted(self.result_name(e) for e in self.spec["results"] if e.get("timing_only"))
        common.write_json(self.pass_dir / "summary.json", summary)
        return summary


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--spec", type=Path, required=True)
    parser.add_argument("--r-root", type=Path, default=common.DEFAULT_R)
    parser.add_argument("--stage", default="all", choices=["all", "sessions", "coverage", "judge", "grade", "second"])
    parser.add_argument("--only", help="comma-separated result names for the sessions stage (e.g. focus-cpu)")
    parser.add_argument("--only-second", help="comma-separated result names for the second stage")
    args = parser.parse_args(argv)
    run = Pass(common.read_json(args.spec), args.r_root)
    only = set(args.only.split(",")) if args.only else None
    if args.stage in ("all", "sessions"):
        run.sessions(only)
    if args.stage == "second":
        print(json.dumps({"second_sessions": run.second_sessions(
            set(args.only_second.split(",")) if args.only_second else None)}))
    if args.stage in ("all", "coverage", "second"):
        run.coverage()
    if args.stage in ("all", "judge", "second"):
        run.judge()
    if args.stage in ("all", "grade", "second"):
        summary = run.grade()
        print(json.dumps({"results": summary["results"], "headroom": summary["headroom"]["count"],
                          "pending": len(summary["pending_adjudication"]), "cost_usd": summary["cost_usd"]}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
