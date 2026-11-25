# SPDX-License-Identifier: Apache-2.0
"""Orchestrate one pass: sessions, coverage, judging and grading (plan 3.3-3.5, 4, 5.2).

The pass spec (JSON) names the series, the scenario set and one entry per
result::

    {"series": "01-baseline",
     "scenarios_dir": "<R>/scenarios", "repos": ["munarium", "bookbuilder"], "slots": [...],
     "scenario_ids": ["munarium-C1", ...],     # optional subset (e.g. a pilot)
     "freeze": "<R>/scenario-freeze.json",     # optional: refuse scenarios that differ from the freeze
     "pilot": false,                           # true only for plumbing pilots (never judged evidence)
     "strict_judged_receipts": true,             # require exact terminal and judge receipts (L7)
     "accept_queryable": false,                # pilot specs only: accept a queryable, not yet ready, handle
     "prompts_freeze": "<R>/design/prompts-freeze.json",   # required unless pilot; refuses a mismatch
     "handle": "<handle.json>",                # default handle for entries that name none
     "settle_repos": ["munarium"],             # indexes the settled guard checks (default: the scenarios')
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

Loop gate entries (``make_loop_spec.py``) may also carry ``arm`` and
``arm_check`` (the canonical digest reply must be that arm's, ``loop_reply.check_arm``,
or no session runs), ``reuse: false``, and ``replay_only`` (the precision arms:
their sessions run only when named with ``--only``). ``--stage second
--second-jobs <file>`` runs the gate's close calls listed by ``gate.py outcomes``.

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

Stages: ``sessions``, ``coverage``, ``judge``, ``grade``, and ``all`` (default).
``freeze-second`` freezes the first grade's close calls and chosen controls.
``second-sessions --second-jobs <file>`` runs selected second sessions while
leaving coverage, judging and grading to later stages. Historical ``second``
runs the same cohort or explicit gate jobs and then covers, judges and grades.
``--only`` limits first sessions to named results (e.g. ``focus-cpu``).
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
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
import evidence  # noqa: E402
import grade  # noqa: E402
import judge  # noqa: E402
import loop_reply  # noqa: E402
import prompts_freeze  # noqa: E402
import replay  # noqa: E402
import run_session  # noqa: E402
import settled  # noqa: E402


FIRST_GRADE_SECOND_JOBS = grade.FIRST_GRADE_SECOND_JOBS


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
        self._stage_guard_lock = threading.Lock()
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
        strict = entry.get("require_live_stages", False)
        if "stages" not in entry:
            common.require(not strict, "require_live_stages needs explicit expected stages")
            return
        handle = common.load_handle(self.handle_of(entry), accept_queryable=bool(self.spec.get("accept_queryable")))
        live = handle.live_stages() if strict else handle.stages()
        common.require(live is not None, f"{self.result_name(entry)} could not verify live stages")
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
            key = self.settle_key(handle_path, repo)
            first = starts.get(key)
            # Only a settled snapshot with a token can be the pass's reference start (a refused start is not).
            if first is None and phase == "start" and snap.get("settled") and snap.get("publication_token"):
                starts[key] = snap
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

    def settle_key(self, handle_path: str, repo: str) -> str:
        """The settled-start key: the repository for the pass's own fixture root, else root and repository, so
        side-by-side lifetimes (one fixture root each, their own indexes and tokens) are compared each with
        itself. Only with the spec's ``settle_per_root`` (gate specs), so earlier passes keep their keys."""
        if not self.spec.get("settle_per_root"):
            return repo
        root = Path(handle_path).resolve().parent.parent
        own = (self.r_root / "fixture").resolve()
        return repo if root == own else f"{root}::{repo}"

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
        value = {"digest": result["first_digest"], "elapsed_ms": result["call"]["elapsed_ms"],
                 "bytes": result["call"]["bytes"], "readiness": handle.readiness(),
                 "ok": result["call"]["ok"], "handle_sha256": handle.sha256,
                 "first_call_yield": result["coverage"]["first_call_yield"], "at": common.now_iso(),
                 "budget_sent": result["call"].get("budget_sent")}
        if result["call"].get("loop") is not None:
            value["loop"] = result["call"]["loop"]
        if entry.get("arm_check") is not None and result["call"]["ok"]:
            body = common.raw_body(Path(result["call"]["result_path"]).read_bytes())
            value["arm_problems"] = loop_reply.check_arm(body, entry["arm_check"])
            # Not a refusal: a stage whose model decisions were all shed, in a reply the model did take part in.
            value["arm_flags"] = loop_reply.arm_flags(body, entry["arm_check"])
        return value

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
            # Gate arms (plan 12.7): the live configuration must be the arm's before any agent session runs.
            if digest.get("arm_problems") and not self.pilot:
                raise common.HarnessError(f"{key} is not arm {entry.get('arm')}: {digest['arm_problems']}")
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
        if entry.get("stage_guard"):
            # Before every session, not only every plan step: an agent can pause long between calls, and a
            # stage that unloaded meanwhile would serve the next session without it (s8 shakedown, 23 Nov 2026).
            with self._stage_guard_lock:
                done = subprocess.run(entry["stage_guard"], capture_output=True, text=True,
                                      stdin=subprocess.DEVNULL, cwd=str(common.REPO_ROOT))
            self.log(event="stage-guard", key=key, exit=done.returncode, out=(done.stdout or done.stderr)[-400:])
            common.require(done.returncode == 0, f"{key}: the arm's stages are not active: {done.stderr[-400:]}")
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
        # A replay-only entry (the gate's precision arms, plan 12.7.3) runs sessions only when named.
        entries = [e for e in self.spec["results"]
                   if (not only and not e.get("replay_only")) or (only and self.result_name(e) in only)]
        if not entries:
            return
        for entry in entries:
            self.check_image(entry)
            self.check_lifetime(entry)
            self.check_stages(entry)
        self.guard("start", entries)
        try:
            if self.spec.get("parallel_entries"):
                self.parallel_sessions(entries, workers)
            else:
                for entry in entries:
                    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
                        futures = [pool.submit(self.run_one, s, entry) for s in self.entry_scenarios(entry)]
                        for future in concurrent.futures.as_completed(futures):
                            future.result()
        finally:
            self.guard("end", entries)

    def parallel_sessions(self, entries: list[dict], workers: int) -> None:
        """All entries at once (the gate's side-by-side arms, each on its own lifetime): one pool of ``workers``
        sessions, with at most an entry's ``concurrency`` sessions of that entry running together, scenarios
        interleaved across entries so every arm starts at once."""
        gates = {id(e): threading.Semaphore(int(e.get("concurrency") or workers)) for e in entries}

        def one(scenario, entry):
            with gates[id(entry)]:
                return self.run_one(scenario, entry)

        queues = [[(s, e) for s in self.entry_scenarios(e)] for e in entries]
        longest = max((len(q) for q in queues), default=0)
        jobs = [q[i] for i in range(longest) for q in queues if i < len(q)]
        with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
            futures = [pool.submit(one, s, e) for s, e in jobs]
            for future in concurrent.futures.as_completed(futures):
                future.result()

    def read_second_jobs(self, path: Path) -> list[tuple[str, str]]:
        listed = common.read_json(path)
        common.require(isinstance(listed, dict) and isinstance(listed.get("second_sessions"), list),
                       f"{path} lists no second_sessions")
        pairs = listed["second_sessions"]
        common.require(all(isinstance(pair, list) and len(pair) == 2
                           and all(isinstance(value, str) and value for value in pair) for pair in pairs),
                       f"{path} contains an invalid second-session pair")
        return sorted({(scenario, result) for scenario, result in pairs})

    def validate_second_jobs(self, jobs: list[tuple[str, str]]) -> None:
        allowed = {self.result_name(entry): {s["id"] for s in self.entry_scenarios(entry)}
                   for entry in self.spec["results"]}
        for scenario, result in jobs:
            common.require(result in allowed, f"second sessions name an unknown result {result!r}")
            common.require(scenario in allowed[result],
                           f"second sessions name an ineligible scenario {scenario!r} for {result!r}")

    def freeze_second_jobs(self) -> Path:
        """Keep the first-grade close calls and their original Parse/Search comparisons."""
        path = self.pass_dir / FIRST_GRADE_SECOND_JOBS
        first = (self.require_judged_receipts(self.index(), self.adjudications())
                 if self.spec.get("strict_judged_receipts") else None)
        if path.is_file():
            self.validate_second_jobs(self.read_second_jobs(path))
            return path
        common.require(not (self.pass_dir / "second-sessions.json").exists(),
                       "cannot freeze first-grade jobs after running second sessions")
        summary_path = self.pass_dir / "summary.json"
        common.require(summary_path.is_file(), "grade the pass before freezing second sessions")
        jobs = set()
        controls = {}
        summary = common.read_json(summary_path)
        if first is not None:
            common.require(summary["configurations"] == grade.summarize(first, self.series)["configurations"],
                           "first-grade summary differs from the retained L7 receipts; grade again")
        common.require(not summary.get("pending_adjudication"),
                       "resolve first-grade adjudications before freezing second sessions")
        for name, config in summary["configurations"].items():
            close = set(config["close_calls"])
            rows = {row["scenario"]: row for row in config["scenarios"]}
            common.require(close <= rows.keys(), f"{name} has a close call without a grade row")
            for scenario in close:
                best = rows[scenario].get("best_path")
                common.require(best in grade.BASELINES, f"{name}/{scenario} has no first-grade Parse/Search comparator")
                prior = controls.setdefault(scenario, best)
                common.require(prior == best, f"{scenario} has conflicting first-grade Parse/Search comparators")
                jobs.add((scenario, name))
        jobs.update(controls.items())
        ordered = sorted(jobs)
        self.validate_second_jobs(ordered)
        # Exclusive creation preserves the original first-grade cohort on resume.
        with path.open("x", encoding="utf-8", newline="\n") as stream:
            json.dump({"second_sessions": ordered}, stream, indent=2, ensure_ascii=False)
            stream.write("\n")
        return path

    def second_sessions(self, only: set[str] | None = None, jobs_file: Path | None = None) -> list[str]:
        """Run the selected first-grade close calls or the gate's explicit immutable jobs."""
        self.check_names(only)
        by_id = {s["id"]: s for s in self.scenarios}
        names = {self.result_name(e) for e in self.spec["results"]}
        if jobs_file is not None or self.spec.get("strict_judged_receipts"):
            jobs = self.read_second_jobs(jobs_file if jobs_file is not None else self.freeze_second_jobs())
            self.validate_second_jobs(jobs)
        else:
            summary_path = self.pass_dir / "summary.json"
            common.require(summary_path.is_file(), "grade the pass before running second sessions")
            summary = common.read_json(summary_path)
            # Earlier passes regrade after each group. Keep selecting the remaining
            # close calls, including their controls, with the original name filter.
            jobs = sorted({(row["scenario"], candidate)
                           for name, config in summary["configurations"].items()
                           for row in config["scenarios"] if row["scenario"] in config["close_calls"]
                           for candidate in (name, row.get("best_path"))
                           if candidate in names and row["scenario"] in by_id})
        jobs = [(scenario, name) for scenario, name in jobs if not only or name in only]
        if not jobs:
            return []
        names = {name for _, name in jobs}
        entries = [e for e in self.spec["results"] if self.result_name(e) in names]
        by_name = {self.result_name(e): e for e in entries}
        for entry in entries:
            self.check_image(entry)
            self.check_lifetime(entry)
            self.check_stages(entry)
        workers = int(self.spec.get("concurrency", 4))
        self.guard("start", entries)
        try:
            with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
                futures = [pool.submit(self.run_one, by_id[scenario], by_name[name], 2)
                           for scenario, name in jobs]
                for future in concurrent.futures.as_completed(futures):
                    future.result()
        finally:
            self.guard("end", entries)
        scheduled = [self.key(by_id[scenario], by_name[name], 2) for scenario, name in jobs]
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

    def adjudications(self) -> dict:
        path = self.pass_dir / "adjudications.json"
        return common.read_json(path) if path.is_file() else {}

    def require_receipt_hash(self, path: Path, expected: str | None, key: str) -> None:
        common.require(path.is_file() and common.sha256_file(path) == expected,
                       f"{key}: missing or changed {path.name}")

    def require_canonical_digest(self, series: str, key: str) -> str:
        directory = self.r_root / "passes" / series
        path = directory / "digests.json"
        records = self.digests if series == self.series else common.read_json(path) if path.is_file() else {}
        record = records.get(key) or {}
        expected = record.get("digest")
        common.require(record.get("ok") is True and isinstance(expected, str) and not record.get("arm_problems"),
                       f"{key}: missing or failed canonical digest")
        raw = directory / "digests" / key / "canonical.stdout.json"
        envelope = common.read_json(raw) if raw.is_file() else {}
        body = common.envelope_body(envelope)
        common.require(isinstance(body, dict) and envelope.get("isError") is not True
                       and evidence.evidence_digest(body) == expected, f"{key}: canonical evidence digest differs")
        return expected

    def require_terminal_session(self, row: dict, scenario: dict, reuse: dict | None) -> tuple[dict, str]:
        key = row["key"]
        series, source_key = (reuse["series"], reuse["key"]) if reuse else (self.series, key)
        directory = Path(row["session_dir"])
        path = directory / "session.json"
        common.require(path.is_file(), f"{key}: missing session receipt")
        session = common.read_json(path)
        actual_key = (f"{session.get('repo')}-{session.get('slot')}/"
                      + run_session.result_name(session.get("configuration"), session.get("device")))
        if session.get("sample") == 2:
            actual_key += ".s2"
        common.require(session.get("schema") == run_session.SESSION_SCHEMA and session.get("series") == series
                       and session.get("scenario_id") == scenario["id"]
                       and session.get("scenario_sha256") == scenario["_sha256"]
                       and session.get("sample") == row["sample"] and actual_key == source_key
                       and session.get("model_requested") == row["agent_model"]
                       and session.get("effort_requested") == row["agent_effort"],
                       f"{key}: session receipt identity differs")
        common.require(session.get("status") in ("complete", "failed", "timeout")
                       and session.get("started") and session.get("ended"),
                       f"{key}: session has no terminal complete, failed or timeout receipt")
        if session["status"] == "complete":
            common.require(session.get("exit_code") == 0 and not session.get("is_error"),
                           f"{key}: complete session receipt has a failing exit")
        self.require_receipt_hash(directory / "stream.jsonl", session.get("stream_sha256"), key)
        self.require_receipt_hash(directory / "brief.md", session.get("brief_sha256"), key)
        if row["sample"] == 1:
            expected = self.require_canonical_digest(self.series, key)
            if reuse:
                common.require(self.require_canonical_digest(series, source_key) == expected,
                               f"{key}: reused canonical evidence differs")
        return session, source_key

    def require_complete_judges(self, row: dict, scenario: dict, source_key: str) -> None:
        key = row["key"]
        coverage_path = Path(row["coverage"])
        common.require(coverage_path.is_file(), f"{key}: missing coverage receipt")
        observed = common.read_json(coverage_path)
        common.require(observed.get("schema") == coverage.COVERAGE_SCHEMA
                       and observed.get("scenario_id") == scenario["id"]
                       and observed.get("scenario_sha256") == scenario["_sha256"],
                       f"{key}: coverage receipt identity differs")
        directory = Path(row["session_dir"])
        common.require(observed == coverage.for_session(scenario, directory, write=False),
                       f"{key}: coverage differs from the retained tool receipts")
        packet = judge.build_packet(scenario, directory, source_key)
        judge_sessions = set()
        for name, verdict_path in zip(("j1", "j2"), map(Path, row["judges"]), strict=True):
            common.require(verdict_path.is_file(), f"{key}: missing {name} verdict")
            record = common.read_json(verdict_path)
            verdict = record.get("verdict")
            common.require(record.get("schema") == judge.VERDICT_SCHEMA and record.get("key") == source_key
                           and record.get("judge") == name and record.get("status") == "valid"
                           and record.get("exit_code") == 0 and not record.get("problems")
                           and Path(record.get("session_dir", "")).resolve() == Path(row["session_dir"]).resolve(),
                           f"{key}: invalid or crossed {name} verdict")
            packet_path = verdict_path.with_name("packet.json")
            self.require_receipt_hash(packet_path, record.get("packet_sha256"), key)
            common.require(common.read_json(packet_path) == packet,
                           f"{key}: {name} packet differs from the judged session")
            output_path = verdict_path.with_name("output.json")
            self.require_receipt_hash(output_path, record.get("output_sha256"), key)
            output = common.read_json(output_path)
            identifier = record.get("session_id")
            common.require(isinstance(identifier, str) and identifier and identifier not in judge_sessions
                           and output.get("session_id") == identifier
                           and judge.parse_verdict(output) == verdict and not judge.validate(verdict, scenario),
                           f"{key}: {name} lacks a distinct valid judge result")
            judge_sessions.add(identifier)

    def require_judged_receipts(self, index: dict, adjudications: dict, frozen=None) -> list[dict]:
        """Admit an L7 grade only from complete retained receipts for its expected roster."""
        rows = index["results"]
        by_id = {s["id"]: s for s in self.scenarios}
        common.require(rows and len({row["key"] for row in rows}) == len(rows), "invalid L7 grade roster")
        if frozen is not None:
            self.validate_second_jobs(frozen)
            controls = [scenario for scenario, name in frozen if name in grade.BASELINES]
            answers = {scenario for scenario, name in frozen if name not in grade.BASELINES}
            common.require(len(controls) == len(set(controls)) and set(controls) == answers,
                           "frozen L7 jobs must include one original control per scheduled Answer scenario")
            planned = {f"{scenario}/{name}.s2" for scenario, name in frozen}
            scheduled_path = self.pass_dir / "second-sessions.json"
            scheduled = set(common.read_json(scheduled_path)) if scheduled_path.is_file() else set()
            indexed = {row["key"] for row in rows if row["sample"] == 2}
            common.require(planned == scheduled == indexed, "L7 scheduled second sessions differ from the frozen roster")
        else:
            rows = [row for row in rows if row["sample"] == 1]
        reuse = self.reuse_map()
        for row in rows:
            scenario = by_id[row["scenario_id"]]
            session, source_key = self.require_terminal_session(row, scenario, reuse.get(row["key"]))
            if session["status"] == "complete":
                self.require_complete_judges(row, scenario, source_key)
            else:
                common.require(source_key not in adjudications,
                               f"{row['key']}: a failed or timed-out session cannot be adjudicated as complete")
        graded = grade.build_rows({**index, "results": rows}, adjudications)
        common.require(len(graded) == len(rows), "L7 grade lost a receipt from the expected roster")
        unresolved = [row["key"] for row in graded if row["adjudication"] in ("pending", "single-judge")
                      or type(row["adjudicated_grade"]) is not int or not 0 <= row["adjudicated_grade"] <= 4
                      or (row["correct_finish"] is True and row["turns_to_finish"] is None)]
        common.require(not unresolved, f"L7 grading has unresolved adjudication: {unresolved}")
        return graded

    def grade(self) -> dict:
        index = self.index()
        adjudications = self.adjudications()
        frozen_path = self.pass_dir / FIRST_GRADE_SECOND_JOBS
        strict = self.spec.get("strict_judged_receipts")
        frozen = self.read_second_jobs(frozen_path) if strict and frozen_path.is_file() else None
        rows = (self.require_judged_receipts(index, adjudications, frozen) if strict
                else grade.build_rows(index, adjudications))

        summary = grade.summarize(rows, self.series, frozen)
        if frozen is not None:
            cases = {(row["scenario"], name): row["case"] for name, config in summary["configurations"].items()
                     for row in config["scenarios"]}
            common.require(all(cases.get((scenario, name), {}).get("samples") == 2
                               for scenario, name in frozen if name not in grade.BASELINES),
                           "L7 final grade does not resolve every frozen pair")
        invalid = self.pass_dir / "INVALID.json"
        summary["invalid"] = common.read_json(invalid) if invalid.is_file() else None
        summary["pilot"] = self.pilot
        summary["prompts_freeze"] = self.prompts
        summary["timing_only"] = sorted(self.result_name(e) for e in self.spec["results"] if e.get("timing_only"))
        common.write_text(self.pass_dir / "results.csv", grade.csv_text(rows))
        common.write_json(self.pass_dir / "summary.json", summary)
        return summary


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--spec", type=Path, required=True)
    parser.add_argument("--r-root", type=Path, default=common.DEFAULT_R)
    parser.add_argument("--stage", default="all", choices=["all", "sessions", "coverage", "judge", "grade", "second", "freeze-second", "second-sessions"])
    parser.add_argument("--only", help="comma-separated result names for the sessions stage (e.g. focus-cpu)")
    parser.add_argument("--only-second", help="comma-separated result names for the second stage")
    parser.add_argument("--second-jobs", type=Path, help="frozen first-grade or explicit gate close-call jobs")
    args = parser.parse_args(argv)
    run = Pass(common.read_json(args.spec), args.r_root)
    only = set(args.only.split(",")) if args.only else None
    if args.stage in ("all", "sessions"):
        run.sessions(only)
    if args.stage == "freeze-second":
        print(json.dumps({"second_jobs": str(run.freeze_second_jobs())}))
    if args.stage == "second-sessions":
        expected = run.pass_dir / FIRST_GRADE_SECOND_JOBS
        common.require(args.second_jobs is not None and args.second_jobs.resolve() == expected.resolve(),
                       f"second-sessions requires --second-jobs {expected}")
    if args.stage in ("second", "second-sessions"):
        print(json.dumps({"second_sessions": run.second_sessions(
            set(args.only_second.split(",")) if args.only_second else None, args.second_jobs)}))
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
