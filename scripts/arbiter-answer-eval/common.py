# SPDX-License-Identifier: Apache-2.0
"""Shared helpers for the Answer evaluation harness.

Standard library only. Every writer here produces UTF-8 with LF line endings.
Nothing in this module talks to a model; it reads handles, catalogs and
scenarios and runs one CLI process per tool call.
"""

from __future__ import annotations

import dataclasses
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import tool_names

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[1]
DEFAULT_R = REPO_ROOT / "artifacts" / "v190" / "final-plan" / "exec-20261116T052942Z" / "answer-eval-r1"

# User decision, 21 Nov 2026 (plan section 2, "Changed by the user during execution"): agents on Haiku,
# judges on Sonnet, both at medium effort. The pilot ran Sonnet agents and Opus judges at the default effort.
OPENAI_EVAL_AGENT_MODELS = frozenset({"gpt-6.2-sol", "gpt-6.2-astra", "gpt-6.1-terra"})
AGENT_MODEL = "claude-haiku-5-5"
AGENT_EFFORT = "medium"
JUDGE_MODEL = "claude-sonnet-5-5"
JUDGE_EFFORT = "medium"
EFFORTS = ("low", "medium", "high", "xhigh", "max")
TURN_CAP = 12
SERVER_NAME = "airk"

READS = ["airk_read_file", "airk_context", "airk_references", "airk_symbols", "airk_outline"]
ANSWER_SET = ["airk_answer", *READS]
PATH_TOOLS: dict[str, list[str]] = {
    "answer": ANSWER_SET,
    "noModel": ANSWER_SET,
    "focus": ANSWER_SET,
    "compact": ANSWER_SET,
    "large": ANSWER_SET,
    # Arbiter only (LH): the Context model with Focus switched off.
    "arbiter": ANSWER_SET,
    "search": ["airk_search", *READS],
    "parse": ["airk_parse"],
}
ANSWER_PATHS = ("answer", "noModel", "focus", "compact", "large", "arbiter")
PRIMARY_ARG = {"airk_answer": "question", "airk_search": "query", "airk_parse": "text"}

# Answer /3, the local loop (plan section 12; CONTRACT.md "Answer loop wire contract version 3"). The
# request refuses a budget that lacks any field, so the default is the contract's whole default budget.
ANSWER_SCHEMA = "airk.arbiter.context.request/3"
PARSE_SCHEMA = "airk.parse.request/1"
# The default budget is not written here. ``ANSWER_BUDGET_DEFAULT`` (below, after the helpers it needs) is read
# at import from the Rust-emitted semantic cases, the one owner of the contract's default.
ANSWER_CASES = REPO_ROOT / "fixtures" / "split" / "arbiter" / "answer_loop_cases.json"
ANSWER_CASES_NAME = "fixtures/split/arbiter/answer_loop_cases.json"
# History only (the one-turn evaluation, 21-22 Nov 2026): the /2 request and plan 3.7's defaults.
ANSWER_SCHEMA_V2 = "airk.arbiter.context.request/2"
ANSWER_BUDGET_DEFAULT_V2 = {"calls": 8, "rounds": 3, "fanout": 8, "input_tokens": 8192,
                            "candidates": 32, "max_bytes": 262144, "deadline_ms": 120000}
# Arguments the wrapper hides from the agent's schemas and sets itself.
HIDDEN_ARGS = {"index", "project"}
FORCED_HIDDEN = {"airk_answer": {"schema", "budget"}, "airk_parse": {"schema"}}

SLOTS = ("C1", "C2", "C3", "W1", "W2", "T1", "T2", "A1", "A2", "A3")
KINDS = {"C": "coding", "W": "writing", "T": "test", "A": "architecture"}
# The loop series' pair (user direction, 22 Nov 2026): the index under test must hold nothing about the
# experiment, so munarium replaces airk. Every default that names repositories uses this pair.
REPOS = ("munarium", "bookbuilder")
# The index names a loop lifetime holds for REPOS, in the launcher's order (fixture.py DEFAULT_REPOSITORIES: the
# repository with denies first). "bookbuilder" is served by "bookbuilder-v2" (ALIASES_FILE).
LOOP_INDEXES = ("bookbuilder-v2", "munarium")
# Every repository name a scenario, pass or record may carry. airk stays known so that the one-turn history
# (21-22 Nov 2026) and the tools that read it still load; no loop lifetime holds an airk index.
KNOWN_REPOS = ("airk", "bookbuilder", "munarium")
FORBIDDEN_LOOP_INDEX = "airk"


class HarnessError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise HarnessError(message)


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def answer_default_budget(path: Path = ANSWER_CASES) -> dict:
    """The contract's whole default budget, read from the Rust-emitted cases file (``default_budget``).

    Never copied by hand. On 22 November 2026 the product default moved from 131,072 to 196,608 input tokens
    (commit 936c9b94); a copied 131,072 would have been forced on every gate Answer and, by the change log's
    figures, taken the model away from a two-part Answer in round 3 and from a four-part one in round 2."""
    cases = json.loads(Path(path).read_bytes())
    budget = cases.get("default_budget") if isinstance(cases, dict) else None
    require(isinstance(cases, dict) and cases.get("request_schema") == ANSWER_SCHEMA,
            f"{path} is not the cases file of {ANSWER_SCHEMA}")
    require(isinstance(budget, dict) and bool(budget)
            and all(isinstance(k, str) and type(v) is int and v > 0 for k, v in budget.items()),
            f"{path} carries no whole default budget")
    return dict(budget)


ANSWER_BUDGET_DEFAULT = answer_default_budget()


def answer_budget_source() -> dict:
    """Where the default budget came from, for receipts and the prompts freeze: the cases file's bytes and the
    budget read from them."""
    return {"path": ANSWER_CASES_NAME, "sha256": sha256_file(ANSWER_CASES),
            "default_budget": dict(ANSWER_BUDGET_DEFAULT)}


def distinct_budgets(rows: list[dict]) -> list[dict]:
    """The exact budgets a set of ``airk_answer`` calls sent, each with the number of calls that sent it: a
    replay row's ``budget_sent`` or a wrapper turn's ``sent_arguments``. Receipts carry this so that a pass
    shows what the service was asked for, not what the harness meant to send."""
    found: dict[bytes, dict] = {}
    for row in rows:
        budget = row.get("budget_sent")
        if budget is None and row.get("tool") == "airk_answer":
            budget = (row.get("sent_arguments") or {}).get("budget")
        if isinstance(budget, dict):
            found.setdefault(canonical_json(budget), {"budget": budget, "calls": 0})["calls"] += 1
    return [found[key] for key in sorted(found)]


def read_json(path: Path) -> Any:
    return json.loads(Path(path).read_bytes())


def write_bytes(path: Path, data: bytes) -> None:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + ".tmp")
    with tmp.open("wb") as stream:
        stream.write(data)
    os.replace(tmp, path)


def write_text(path: Path, text: str) -> None:
    write_bytes(path, text.replace("\r\n", "\n").encode("utf-8"))


def write_json(path: Path, value: Any) -> None:
    write_bytes(path, (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode("utf-8"))


def append_jsonl(path: Path, value: Any) -> None:
    with Path(path).open("ab") as stream:
        stream.write(json.dumps(value, ensure_ascii=False).encode("utf-8") + b"\n")
        stream.flush()


def read_jsonl(path: Path) -> list[Any]:
    path = Path(path)
    if not path.is_file():
        return []
    return [json.loads(line) for line in path.read_bytes().splitlines() if line.strip()]


# ---------------------------------------------------------------- handle


@dataclasses.dataclass(frozen=True)
class Handle:
    path: Path
    sha256: str
    cli: tuple[str, ...]
    control: str
    env: dict[str, str]
    indexes: dict[str, str]
    state: str | None = None
    queryable: bool | None = None
    bindings: dict | None = None
    aliases: dict | None = None

    def logged_stages(self) -> dict:
        """Enabled Arbiter stages from the lifetime's ``stages.jsonl`` (written by ``fixture.py stage``).

        Returns ``{"focus": profile | None, "context": profile | None}``: the last record of each kind wins.
        """
        current: dict[str, str | None] = {"focus": None, "context": None}
        for record in read_jsonl(self.path.parent / "stages.jsonl"):
            profile = str(record.get("profile") or "")
            kind = "focus" if profile.startswith("focus") else "context" if profile.startswith("context") else None
            if kind:
                enabled = record.get("enabled", record.get("already_enabled"))
                current[kind] = profile if enabled else None
        return current

    def live_stages(self, timeout_s: float = 60.0) -> dict | None:
        """Enabled stages as the service reports them (``arbiter status-legacy``, read-only).

        Manifests are named through the profile/manifest pairs of ``stages.jsonl``; an unknown manifest
        is returned as its hash. None when the CLI cannot answer (e.g. the mock CLI of the tests).
        """
        try:
            proc = subprocess.run([*self.cli, "--output", "json", "--service-runtime-dir", self.control,
                                   "arbiter", "status-legacy"], capture_output=True, stdin=subprocess.DEVNULL,
                                  env=tool_env(self), timeout=timeout_s,
                                  creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            state = json.loads(proc.stdout) if proc.returncode == 0 else None
        except (OSError, ValueError, subprocess.TimeoutExpired):
            state = None
        if not isinstance(state, dict) or not isinstance(state.get("configuration"), dict):
            return None
        # Name manifests from every lifetime's stages.jsonl: a restarted lifetime carries the stages over
        # without logging them again (L17-cs1, 22 Nov 2026).
        names = {}
        own_log = self.path.parent / "stages.jsonl"
        history = sorted(set(self.path.parent.parent.glob("L*/stages.jsonl")) - {own_log})
        for log in [*history, own_log]:
            names.update({r.get("manifest"): r.get("profile") for r in read_jsonl(log) if r.get("manifest")})
        current: dict[str, str | None] = {}
        for kind in ("focus", "context"):
            config = state["configuration"].get(kind) or {}
            manifest = config.get("selected_profile")
            current[kind] = (names.get(manifest) or manifest) if config.get("enabled") else None
        return current

    def stages(self) -> dict:
        """The live stages when the service answers (smoke runs change stages without logging), else the log."""
        live = self.live_stages()
        return live if live is not None else self.logged_stages()

    def readiness(self) -> dict:
        """The handle's readiness as recorded with every session (pilot: queryable may stand in for ready)."""
        raw = read_json(self.path)
        progress_path = self.path.parent.parent / "progress.json"
        progress = read_json(progress_path) if progress_path.is_file() else {}
        semantic = {repo: {k: row.get(k) for k in ("state", "completed", "selected", "vectors", "utc", "lifetime")}
                    for repo, row in (progress.get("indexes") or {}).items() if isinstance(row, dict)}
        return {"state": raw.get("state"), "queryable": raw.get("queryable"), "lifetime": raw.get("lifetime"),
                "stages": self.stages(),
                "image": image_identity(raw.get("image")),
                "progress_image": progress.get("image"), "semantic": semantic, "aliases": self.aliases or {},
                "read_at": now_iso()}

    def index_id(self, repo: str) -> str:
        require(repo in self.indexes, f"handle {self.path} names no index for repository {repo!r}")
        return self.indexes[repo]

    def binding(self, repo: str) -> str | None:
        return (self.bindings or {}).get(repo)


def _first(raw: dict, *names: str) -> Any:
    for name in names:
        if name in raw and raw[name] not in (None, ""):
            return raw[name]
    return None


def image_identity(image) -> dict | str | None:
    """The fixture image a handle runs: label and service SHA-256 (fixture.py's ``image`` record)."""
    if not isinstance(image, dict):
        return image
    service = image.get("service") if isinstance(image.get("service"), dict) else {}
    return {"label": image.get("label") or image.get("name"), "service_sha256": service.get("sha256"),
            "git_head": image.get("git_head")}


def load_handle(path: Path, accept_queryable: bool = False) -> Handle:
    """Read a fixture handle. Accepts the spellings the fixture agent may use.

    A handle must be ``ready``. ``accept_queryable`` (pilot only) also accepts
    a handle marked ``queryable: true`` whose vectors are still partial; the
    semantic state is then recorded with every session (Handle.readiness).
    """
    path = Path(path)
    raw = read_json(path)
    cli = _first(raw, "cli", "cli_path", "cli_command", "cli_image")
    if isinstance(cli, dict):
        cli = _first(cli, "path", "command")
    require(cli is not None, f"handle {path} has no CLI path")
    cli = tuple(cli) if isinstance(cli, list) else (str(cli),)
    control = _first(raw, "control", "control_dir", "control_root", "service_runtime_dir")
    if isinstance(control, dict):
        control = _first(control, "path")
    require(control is not None, f"handle {path} has no control directory")
    state = raw.get("state")
    queryable = raw.get("queryable")
    require(state in (None, "ready") or (accept_queryable and queryable is True),
            f"handle {path} is not ready (state {state!r}, queryable {queryable!r})")
    # fixture.py: "environment" is the client environment; "service_environment" is the service's own.
    env = _first(raw, "environment", "env") or {}
    require(isinstance(env, dict), f"handle {path} env is not an object")
    indexes_raw = _first(raw, "indexes", "index_ids", "repositories") or {}
    indexes: dict[str, str] = {}
    bindings: dict[str, str] = {}
    if isinstance(indexes_raw, list):
        for item in indexes_raw:
            indexes[str(_first(item, "repository", "repo", "name"))] = str(_first(item, "index_id", "id"))
    else:
        for repo, value in indexes_raw.items():
            indexes[repo] = str(_first(value, "index_id", "id") if isinstance(value, dict) else value)
            if isinstance(value, dict) and value.get("binding"):
                bindings[repo] = str(value["binding"])
    require(indexes, f"handle {path} names no indexes")
    aliases = load_aliases(path.parent.parent / ALIASES_FILE)
    for logical, physical in aliases.items():
        require(physical in indexes, f"alias {logical} -> {physical}: the handle has no index {physical!r}")
        indexes[f"{logical}{ORIGINAL_SUFFIX}"] = indexes.get(logical, "")
        indexes[logical] = indexes[physical]
        if physical in bindings:
            if logical in bindings:
                bindings[f"{logical}{ORIGINAL_SUFFIX}"] = bindings[logical]
            bindings[logical] = bindings[physical]
    return Handle(path, sha256_file(path), cli, str(control), {str(k): str(v) for k, v in env.items()}, indexes,
                  state, queryable, bindings, aliases)


# Index aliases (primary agent's decision, 22 Nov 2026): a scenario's repository name can be served by
# another index of the same installation, e.g. "bookbuilder" by the fresh "bookbuilder-v2" index built with
# its denies before the first index pass. The file lives beside the lifetimes (R/fixture/index-aliases.json)
# so every tool, the wrapper's own process included, resolves the same index; every record keeps the
# aliases in Handle.readiness, and the replaced index stays reachable as "<name>@original".
ALIASES_FILE = "index-aliases.json"
ORIGINAL_SUFFIX = "@original"


def load_aliases(path: Path) -> dict:
    if not Path(path).is_file():
        return {}
    raw = read_json(path)
    aliases = raw.get("aliases", raw) if isinstance(raw, dict) else {}
    require(isinstance(aliases, dict) and all(isinstance(k, str) and isinstance(v, str) for k, v in aliases.items()),
            f"{path} must map repository names to index names")
    return dict(aliases)


# Decision 2 (21 Nov 2026): agent sessions get the product's default MCP response mode, so the
# handle's AIRK_MCP_RESPONSE_MODE (the fixture sets "full") never reaches an agent's tool calls.
AGENT_STRIPPED_ENV = ("AIRK_MCP_RESPONSE_MODE",)


NEWEST = "@newest"


def newest_handle(fixture_dir: Path) -> Path:
    """The handle of the newest fixture lifetime (highest ``L<n>``) under ``fixture_dir``.

    The fixture owner starts a new lifetime near each deadline. A spec that names ``@newest`` follows it,
    and every session record keeps the concrete handle path and SHA-256 it used.
    """
    best, best_n = None, -1
    for path in Path(fixture_dir).glob("L*/handle.json"):
        digits = "".join(ch for ch in path.parent.name[1:].split("-")[0] if ch.isdigit())
        if digits and int(digits) > best_n:
            best, best_n = path, int(digits)
    require(best is not None, f"no fixture handle under {fixture_dir}")
    return best


def resolve_handle(value: str | Path, fixture_dir: Path) -> Path:
    return newest_handle(fixture_dir) if str(value) == NEWEST else Path(value)


def tool_env(handle: Handle, strip: tuple[str, ...] = ()) -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith("AIRK_")}
    env.update({k: v for k, v in handle.env.items() if k not in strip})
    return env


def effective_response_mode(handle: Handle | None, args: dict, strip: tuple[str, ...] = ()) -> str:
    """The response mode a call is served in: its own argument, else the environment default, else terse."""
    if isinstance(args.get("response_mode"), str):
        return args["response_mode"]
    if handle is not None and "AIRK_MCP_RESPONSE_MODE" not in strip and handle.env.get("AIRK_MCP_RESPONSE_MODE"):
        return handle.env["AIRK_MCP_RESPONSE_MODE"]
    return "terse (product default)"


# ---------------------------------------------------------------- catalog


def load_catalog(path: Path) -> dict[str, dict]:
    raw = read_json(path)
    if isinstance(raw, dict):
        for key in ("tools", "result", "catalog"):
            if key in raw:
                raw = raw[key]
                if isinstance(raw, dict) and "tools" in raw:
                    raw = raw["tools"]
                break
    require(isinstance(raw, list), f"catalog {path} holds no tool list")
    tools = {}
    for tool in raw:
        name = tool["name"]
        schema = tool.get("inputSchema") or tool.get("input_schema") or tool.get("parameters") or {"type": "object"}
        tools[name] = {"name": name, "description": tool.get("description", ""), "inputSchema": schema,
                       "catalog_name": name}
    # The CLI's `tools list --json` names tools by their public names (search before 1.9,
    # search_passages from 1.9) and `tools call` also accepts the native name; the harness uses the
    # native names throughout. tool_names maps either image's public names to the native name.
    for name, tool in list(tools.items()):
        native = tool_names.native(name)
        if native != name and native not in tools:
            tools[native] = dict(tool, name=native)
    return tools


def agent_schema(tool: str, schema: dict) -> dict:
    """The tool's input schema as the agent sees it: wrapper-owned arguments removed."""
    schema = json.loads(json.dumps(schema))
    hidden = HIDDEN_ARGS | FORCED_HIDDEN.get(tool, set())
    props = schema.get("properties")
    if isinstance(props, dict):
        for name in hidden:
            props.pop(name, None)
    if isinstance(schema.get("required"), list):
        schema["required"] = [name for name in schema["required"] if name not in hidden]
    return schema


# ---------------------------------------------------------------- tool calls


@dataclasses.dataclass
class CallOutcome:
    ok: bool
    exit_code: int | None
    stdout: bytes
    stderr: bytes
    elapsed_ms: int
    envelope: dict | None
    error: str | None


def cli_command(handle: Handle, tool: str, repo: str, args: dict) -> list[str]:
    return [*handle.cli, "--output", "json", "--service-runtime-dir", handle.control,
            "tools", "call", tool, "--index", handle.index_id(repo), "--args", json.dumps(args, ensure_ascii=False)]


def call_tool(handle: Handle, tool: str, repo: str, args: dict, timeout_s: float,
              strip_env: tuple[str, ...] = ()) -> CallOutcome:
    """One tool call is one CLI process (the evaluation service owns the work)."""
    command = cli_command(handle, tool, repo, args)
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    started = time.monotonic()
    try:
        proc = subprocess.run(command, capture_output=True, stdin=subprocess.DEVNULL,
                              env=tool_env(handle, strip_env),
                              timeout=timeout_s, creationflags=flags)
    except subprocess.TimeoutExpired as error:
        elapsed = int((time.monotonic() - started) * 1000)
        return CallOutcome(False, None, error.stdout or b"", error.stderr or b"", elapsed, None,
                           f"tool call exceeded {timeout_s:.0f} s")
    except OSError as error:
        elapsed = int((time.monotonic() - started) * 1000)
        return CallOutcome(False, None, b"", str(error).encode(), elapsed, None, f"cannot start CLI: {error}")
    elapsed = int((time.monotonic() - started) * 1000)
    envelope = None
    try:
        parsed = json.loads(proc.stdout)
        if isinstance(parsed, dict):
            envelope = parsed
    except ValueError:
        pass
    ok = proc.returncode == 0 and envelope is not None and envelope.get("isError") is not True
    error = None if ok else f"CLI exit {proc.returncode}" + ("" if envelope is not None else "; stdout is not JSON")
    return CallOutcome(ok, proc.returncode, proc.stdout, proc.stderr, elapsed, envelope, error)


CATALOG_FILE = "tools-catalog.json"


def write_tools_catalog(handle: Handle, fixtures: Path, timeout_s: float = 120.0) -> dict:
    """Write ``<fixtures>/tools-catalog.json``: the live lifetime's own tool listing with complete schemas
    (``tools list --json`` of the lifetime's CLI, against its control root). The wrapper serves agents from
    this file, so a session cannot start in a root without it. The listing depends on the image, not on the
    root: an existing catalog of the same bytes is kept, and a different one is refused, never replaced."""
    command = [*handle.cli, "--output", "json", "--service-runtime-dir", handle.control, "tools", "list", "--json"]
    proc = subprocess.run(command, capture_output=True, stdin=subprocess.DEVNULL, env=tool_env(handle),
                          timeout=timeout_s, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    require(proc.returncode == 0, f"tools list failed (exit {proc.returncode}): "
                                  f"{proc.stderr.decode('utf-8', 'replace')[:300]}")
    path = Path(fixtures) / CATALOG_FILE
    if path.is_file():
        require(path.read_bytes() == proc.stdout,
                f"{path} differs from this lifetime's tool listing; it was written by another image")
    else:
        write_bytes(path, proc.stdout)
    tools = load_catalog(path)
    missing = sorted({tool for names in PATH_TOOLS.values() for tool in names} - set(tools))
    require(not missing, f"{path} lacks {missing}")
    return {"path": str(path), "sha256": sha256_file(path), "tools": len(tools)}


TYPED_RETRY_CODES = ("selection_busy", "publication_busy", "scoped_observation_unavailable",
                     "scoped_status_unavailable")


def typed_retry(envelope: dict | None, index_id: str | None = None) -> tuple[bool, int, str | None]:
    """Whether a reply is a typed retry an MCP client must honour (the server's "Honor typed retries;
    never replay uncertain work"), with the delay in ms and the code.

    Retried only when the service says so (``retryable`` true) and no work is uncertain: either the reply
    asks to ``repeat_same_request`` (status ``retry``), or the error is one of TYPED_RETRY_CODES and the
    query was not submitted (``query_submitted`` is not true).
    """
    body = envelope_body(envelope)
    if not isinstance(body, dict):
        return False, 0, None
    if body.get("status") == "retry" and body.get("retryable") is True and body.get("action") == "repeat_same_request":
        return True, int(body.get("retry_after_ms") or 25), body.get("code")
    error = body.get("error")
    if isinstance(error, dict) and error.get("retryable") is True and error.get("code") == "scoped_connection_unavailable":
        # A new CLI process gets fresh brokerage. Match the original single
        # index and require positive proof that no query ever ran.
        scope = body.get("requested_scope")
        selection = scope.get("selection") if isinstance(scope, dict) else None
        window = selection.get("window_scope_selection") if isinstance(selection, dict) else None
        delay = error.get("retry_after_ms")
        if (index_id is not None and isinstance(window, dict) and window.get("index_ids") == [index_id]
                and error.get("query_submitted") is False and error.get("fresh_brokerage_required") is True
                and error.get("query_replay_safe", True) is True and type(delay) is int and 1 <= delay <= 1000):
            return True, delay, error["code"]
    if isinstance(error, dict) and error.get("retryable") is True and error.get("code") in TYPED_RETRY_CODES \
            and error.get("query_submitted") is not True:
        return True, int(error.get("retry_after_ms") or 250), error.get("code")
    return False, 0, None


def call_tool_retrying(handle: Handle, tool: str, repo: str, args: dict, timeout_s: float,
                       strip_env: tuple[str, ...] = (), attempts: int = 8) -> tuple[CallOutcome, int]:
    """call_tool, repeated while the reply is a typed retry (the same rule as the wrapper); returns the retries."""
    outcome = call_tool(handle, tool, repo, args, timeout_s, strip_env)
    retries = 0
    while retries < attempts:
        retry, delay_ms, _ = typed_retry(outcome.envelope, handle.index_id(repo))
        if not retry:
            break
        time.sleep(min(max(delay_ms, 25) * (2 ** retries), 2000) / 1000)
        retries += 1
        outcome = call_tool(handle, tool, repo, args, timeout_s, strip_env)
    return outcome, retries


def envelope_content(envelope: dict | None) -> list[dict]:
    if not envelope:
        return []
    content = envelope.get("content")
    if isinstance(content, list):
        return [block for block in content if isinstance(block, dict)]
    if "structuredContent" in envelope:
        return [{"type": "text", "text": json.dumps(envelope["structuredContent"], ensure_ascii=False)}]
    return [{"type": "text", "text": json.dumps(envelope, ensure_ascii=False)}]


def envelope_text(envelope: dict | None) -> str:
    return "\n".join(block.get("text", "") for block in envelope_content(envelope) if block.get("type") == "text")


def envelope_body(envelope: dict | None) -> Any:
    """The JSON body of a tool result (structuredContent, else the single text block)."""
    if not envelope:
        return None
    if "structuredContent" in envelope:
        return envelope["structuredContent"]
    texts = [block.get("text", "") for block in envelope_content(envelope) if block.get("type") == "text"]
    if len(texts) == 1:
        try:
            return json.loads(texts[0])
        except ValueError:
            return texts[0]
    return texts or None


def raw_body(raw: bytes) -> Any:
    try:
        envelope = json.loads(raw)
    except ValueError:
        return None
    return envelope_body(envelope) if isinstance(envelope, dict) else None


# ---------------------------------------------------------------- scenarios


def load_scenario(path: Path) -> dict:
    scenario = read_json(path)
    require(scenario.get("schema") == "airk.private.answer-eval-scenario/1", f"{path}: unknown scenario schema")
    require(scenario.get("repository") in KNOWN_REPOS, f"{path}: unknown repository")
    require(scenario.get("slot") in SLOTS, f"{path}: unknown slot")
    notes = scenario.get("notes") or []
    require(4 <= len(notes) <= 10, f"{path}: needs 4 to 10 notes")
    require(sum(n.get("mark") == "essential" for n in notes) >= 3, f"{path}: needs at least 3 essential notes")
    for note in notes:
        require(isinstance(note.get("path"), str), f"{path}: note {note.get('id')} has no path")
        if note.get("mark") == "essential":
            require(bool(note.get("fact")), f"{path}: essential note {note.get('id')} has no fact")
    scenario["_path"] = str(path)
    scenario["_sha256"] = sha256_file(path)
    return scenario


def scenario_files(directory: Path, repos=REPOS, slots=SLOTS) -> list[Path]:
    return [Path(directory) / repo / f"{slot}.json" for repo in repos for slot in slots
            if (Path(directory) / repo / f"{slot}.json").is_file()]


# ---------------------------------------------------------------- processes


def _version_key(path: Path) -> tuple:
    match = path.parents[2].name.split("claude-code-")[-1].split("-")[0]
    return tuple(int(p) if p.isdigit() else 0 for p in match.split("."))


def find_claude() -> str:
    """The native Claude Code executable.

    Preference: the newest installed VS Code extension binary (the npm global
    install, 2.1.118 on 21 Nov 2026, refuses claude-opus-5-5 and needs 2.1.280
    or newer), then the npm install's native binary, then whatever is on PATH.
    Nothing is installed or updated here. Every session records the path and
    ``--version`` it used.
    """
    extensions = Path.home() / ".vscode" / "extensions"
    candidates = sorted(extensions.glob("anthropic.claude-code-*/resources/native-binary/claude.exe"), key=_version_key)
    if candidates:
        return str(candidates[-1])
    shim = shutil.which("claude.cmd") or shutil.which("claude")
    require(shim is not None, "claude is not on PATH")
    native = Path(shim).parent / "node_modules" / "@anthropic-ai" / "claude-code" / "bin" / "claude.exe"
    return str(native) if native.is_file() else shim


def clean_child_env(extra: dict[str, str] | None = None) -> dict[str, str]:
    """Environment for a headless Claude Code child: no AIRK settings, no parent-session markers."""
    drop = {"CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SSE_PORT"}
    env = {k: v for k, v in os.environ.items() if k.upper() not in drop and not k.upper().startswith("AIRK_")}
    env.update({"CLAUDE_CODE_DISABLE_AUTO_MEMORY": "1", "DISABLE_AUTOUPDATER": "1",
                "ENABLE_TOOL_SEARCH": "false", "ENABLE_CLAUDEAI_MCP_SERVERS": "false"})
    env.update(extra or {})
    return env


def kill_tree(pid: int) -> None:
    if sys.platform == "win32":
        subprocess.run(["taskkill", "/T", "/F", "/PID", str(pid)], capture_output=True,
                       creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    else:  # pragma: no cover - the harness runs on Windows
        try:
            os.killpg(pid, 9)
        except OSError:
            pass


def claude_version(claude: str) -> str:
    proc = subprocess.run([claude, "--version"], capture_output=True, text=True, timeout=60,
                          creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    return proc.stdout.strip()


def parse_stream(path: Path) -> dict:
    """Summarize a stream-json transcript: init, tool uses, final result."""
    init, result, tool_uses, assistant_texts = None, None, [], []
    for event in read_jsonl(path) if Path(path).is_file() else []:
        kind = event.get("type")
        if kind == "system" and event.get("subtype") == "init" and init is None:
            init = event
        elif kind == "assistant":
            for block in (event.get("message") or {}).get("content") or []:
                if block.get("type") == "tool_use":
                    tool_uses.append({"name": block.get("name"), "id": block.get("id"), "input": block.get("input")})
                elif block.get("type") == "text":
                    assistant_texts.append(block.get("text", ""))
        elif kind == "result":
            result = event
    return {"init": init, "result": result, "tool_uses": tool_uses, "assistant_texts": assistant_texts}


def read_stream_lines(raw: bytes) -> list[dict]:
    events = []
    for line in raw.splitlines():
        line = line.strip()
        if not line.startswith(b"{"):
            continue
        try:
            events.append(json.loads(line))
        except ValueError:
            continue
    return events
