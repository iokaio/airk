# SPDX-License-Identifier: Apache-2.0
"""Run one headless agent session (plan 3.5) for one scenario and path.

The agent is headless Claude Code on ``claude-sonnet-5-5``, started in a fresh
empty folder ``<sessions>/<series>/<repo>-<slot>/<path>[.s<k>]/work`` with an
MCP configuration naming only the wrapper. ``--setting-sources ''`` keeps
user, project and local settings, CLAUDE.md files and memory out (verified by
probe: the instruction files load without it and not with it);
``--strict-mcp-config`` drops every other MCP server; ``--tools`` naming only
the wrapper's tools leaves no built-in tool offered (probe, Claude Code
2.1.118: ``--tools ''`` also hides MCP tools, so it is not used); only the
wrapper's tools are allowed and
``--permission-mode dontAsk`` refuses anything else.

Writes ``session.json`` (command, version, model, usage, cost, times, audit),
``stream.jsonl`` (the stream-json transcript), ``brief.md`` and the wrapper's
``wrapper/turns.jsonl`` with raw results.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import codex_isolation_probe as codex_driver  # noqa: E402
import common  # noqa: E402
import evidence  # noqa: E402
from complete_evidence import CompleteEvidence  # noqa: E402
import mcp_wrapper  # noqa: E402

SYSTEM_PROMPT = """You are preparing to carry out a software development task in a code repository that you cannot see directly. You can reach the repository only through the tools you have been given; they only read it. There is no shell, file system or web access.

Your job now is to gather the context needed to do the task, then stop and write a short work brief. Do not do the task itself: write no code, documentation or tests.

Rules:
- Your first tool call must be {primary} with the task text, exactly as it was given to you, as its "{field}" argument. After that, choose your own follow-up calls.
- You may make at most 12 tool calls. Stop as soon as you have what you need; fewer calls are better.

When you stop, write the work brief as your final message, with these three headings:
## Where I would work
## What I must respect
## What I still do not know
Cite every location as path:start-end (the repository-relative path and the inclusive line range), using only locations that appeared in your tool results. Write the full path with every range, also for a second place in the same file; never write a bare line number such as "lines 14-26" or "at 236". Keep the brief under 400 words."""

SESSION_SCHEMA = "airk.private.answer-eval-session/1"
BUILTIN_DENY = ["Bash", "PowerShell", "Read", "Write", "Edit", "MultiEdit", "NotebookEdit", "Glob", "Grep", "LS",
                "WebFetch", "WebSearch", "Agent", "Task", "TodoWrite", "Skill", "ToolSearch", "SlashCommand",
                "BashOutput", "KillShell", "ExitPlanMode", "Monitor"]


@dataclass(frozen=True)
class CodexRunConfig:
    executable: Path
    executable_sha256: str
    model_catalog: Path
    model_catalog_sha256: str
    isolated_home: Path
    freeze_sha256: str


def codex_provenance(config: CodexRunConfig) -> dict:
    for digest in (config.executable_sha256, config.model_catalog_sha256, config.freeze_sha256):
        common.require(re.fullmatch(r"[a-f0-9]{64}", digest) is not None, "Codex requires SHA-256 pins")
    common.require(config.executable.is_absolute() and config.model_catalog.is_absolute(),
                   "Codex binary and model catalog paths must be absolute")
    common.require(common.sha256_file(config.executable) == config.executable_sha256, "Codex executable pin differs")
    common.require(common.sha256_file(config.model_catalog) == config.model_catalog_sha256,
                   "Codex model catalog pin differs")
    home = config.isolated_home
    default_home = Path.home() / ".codex"
    active_home = Path(os.environ.get("CODEX_HOME") or default_home)
    common.require(home.is_absolute() and home.is_dir() and home.resolve() == home
                   and home not in (default_home.resolve(), active_home.resolve())
                   and not (home / "config.toml").exists(),
                   "Codex requires a dedicated preprovisioned home without a user config")
    return {"provider": "openai", "driver": "codex-app-server-0.171.0-contract",
            "executable": str(config.executable), "executable_sha256": config.executable_sha256,
            "model_catalog_sha256": config.model_catalog_sha256, "freeze_sha256": config.freeze_sha256,
            "auth_route": "isolated_chatgpt"}


def codex_command(config: CodexRunConfig, directory: Path) -> list[str]:
    options = {
        "model_provider": "openai", "model_catalog_json": config.model_catalog.as_posix(),
        "model_reasoning_effort": "medium", "web_search": "disabled", "project_doc_max_bytes": 0,
        "features.shell_tool": False, "features.view_image": False, "features.apps": False,
        "features.multi_agent": False, "features.multi_agent_v2.disable_native_tools": True,
        "features.code_mode.enabled": False, "agents.enabled": False, "tools.experimental_request_user_input.enabled": False,
        "features.goals": False, "features.plugins": False, "features.remote_plugin": False,
        "history.persistence": "none", "analytics.enabled": False, "otel.metrics_exporter": "none",
        "cli_auth_credentials_store": "file", "forced_login_method": "chatgpt",
        "log_dir": (directory / "codex-logs").resolve().as_posix(),
        "sqlite_home": (directory / "codex-state").resolve().as_posix(),
    }
    command = [str(config.executable), "app-server", "--stdio", "--strict-config"]
    for name, value in options.items():
        command.extend(("-c", f"{name}={json.dumps(value)}"))
    return command


def codex_child_env(config: CodexRunConfig, directory: Path) -> dict:
    omitted = {"CODEX_HOME", "OPENAI_API_KEY", "CODEX_API_KEY", "OPENAI_BASE_URL",
               "CODEX_INTERNAL_ORIGINATOR_OVERRIDE", "CHATGPT_BASE_URL"}
    env = {name: os.environ[name] for name in os.environ
           if name.upper() not in omitted and not name.upper().startswith("AIRK_")}
    temporary = str((directory / "codex-tmp").resolve())
    env.update(CODEX_HOME=str(config.isolated_home), TEMP=temporary, TMP=temporary, TMPDIR=temporary)
    return env


def result_name(configuration: str, device: str) -> str:
    """Folder name of a result: the configuration, with its device when one applies (focus-cpu)."""
    return configuration if device in (None, "", "none") else f"{configuration}-{device}"


def session_dir(sessions_root: Path, series: str, repo: str, slot: str, path: str, sample: int = 1) -> Path:
    name = path if sample == 1 else f"{path}.s{sample}"
    return Path(sessions_root) / series / f"{repo}-{slot}" / name


def system_prompt(path: str) -> str:
    primary = common.PATH_TOOLS[path][0]
    return SYSTEM_PROMPT.format(primary=primary, field=common.PRIMARY_ARG[primary])


def wrapper_command(handle: Path, repo: str, path: str, instruction_file: Path, log_dir: Path,
                    catalog: Path | None, answer_defaults: dict | None, call_timeout: float,
                    accept_queryable: bool = False) -> list[str]:
    command = [sys.executable, "-I", "-B", str(HERE / "mcp_wrapper.py"), "--handle", str(handle), "--repo", repo,
               "--path", path, "--instruction-file", str(instruction_file), "--log-dir", str(log_dir),
               "--call-timeout", str(call_timeout)]
    if catalog is not None:
        command += ["--catalog", str(catalog)]
    if answer_defaults:
        command += ["--answer-defaults", json.dumps(answer_defaults, separators=(",", ":"))]
    if accept_queryable:
        command += ["--accept-queryable"]
    return command


def claude_command(claude: str, model: str, mcp_config: Path, path: str, max_budget_usd: float | None,
                   effort: str | None = common.AGENT_EFFORT) -> list[str]:
    allowed = [f"mcp__{common.SERVER_NAME}__{tool}" for tool in common.PATH_TOOLS[path]]
    common.require(effort is None or effort in common.EFFORTS, f"unknown effort {effort!r}")
    command = [claude, "-p", "--model", model, *(["--effort", effort] if effort else []),
               "--setting-sources", "", "--strict-mcp-config",
               "--mcp-config", str(mcp_config), "--tools", ",".join(allowed), "--allowedTools", ",".join(allowed),
               "--disallowedTools", ",".join(BUILTIN_DENY), "--permission-mode", "dontAsk",
               "--disable-slash-commands", "--no-session-persistence", "--system-prompt", system_prompt(path),
               "--output-format", "stream-json", "--verbose"]
    if max_budget_usd is not None:
        command += ["--max-budget-usd", str(max_budget_usd)]
    return command


def audit(stream: dict, path: str, turns: list[dict], brief: str) -> dict:
    """Discard checks (tools outside the wrapper) and the brief's citation audit.

    ``turns`` is the whole wrapper log: counted calls back the citations; every
    call, refused ones included, is compared with the stream's tool uses.
    """
    allowed = {f"mcp__{common.SERVER_NAME}__{tool}" for tool in common.PATH_TOOLS[path]}
    reasons = []
    init = stream.get("init") or {}
    outside_tools = [t for t in init.get("tools") or [] if t not in allowed]
    if init and outside_tools:
        reasons.append(f"tools offered outside the wrapper: {outside_tools}")
    servers = [s.get("name") for s in init.get("mcp_servers") or []]
    if init and servers != [common.SERVER_NAME]:
        reasons.append(f"MCP servers other than the wrapper: {servers}")
    used_outside = [u["name"] for u in stream.get("tool_uses") or [] if u.get("name") not in allowed]
    if used_outside:
        reasons.append(f"tool uses outside the wrapper: {used_outside}")
    if not init:
        reasons.append("no init event in the stream")
    return citation_and_call_audit(turns, brief, reasons,
                                   sum(1 for u in stream.get("tool_uses") or [] if u.get("name") in allowed),
                                   (stream.get("result") or {}).get("permission_denials", []))


def citation_and_call_audit(turns, brief, reasons, stream_calls, denials):
    spans = []
    counted = [row for row in turns if row.get("turn")]
    for row in counted:
        if row.get("turn") and row.get("result_path") and Path(row["result_path"]).is_file():
            spans += evidence.extract(common.raw_body(Path(row["result_path"]).read_bytes()))
    cited = evidence.cited_locations(brief)
    unbacked = [c for c in cited if not evidence.backed(c, spans)]
    # Pilot finding: briefs also wrote "lines 14-26" or "at 236" after a named file; those cannot be audited.
    bare = re.findall(r"\blines?\s+\d+(?:\s*[-\u2013]\s*\d+)?|\bat\s+\d{2,}\b", brief or "", re.I)
    wrapper_calls = len(turns)
    return {"discard": bool(reasons), "discard_reasons": reasons,
            "citations": {"cited": cited, "unbacked": unbacked, "ok": not unbacked, "unaudited_bare_refs": len(bare)},
            "tool_use_count": {"stream": stream_calls, "wrapper_logged": wrapper_calls, "counted": len(counted),
                               "refused": wrapper_calls - len(counted), "agree": stream_calls == wrapper_calls},
            "permission_denials": denials}


def audit_codex(stream_path, status, path, turns, brief):
    reasons = []
    allowed = set(common.PATH_TOOLS[path])
    ids = set()
    calls = []
    thread_id, turn_id = status.get("thread_id"), status.get("turn_id")
    if status.get("protocol_error"):
        reasons.append(status["protocol_error"])
    if status.get("configured_mcp") not in (None, {}):
        reasons.append("Codex inherited MCP servers")
    for event in common.read_jsonl(stream_path):
        method = event.get("method")
        if method == "item/tool/call" and "id" in event:
            call = event.get("params") or {}
            ident = call.get("callId")
            if (not isinstance(ident, str) or not ident or ident in ids
                    or call.get("threadId") != thread_id or call.get("turnId") != turn_id
                    or call.get("namespace") != codex_driver.DYNAMIC_NAMESPACE or call.get("tool") not in allowed):
                reasons.append("Codex requested a crossed, duplicate or outside tool")
            else:
                ids.add(ident)
                calls.append((ident, event["id"], call.get("tool"), call.get("arguments")))
        elif method and "id" in event:
            reasons.append(f"unexpected Codex server request: {method}")
    attempts = status.get("attempts", [])
    audit_calls = [(a["call_id"], a["rpc_id"], a["tool"], a["arguments"]) for a in attempts]
    aligned = (calls == audit_calls and len(turns) == len(attempts)
               and all(row.get("call") == index and row.get("tool") == a["tool"]
                       and row.get("arguments") == a["arguments"]
                       and bool(row.get("ok")) == a["success"]
                       for index, (row, a) in enumerate(zip(turns, attempts), 1)))
    if not aligned:
        reasons.append("Codex tool requests differ from wrapper log")
    if len([row for row in turns if row.get("turn")]) > common.TURN_CAP:
        reasons.append("Codex exceeded the admitted tool-call cap")
    result = citation_and_call_audit(turns, brief, reasons, len(calls), [])
    result["tool_use_count"]["agree"] = aligned and result["tool_use_count"]["agree"]
    return result


@dataclass(frozen=True)
class CompleteEvidenceInput:
    manifest: Path
    manifest_sha256: str
    source_root: Path
    publication_token: str

    def load(self, scenario: dict) -> CompleteEvidence:
        return CompleteEvidence.from_manifest(
            self.manifest.read_bytes(), expected_sha256=self.manifest_sha256, source_root=self.source_root,
            repo=scenario["repository"], slot=scenario["slot"], scenario_sha256=scenario["_sha256"],
            publication_token=self.publication_token,
            essential_ids=[note["id"] for note in scenario["notes"] if note["mark"] == "essential"])


def verify_complete_evidence(directory: Path, record: dict, treatment: CompleteEvidence) -> dict:
    """Authenticate the first Answer against the successful actor's transcript, wrapper and source."""
    from judge import FIRST_REPLY_CAP
    from replay import canonical_first_call

    observed = record["codex_observed"]
    thread, turn = observed.get("thread_id"), observed.get("turn_id")
    common.require(record["status"] == "complete" and record["exit_code"] == 0 and not record["is_error"]
                   and thread and turn and record["session_id"] == thread and observed.get("final")
                   and observed.get("auth_route") == "chatgpt" and observed.get("provider_observed") == "openai"
                   and observed.get("model_observed") == record["model_requested"] == "gpt-6.2-sol"
                   and observed.get("effort_observed") == record["effort_requested"] == "medium"
                   and observed.get("turn_status") == "completed" and record["path"] == "answer"
                   and record.get("complete_evidence_manifest_sha256") == treatment.manifest_sha256,
                   "ineligible complete-evidence actor, terminal status or manifest")
    stream = directory / "stream.jsonl"
    brief = directory / "brief.md"
    common.require(common.sha256_file(stream) == record["stream_sha256"]
                   and common.sha256_file(brief) == record["brief_sha256"]
                   and brief.read_text(encoding="utf-8") == observed["final"].replace("\r\n", "\n"),
                   "retained actor stream or brief differs")
    log = directory / "wrapper" / "turns.jsonl"
    rows = common.read_jsonl(log)
    audit = audit_codex(stream, observed, "answer", rows, observed["final"])
    common.require(not audit["discard"], "complete-evidence actor and wrapper calls differ")
    counted = [r["turn"] for r in rows if r.get("turn") is not None]
    common.require(counted == list(range(1, len(counted) + 1)), "actor wrapper turn order differs")
    treated = [r for r in rows if "complete_evidence_manifest_sha256" in r]
    common.require(len(treated) == 1, "expected exactly one treated Answer")
    first = treated[0]
    common.require(first["turn"] == 1 and first["ok"] and not first["rejected"]
                   and first["first_call"]["rule"] == "canonical" and first["tool"] == "airk_answer"
                   and first["complete_evidence_manifest_sha256"] == treatment.manifest_sha256,
                   "complete-evidence treatment was not the first canonical Answer")
    instruction = (directory / "instruction.txt").read_text(encoding="utf-8")
    question = first["sent_arguments"]["question"]
    _, expected_args = canonical_first_call("answer", question, record["answer_defaults"])
    common.require(question.replace("\r\n", "\n") == instruction
                   and first["arguments"]["question"].strip() == question.strip()
                   and first["sent_arguments"] == expected_args, "first Answer arguments differ")
    attempt = observed["attempts"][first["call"] - 1]
    events = common.read_jsonl(stream)
    completed_messages = [event["params"]["item"].get("text") for event in events
                          if event.get("method") == "item/completed" and event["params"].get("threadId") == thread
                          and event["params"].get("turnId") == turn
                          and event["params"]["item"].get("type") == "agentMessage"
                          and event["params"]["item"].get("phase") != "commentary"]
    common.require(completed_messages and completed_messages[-1] == observed["final"],
                   "retained brief differs from the terminal actor message")
    terminal = [event["params"] for event in events if event.get("method") == "turn/completed"]
    common.require(len(terminal) == 1 and terminal[0]["threadId"] == thread
                   and terminal[0]["turn"]["id"] == turn and terminal[0]["turn"]["status"] == "completed",
                   "actor transcript lacks its completed turn")

    def result_bytes(key: str, name: str) -> bytes:
        path = directory / "wrapper" / "results" / name
        common.require(Path(first[key + "_path"]).resolve() == path.resolve(), "crossed result path")
        raw = path.read_bytes()
        common.require(common.sha256_bytes(raw) == first[key + "_sha256"], "retained result digest differs")
        return raw

    native = json.loads(result_bytes("native_result", "turn-01.native.stdout.json"))
    delivered = json.loads(result_bytes("result", "turn-01.stdout.json"))
    expected = treatment.augment({"content": common.envelope_content(native)})
    common.require(not native.get("isError") and delivered == expected,
                   "delivered Answer differs from native Answer and frozen passages")
    text = common.envelope_text(delivered)
    common.require(len(text) <= FIRST_REPLY_CAP, "complete-evidence first reply would be clipped in judging")
    receipt = treatment.verify_dispatch(delivered, attempt["output_sha256"])
    common.require(receipt["text_bytes"] == first["bytes"], "delivered Answer byte count differs")
    return {**receipt, "native_result_sha256": first["native_result_sha256"],
            "result_sha256": first["result_sha256"], "turns_sha256": common.sha256_file(log),
            "call_id": attempt["call_id"], "rpc_id": attempt["rpc_id"]}


def run(*, scenario: dict, path: str, handle: Path, series: str, sessions_root: Path, configuration: str | None = None,
        device: str = "none", sample: int = 1, catalog: Path | None = None, answer_defaults: dict | None = None,
        model: str = common.AGENT_MODEL, effort: str | None = common.AGENT_EFFORT,
        timeout_s: float = 1800.0, call_timeout_s: float = 300.0,
        max_budget_usd: float | None = None, claude: str | None = None, rerun: bool = False,
        accept_queryable: bool = False, codex: CodexRunConfig | None = None,
        complete_evidence: CompleteEvidenceInput | None = None) -> dict:
    treatment = None
    if complete_evidence is not None:
        common.require(codex is not None and path == "answer" and model == "gpt-6.2-sol" and sample == 1,
                       "complete evidence requires the first Sol Codex Answer sample")
        treatment = complete_evidence.load(scenario)
    if codex is None:
        common.require(model not in common.OPENAI_EVAL_AGENT_MODELS,
                       "OpenAI actor models require an explicit frozen Codex configuration")
    else:
        common.require(model in common.OPENAI_EVAL_AGENT_MODELS and effort == "medium",
                       "Codex actor requires a locked OpenAI model and medium effort")
        common.require(claude is None and max_budget_usd is None,
                       "Claude executable and budget switches are unavailable for Codex")
        common.require(catalog is not None, "Codex actor requires the frozen AIRK tool catalog")
    provenance = codex_provenance(codex) if codex else None
    repo, slot = scenario["repository"], scenario["slot"]
    directory = session_dir(sessions_root, series, repo, slot, result_name(configuration or path, device), sample)
    record_path = directory / "session.json"
    if record_path.is_file() and not rerun:
        record = common.read_json(record_path)
        common.require(record.get("complete_evidence_manifest_sha256") ==
                       (treatment.manifest_sha256 if treatment else None), "retained session treatment differs")
        if record.get("status") in ("complete", "discarded", "failed", "timeout"):
            if provenance is not None:
                common.require(record.get("codex") == provenance and record.get("model_requested") == model
                               and record.get("effort_requested") == effort and record.get("path") == path
                               and record.get("scenario_sha256") == scenario.get("_sha256")
                               and record.get("tool_catalog_input_sha256") == common.sha256_file(catalog),
                               "retained session differs from the frozen Codex actor")
            else:
                common.require("codex" not in record, "retained Codex session requires its provider configuration")
            if treatment:
                common.require(record["answer_defaults"] == answer_defaults and record["device"] == device
                               and record["configuration"] == (configuration or path)
                               and record["handle"]["sha256"] == common.sha256_file(handle)
                               and record["repo"] == repo and record["slot"] == slot and record["series"] == series
                               and record["sample"] == sample, "retained complete-evidence session inputs differ")
                receipt = verify_complete_evidence(directory, record, treatment)
                common.require(record.get("complete_evidence_receipt") == receipt,
                               "retained complete-evidence receipt differs")
            return record
    if directory.exists():
        attempt = 1
        while directory.with_name(f"{directory.name}.attempt-{attempt}").exists():
            attempt += 1
        directory.rename(directory.with_name(f"{directory.name}.attempt-{attempt}"))
    work, log_dir = directory / "work", directory / "wrapper"
    work.mkdir(parents=True)
    log_dir.mkdir()
    instruction = scenario["instruction"]
    instruction_file = directory / "instruction.txt"
    common.write_text(instruction_file, instruction)
    mcp_config = directory / "mcp-config.json"
    loaded = common.load_handle(handle, accept_queryable=accept_queryable)  # refuse an unready handle up front
    if codex is not None:
        return run_codex_agent(scenario, path, loaded, handle, series, directory, configuration, device,
                               sample, catalog, answer_defaults, model, effort, timeout_s, call_timeout_s,
                               accept_queryable, codex, provenance, treatment)
    wrapper = wrapper_command(handle, repo, path, instruction_file, log_dir, catalog, answer_defaults, call_timeout_s,
                              accept_queryable)
    common.write_json(mcp_config, {"mcpServers": {common.SERVER_NAME: {"type": "stdio", "command": wrapper[0],
                                                                         "args": wrapper[1:], "env": {}}}})
    claude = claude or common.find_claude()
    version = common.claude_version(claude)
    command = claude_command(claude, model, mcp_config, path, max_budget_usd, effort)
    stream_path, stderr_path = directory / "stream.jsonl", directory / "stderr.txt"
    record = {"schema": SESSION_SCHEMA, "series": series, "repo": repo, "slot": slot, "scenario_id": scenario.get("id"),
              "scenario_sha256": scenario.get("_sha256"), "kind": scenario.get("kind"), "path": path,
              "configuration": configuration or path, "device": device, "sample": sample,
              "handle": {"path": str(handle), "sha256": common.sha256_file(handle),
                         "index_id": loaded.index_id(repo), "accept_queryable": accept_queryable},
              "handle_readiness_start": loaded.readiness(),
              "answer_defaults": answer_defaults, "answer_budget_source": common.answer_budget_source(),
              "claude_version": version, "model_requested": model,
              "effort_requested": effort, "claude_path": claude,
              "command": command, "stdin": "instruction.txt", "cwd": str(work),
              "mcp_config_sha256": common.sha256_file(mcp_config), "status": "running", "started": common.now_iso()}
    common.write_json(record_path, record)
    began = time.monotonic()
    with stream_path.open("wb") as out, stderr_path.open("wb") as err:
        proc = subprocess.Popen(command, cwd=work, stdin=subprocess.PIPE, stdout=out, stderr=err,
                                env=common.clean_child_env(), creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        proc.stdin.write(instruction.encode("utf-8"))
        proc.stdin.close()
        try:
            exit_code = proc.wait(timeout=timeout_s)
            timed_out = False
        except subprocess.TimeoutExpired:
            common.kill_tree(proc.pid)
            exit_code = proc.wait(timeout=60)
            timed_out = True
    record["ended"] = common.now_iso()
    record["elapsed_ms"] = int((time.monotonic() - began) * 1000)
    record["exit_code"] = exit_code
    stream = common.parse_stream(stream_path)
    result = stream.get("result") or {}
    brief = result.get("result") if isinstance(result.get("result"), str) else ""
    common.write_text(directory / "brief.md", brief)
    turns = common.read_jsonl(log_dir / "turns.jsonl")
    counted = [t for t in turns if t.get("turn")]
    checks = audit(stream, path, turns, brief)
    checks["rejected_calls"] = [t for t in turns if t.get("rejected")]
    init = stream.get("init") or {}
    record.update({
        "session_id": result.get("session_id") or init.get("session_id"),
        "init": {k: init.get(k) for k in ("tools", "mcp_servers", "model", "claude_code_version", "permissionMode",
                                          "skills", "agents", "plugins", "slash_commands", "apiKeySource")},
        "models_used": sorted((result.get("modelUsage") or {}).keys()),
        "cost_usd": result.get("total_cost_usd"), "usage": result.get("usage"), "model_usage": result.get("modelUsage"),
        "num_turns": result.get("num_turns"), "duration_ms": result.get("duration_ms"),
        "result_subtype": result.get("subtype"), "is_error": result.get("is_error"),
        "stream_sha256": common.sha256_file(stream_path), "brief_sha256": common.sha256_file(directory / "brief.md"),
        "wrapper_turns": len(counted), "audit": checks,
        # The exact budget each airk_answer call of the session was sent (the wrapper's turn log).
        "answer_budgets_sent": common.distinct_budgets(counted),
        "handle_readiness_end": loaded.readiness(),
    })
    if timed_out:
        record["status"] = "timeout"
    elif checks["discard"]:
        record["status"] = "discarded"
    elif exit_code != 0 or not result or result.get("is_error"):
        record["status"] = "failed"
    else:
        record["status"] = "complete"
    common.write_json(record_path, record)
    return record


def run_codex_agent(scenario, path, loaded, handle, series, directory, configuration, device, sample,
                    catalog, answer_defaults, model, effort, timeout_s, call_timeout_s, accept_queryable,
                    codex, provenance, treatment=None):
    work = directory / "work"
    log_dir = directory / "wrapper"
    for child in ("codex-logs", "codex-state", "codex-tmp"):
        (directory / child).mkdir()
    stream_path = directory / "stream.jsonl"
    config_path = directory / "codex-tools.json"
    command = codex_command(codex, directory)
    instruction = scenario["instruction"]
    repo, slot = scenario["repository"], scenario["slot"]
    began = time.monotonic()
    deadline = began + timeout_s

    def backend(tool, arguments):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise subprocess.TimeoutExpired("AIRK wrapper", timeout_s)
        return common.call_tool(loaded, tool, repo, arguments, min(call_timeout_s, remaining),
                                strip_env=common.AGENT_STRIPPED_ENV)

    tool_input_sha256 = common.sha256_file(catalog)
    wrapper = mcp_wrapper.Wrapper(handle=loaded, repo=repo, path=path, instruction=instruction,
                                  catalog=common.load_catalog(catalog), log_dir=log_dir,
                                  answer_defaults=answer_defaults, call_timeout_s=call_timeout_s,
                                  backend=backend, deadline=deadline, complete_evidence=treatment)
    tools = wrapper.list_tools()
    common.write_json(config_path, codex_driver.dynamic_specs(tools))
    record = {"schema": SESSION_SCHEMA, "series": series, "repo": repo, "slot": slot, "scenario_id": scenario.get("id"),
              "scenario_sha256": scenario.get("_sha256"), "kind": scenario.get("kind"), "path": path,
              "configuration": configuration or path, "device": device, "sample": sample,
              "handle": {"path": str(handle), "sha256": common.sha256_file(handle),
                         "index_id": loaded.index_id(repo), "accept_queryable": accept_queryable},
              "handle_readiness_start": loaded.readiness(), "answer_defaults": answer_defaults,
              "answer_budget_source": common.answer_budget_source(), "model_requested": model,
              "effort_requested": effort, "command": command, "stdin": "instruction.txt", "cwd": str(work),
              "codex": provenance, "tool_catalog_input_sha256": tool_input_sha256,
              "codex_tools_sha256": common.sha256_file(config_path), "status": "running", "started": common.now_iso()}
    if treatment:
        record["complete_evidence_manifest_sha256"] = treatment.manifest_sha256
    record_path = directory / "session.json"
    common.write_json(record_path, record)
    try:
        exit_code, stdout, stderr, observed = codex_driver.run_app_server(
            command, codex_child_env(codex, directory), work, model=model, effort=effort,
            instruction=instruction, tool_catalog=tools, clean_home=True, dispatch=wrapper.call,
            base_instructions=system_prompt(path), timeout=max(0, deadline - time.monotonic()), require_chatgpt=True)
    except OSError:
        exit_code, stdout, stderr, observed = 2, b"", b"", {"stage": "start", "startup_failed": True}
    common.write_bytes(stream_path, stdout)
    common.write_bytes(directory / "stderr.txt", stderr)
    brief = observed.get("final") or ""
    common.write_text(directory / "brief.md", brief)
    turns = common.read_jsonl(wrapper.turn_log)
    counted = [row for row in turns if row.get("turn")]
    checks = audit_codex(stream_path, observed, path, turns, brief)
    checks["rejected_calls"] = [row for row in turns if row.get("rejected")]
    model_observed = observed.get("model_observed")
    turn_completed = observed.get("turn_status") == "completed"
    completed = exit_code == 0 and turn_completed and bool(brief)
    record.update(session_id=observed.get("thread_id"), codex_observed=observed, cost_usd=None,
                  usage=observed.get("usage"), model_usage=None, models_used=[model_observed] if model_observed else [],
                  ended=common.now_iso(), elapsed_ms=int((time.monotonic() - began) * 1000),
                  exit_code=exit_code, is_error=not completed, num_turns=1 if observed.get("turn_id") else 0,
                  stream_sha256=common.sha256_file(stream_path), brief_sha256=common.sha256_file(directory / "brief.md"),
                  wrapper_turns=len(counted), audit=checks, answer_budgets_sent=common.distinct_budgets(counted),
                  handle_readiness_end=loaded.readiness())
    if exit_code is None:
        record["status"] = "timeout"
    elif checks["discard"] and observed.get("turn_id"):
        record["status"] = "discarded"
    else:
        record["status"] = "complete" if completed and not checks["discard"] else "failed"
    if treatment and record["status"] == "complete":
        try:
            record["complete_evidence_receipt"] = verify_complete_evidence(directory, record, treatment)
        except (common.HarnessError, OSError, KeyError, TypeError, ValueError) as error:
            record.update(status="failed", is_error=True, complete_evidence_error=str(error))
    common.write_json(record_path, record)
    return record


def reaudit(directory: Path) -> dict:
    """Recompute a finished session's audit from its stored stream, wrapper log and brief."""
    directory = Path(directory)
    record = common.read_json(directory / "session.json")
    common.require("complete_evidence_manifest_sha256" not in record,
                   "use the frozen complete-evidence input to revalidate this session")
    stream_path = directory / "stream.jsonl"
    turns = common.read_jsonl(directory / "wrapper" / "turns.jsonl")
    brief = (directory / "brief.md").read_text(encoding="utf-8") if (directory / "brief.md").is_file() else ""
    checks = (audit_codex(stream_path, record["codex_observed"], record["path"], turns, brief)
              if "codex" in record else audit(common.parse_stream(stream_path), record["path"], turns, brief))
    checks["rejected_calls"] = [t for t in turns if t.get("rejected")]
    record.setdefault("audit_history", []).append({"at": common.now_iso(), "previous": record.get("audit")})
    record["audit"] = checks
    if record.get("status") in ("complete", "discarded"):
        record["status"] = "discarded" if checks["discard"] else "complete"
    common.write_json(directory / "session.json", record)
    return record


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--scenario", type=Path, required=True)
    parser.add_argument("--path", choices=sorted(common.PATH_TOOLS), required=True)
    parser.add_argument("--handle", type=Path, required=True)
    parser.add_argument("--series", required=True)
    parser.add_argument("--sessions-root", type=Path, default=common.DEFAULT_R / "sessions")
    parser.add_argument("--configuration", help="result name when it differs from the path (e.g. focus)")
    parser.add_argument("--device", default="none", choices=["none", "cpu", "gpu"])
    parser.add_argument("--sample", type=int, default=1)
    parser.add_argument("--catalog", type=Path)
    parser.add_argument("--answer-defaults", help="JSON object forced into airk_answer calls")
    parser.add_argument("--model", default=common.AGENT_MODEL)
    parser.add_argument("--effort", default=common.AGENT_EFFORT, choices=common.EFFORTS)
    parser.add_argument("--timeout", type=float, default=1800.0)
    parser.add_argument("--call-timeout", type=float, default=300.0)
    parser.add_argument("--max-budget-usd", type=float)
    parser.add_argument("--rerun", action="store_true", help="keep the old folder as .attempt-N and run again")
    parser.add_argument("--claude", help="Claude Code executable (default: common.find_claude())")
    parser.add_argument("--codex-config", type=Path, help="frozen Codex provider configuration JSON")
    parser.add_argument("--accept-queryable", action="store_true", help="pilot only (see common.load_handle)")
    args = parser.parse_args(argv)
    codex = None
    if args.codex_config:
        config = common.read_json(args.codex_config)
        for name in ("executable", "model_catalog", "isolated_home"):
            config[name] = Path(config[name])
        codex = CodexRunConfig(**config)
    record = run(scenario=common.load_scenario(args.scenario), path=args.path, handle=args.handle, series=args.series,
                 sessions_root=args.sessions_root, configuration=args.configuration, device=args.device,
                 sample=args.sample, catalog=args.catalog,
                 answer_defaults=json.loads(args.answer_defaults) if args.answer_defaults else None,
                 model=args.model, effort=args.effort, timeout_s=args.timeout, call_timeout_s=args.call_timeout,
                 max_budget_usd=args.max_budget_usd, rerun=args.rerun, claude=args.claude,
                 accept_queryable=args.accept_queryable, codex=codex)
    print(json.dumps({k: record.get(k) for k in ("status", "session_id", "cost_usd", "wrapper_turns", "elapsed_ms")}))
    return 0 if record["status"] == "complete" else 1


if __name__ == "__main__":
    sys.exit(main())
