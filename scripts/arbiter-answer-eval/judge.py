# SPDX-License-Identifier: Apache-2.0
"""Blind judging of one result (plan 4.1, 4.2, 4.5).

``build_packet`` makes the judging packet: the instruction, the reference
notes with their facts, the agent's brief, the sequence of calls the agent made
and the text of the first reply (for Rubric B). Path names are replaced by
neutral labels: the primary tool becomes ``primary_tool``, the ``airk_``
prefix and result schema names are removed. Answer's reply keeps a
recognizable shape, so the blinding is partial (plan 4.5).

``run_judge`` launches one fresh headless Claude Code session on
``claude-opus-5-5`` with every tool removed (``--tools ''``), no settings,
CLAUDE.md or memory (``--setting-sources ''``), no MCP servers, and a JSON
schema for its reply. The opt-in Codex path starts a fresh isolated
app-server thread for Astra or Sol and uses a strict translation of the
same verdict schema. Two independent judges use separate sessions (j1, j2).
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path
from typing import TYPE_CHECKING

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402

if TYPE_CHECKING:
    from run_session import CodexRunConfig

PACKET_SCHEMA = "airk.private.answer-eval-judge-packet/1"
VERDICT_SCHEMA = "airk.private.answer-eval-verdict/1"
FIRST_REPLY_CAP = 150_000
RUBRIC_B_KEYS = ("find", "choice", "trace", "useful")

JUDGE_SYSTEM = """You are an independent judge in an evaluation of tools that gather repository context for a software development agent. You have no tools. Everything you need is in the packet in the user message. Judge only from the packet.

The packet holds: a task instruction; reference notes written by a person who read the repository directly (each essential note states a fact the agent must take from one place); the agent's work brief; the calls the agent made; and the text of the first reply the agent received.

Judge three things.

1. For every ESSENTIAL note, did the brief state the note's fact?
   - "correct": the brief states the fact, or something that clearly implies it, without contradicting it. Wording may differ. A location alone is not enough; the brief must convey the rule, behaviour or constraint.
   - "wrong": the brief states something about this point that contradicts the fact or would lead the agent to act against it.
   - "missing": the brief does not convey the fact.
   Give a one-line reason for each.

2. Misleading evidence: did irrelevant or wrong material send the agent down a wrong path? Use the brief and the calls. Answer true only if the brief or the calls show that the agent followed it, for example by planning work in the wrong place or relying on a wrong rule. Give a one-line reason.

3. Rubric B, on the FIRST REPLY only, as a person would read it for the original instruction. Score each question from 0 to 4, using the anchors, and give a one-line reason:
   - find: Can a person find the relevant detail quickly? 0 = buried or absent; 2 = present, needs hunting; 4 = organized so the relevant parts stand out.
   - choice: Does it surface detail that would change a choice: constraints, rules, alternatives, risks? 0 = none; 2 = some, incidental; 4 = the governing rules and trade-offs are in view.
   - trace: Can each statement be traced to an exact source location, and is anything cut off disclosed? 0 = no locations; 2 = locations without ranges, or silent truncation; 4 = exact locations, truncation disclosed.
   - useful: How much of it is useful? 0 = mostly noise or duplicates; 2 = mixed; 4 = little noise, little duplication.
   If the packet says there is no first reply, score every question 0 and say so.

Reply with JSON only, matching the given schema."""


def verdict_schema() -> dict:
    score = {"type": "object", "properties": {"score": {"type": "integer", "minimum": 0, "maximum": 4},
                                              "reason": {"type": "string"}}, "required": ["score", "reason"]}
    return {
        "type": "object",
        "properties": {
            "notes": {"type": "array", "items": {
                "type": "object",
                "properties": {"id": {"type": "string"}, "verdict": {"enum": ["correct", "wrong", "missing"]},
                               "reason": {"type": "string"}},
                "required": ["id", "verdict", "reason"]}},
            "misleading_followed": {"type": "boolean"},
            "misleading_reason": {"type": "string"},
            "rubric_b": {"type": "object", "properties": {k: score for k in RUBRIC_B_KEYS},
                         "required": list(RUBRIC_B_KEYS)},
        },
        "required": ["notes", "misleading_followed", "misleading_reason", "rubric_b"],
    }


_SCHEMA_NAMES = re.compile(r"airk\.(?:arbiter\.context(?:\.request)?|workbench\.result|parse\.request)/\d+")


def neutralize(text: str, primary: str) -> str:
    text = text.replace(f"mcp__{common.SERVER_NAME}__", "")
    text = text.replace(primary, "primary_tool")
    for tool in ("airk_answer", "airk_search", "airk_parse"):
        text = text.replace(tool, "primary_tool")
    text = _SCHEMA_NAMES.sub("<schema>", text)
    return re.sub(r"\bairk_(read_file|context|references|symbols|outline)\b", r"\1", text)


def result_label(key: str) -> str:
    return "Result-" + common.sha256_bytes(key.encode("utf-8"))[:8]


def build_packet(scenario: dict, session_dir: Path, key: str) -> dict:
    session_dir = Path(session_dir)
    record = common.read_json(session_dir / "session.json")
    primary = common.PATH_TOOLS[record["path"]][0]
    turns = [t for t in common.read_jsonl(session_dir / "wrapper" / "turns.jsonl") if t.get("turn")]
    brief = (session_dir / "brief.md").read_text(encoding="utf-8") if (session_dir / "brief.md").is_file() else ""
    first = {"present": False, "text": "", "bytes": 0, "truncated_for_packet": False}
    if turns and turns[0].get("result_path") and Path(turns[0]["result_path"]).is_file():
        raw = Path(turns[0]["result_path"]).read_bytes()
        try:
            text = common.envelope_text(json.loads(raw))
        except ValueError:
            text = raw.decode("utf-8", "replace")
        first = {"present": bool(turns[0].get("ok")), "bytes": len(text.encode("utf-8")),
                 "truncated_for_packet": len(text) > FIRST_REPLY_CAP,
                 "text": neutralize(text[:FIRST_REPLY_CAP], primary)}
    calls = [{"turn": t["turn"], "tool": "primary_tool" if t["tool"] == primary else t["tool"].replace("airk_", ""),
              "arguments": json.loads(neutralize(json.dumps(t.get("arguments"), ensure_ascii=False), primary)),
              "ok": t.get("ok"), "bytes": t.get("bytes")} for t in turns]
    notes = [{"id": n["id"], "mark": n["mark"],
              "location": f"{n['path']}:{n['lines'][0]}-{n['lines'][1]}" if n.get("lines") else n["path"],
              "symbol": n.get("symbol"), "fact": n.get("fact")} for n in scenario["notes"]]
    return {"schema": PACKET_SCHEMA, "result": result_label(key), "instruction": scenario["instruction"],
            "notes": notes, "brief": neutralize(brief, primary), "calls": calls, "first_reply": first,
            "session_status": record.get("status")}


def packet_prompt(packet: dict) -> str:
    return ("Judge this result. The packet is JSON.\n\n" + json.dumps(packet, indent=1, ensure_ascii=False)
            + "\n\nReply with the JSON verdict only.")


def judge_command(claude: str, model: str, effort: str | None = common.JUDGE_EFFORT) -> list[str]:
    common.require(effort is None or effort in common.EFFORTS, f"unknown effort {effort!r}")
    return [claude, "-p", "--model", model, *(["--effort", effort] if effort else []),
            "--setting-sources", "", "--strict-mcp-config", "--tools", "",
            "--permission-mode", "dontAsk", "--disable-slash-commands", "--no-session-persistence",
            "--system-prompt", JUDGE_SYSTEM, "--output-format", "json",
            "--json-schema", json.dumps(verdict_schema(), separators=(",", ":"))]


def parse_verdict(output: dict) -> dict | None:
    structured = output.get("structured_output")
    if isinstance(structured, dict):
        return structured
    text = output.get("result")
    if not isinstance(text, str):
        return None
    text = text.strip()
    fence = re.match(r"^```(?:json)?\s*(.*?)\s*```$", text, re.S)
    if fence:
        text = fence.group(1)
    try:
        value = json.loads(text)
    except ValueError:
        start, end = text.find("{"), text.rfind("}")
        if start < 0 or end <= start:
            return None
        try:
            value = json.loads(text[start:end + 1])
        except ValueError:
            return None
    return value if isinstance(value, dict) else None


def validate(verdict: dict | None, scenario: dict) -> list[str]:
    if verdict is None:
        return ["no JSON verdict"]
    problems = []
    essential = [n["id"] for n in scenario["notes"] if n["mark"] == "essential"]
    given = {}
    for row in verdict.get("notes") or []:
        if isinstance(row, dict) and row.get("id") in essential:
            if row["id"] in given:
                problems.append(f"note {row['id']} judged twice")
            given[row["id"]] = row.get("verdict")
    missing = [n for n in essential if n not in given]
    if missing:
        problems.append(f"essential notes not judged: {missing}")
    bad = [n for n, v in given.items() if v not in ("correct", "wrong", "missing")]
    if bad:
        problems.append(f"invalid note verdicts: {bad}")
    if not isinstance(verdict.get("misleading_followed"), bool):
        problems.append("misleading_followed is not a boolean")
    rubric = verdict.get("rubric_b") or {}
    for key in RUBRIC_B_KEYS:
        score = (rubric.get(key) or {}).get("score") if isinstance(rubric.get(key), dict) else None
        if not (isinstance(score, int) and 0 <= score <= 4):
            problems.append(f"rubric_b.{key} score invalid")
    return problems


CODEX_MODELS = ("gpt-6.2-astra", "gpt-6.2-sol")


def codex_verdict_schema() -> dict:
    """Close the same verdict schema for the app-server strict output format."""
    def close(node):
        if "properties" in node:
            return {**node, "properties": {key: close(value) for key, value in node["properties"].items()},
                    "additionalProperties": False}
        if "items" in node:
            return {**node, "items": close(node["items"])}
        if "enum" in node and "type" not in node:
            return {"type": "string", **node}
        return node
    return close(verdict_schema())


def exact_json(text: str) -> dict:
    def unique_object(pairs):
        value = dict(pairs)
        if len(value) != len(pairs):
            raise ValueError("duplicate verdict key")
        return value

    value = json.loads(text, object_pairs_hook=unique_object)
    if not isinstance(value, dict):
        raise ValueError("expected a JSON object")
    return value


def matches_verdict_schema(value, schema) -> bool:
    kind = schema["type"]
    if kind == "object":
        props = schema["properties"]
        return (type(value) is dict and set(value) == set(schema["required"])
                and all(matches_verdict_schema(value[key], prop) for key, prop in props.items()))
    if kind == "array":
        return type(value) is list and all(matches_verdict_schema(item, schema["items"]) for item in value)
    if kind == "string":
        return type(value) is str and ("enum" not in schema or value in schema["enum"])
    if kind == "boolean":
        return type(value) is bool
    if kind == "integer":
        return type(value) is int and schema["minimum"] <= value <= schema["maximum"]
    raise ValueError("unexpected verdict schema type")


def codex_usage(status: dict) -> dict | None:
    raw = status.get("usage")
    if not isinstance(raw, dict):
        return None
    observed = raw.get("total")
    fields = {"inputTokens": "input_tokens", "cachedInputTokens": "cached_input_tokens",
              "outputTokens": "output_tokens", "reasoningOutputTokens": "reasoning_output_tokens",
              "totalTokens": "total_tokens"}
    if not isinstance(observed, dict) or any(
        type(observed.get(key)) is not int or observed[key] < 0 for key in fields
    ):
        return None
    return {name: observed[key] for key, name in fields.items()}


def validate_codex_judge(exit_code, status: dict, model: str, scenario: dict) -> tuple:
    if exit_code is None:
        return None, None, ["judge timed out"]
    if exit_code != 0 or status.get("turn_status") != "completed":
        return None, None, ["Codex judge did not complete"]
    if (status.get("model_observed") != model or status.get("effort_observed") != "medium"
        or status.get("provider_observed") != "openai" or status.get("auth_route") != "chatgpt"
        or status.get("configured_mcp") != {} or status.get("attempts") != []
        or status.get("instructions_count") != 0
        or not all(isinstance(status.get(key), str) and status[key] for key in ("thread_id", "turn_id"))):
        return None, None, ["Codex judge identity or tool isolation differs"]
    usage = codex_usage(status)
    if usage is None:
        return None, None, ["Codex judge token usage is unavailable"]
    final = status.get("final")
    try:
        verdict = exact_json(final)
    except (ValueError, TypeError):
        return None, usage, ["no exact JSON verdict"]
    if not matches_verdict_schema(verdict, codex_verdict_schema()):
        return None, usage, ["Codex verdict does not match the strict schema"]
    problems = validate(verdict, scenario)
    expected = {note["id"] for note in scenario["notes"] if note["mark"] == "essential"}
    if any(note["id"] not in expected for note in verdict["notes"]):
        problems.append("verdict judges notes outside the essential roster")
    return verdict, usage, problems


def run_codex_judge(config, *, record, model, prompt, work, out, timeout_s, scenario):
    from codex_isolation_probe import run_app_server
    from run_session import codex_child_env, codex_command, codex_provenance

    schema = codex_verdict_schema()
    record.update({"provider": "codex", "stdin": "app-server JSON-RPC",
                   "output_schema_sha256": common.sha256_bytes(json.dumps(
                       schema, sort_keys=True, separators=(",", ":")).encode("utf-8"))})
    began = time.monotonic()
    try:
        record["codex"] = codex_provenance(config)
        command = codex_command(config, work)
        record["command"] = command
        for name in ("codex-tmp", "codex-logs", "codex-state"):
            (work / name).mkdir()
        child_env = codex_child_env(config, work)
        exit_code, transcript, stderr, status = run_app_server(
            command, child_env, work, model=model, effort="medium", instruction=prompt,
            tool_catalog=[], clean_home=True, output_schema=schema, base_instructions=JUDGE_SYSTEM,
            timeout=timeout_s, require_chatgpt=True,
        )
    except subprocess.TimeoutExpired:
        exit_code, transcript, stderr, status = None, b"", b"", {}
    except (common.HarnessError, OSError, subprocess.SubprocessError, ValueError) as error:
        exit_code, transcript, stderr, status = 2, b"", b"", {"launch_failure": type(error).__name__}
    record["ended"] = common.now_iso()
    record["elapsed_ms"] = int((time.monotonic() - began) * 1000)
    common.write_bytes(out / "app-server.jsonl", transcript)
    if stderr:
        common.write_bytes(out / "stderr.txt", stderr)
    verdict, usage, problems = validate_codex_judge(exit_code, status, model, scenario)
    common.write_json(out / "output.json", status)
    observed_model = status.get("model_observed")
    record.update({"exit_code": exit_code, "output_sha256": common.sha256_file(out / "output.json"),
                   "app_server_sha256": common.sha256_bytes(transcript),
                   "cost_usd": None, "usage": usage, "model_usage": None,
                   "models_used": [observed_model] if observed_model else [],
                   "session_id": status.get("thread_id"), "codex_turn_id": status.get("turn_id"),
                   "verdict": verdict, "problems": problems,
                   "status": "valid" if not problems else "invalid"})
    return record


def run_judge(*, scenario: dict, session_dir: Path, key: str, out_dir: Path, judge: str = "j1",
              model: str = common.JUDGE_MODEL, effort: str | None = common.JUDGE_EFFORT,
              claude: str | None = None, timeout_s: float = 900.0,
              rerun: bool = False, codex: CodexRunConfig | None = None) -> dict:
    if codex is not None:
        common.require(claude is None and model in CODEX_MODELS and effort == "medium",
                       "Codex judging requires the exact Astra or Sol model and medium effort")
    out = Path(out_dir) / judge
    verdict_path = out / "verdict.json"
    if verdict_path.is_file() and not rerun:
        cached = common.read_json(verdict_path)
        if codex is not None:
            from run_session import codex_provenance

            packet = build_packet(scenario, session_dir, key)
            packet_bytes = (json.dumps(packet, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
            common.require(cached.get("provider") == "codex" and cached.get("key") == key
                           and cached.get("judge") == judge
                           and cached.get("model_requested") == model
                           and cached.get("effort_requested") == effort
                           and cached.get("codex") == codex_provenance(codex)
                           and cached.get("packet_sha256") == common.sha256_bytes(packet_bytes)
                           and cached.get("output_sha256") == common.sha256_file(out / "output.json")
                           and cached.get("app_server_sha256") == common.sha256_file(out / "app-server.jsonl"),
                           "cached judge has different Codex evidence; use an explicit rerun")
        else:
            common.require(cached.get("provider") != "codex",
                           "cached judge belongs to Codex; use an explicit rerun")
        return cached
    if out.exists():
        attempt = 1
        while out.with_name(f"{judge}.attempt-{attempt}").exists():
            attempt += 1
        out.rename(out.with_name(f"{judge}.attempt-{attempt}"))
    work = out / "work"
    work.mkdir(parents=True)
    packet = build_packet(scenario, session_dir, key)
    common.write_json(out / "packet.json", packet)
    prompt = packet_prompt(packet)
    if codex is not None:
        record = {"schema": VERDICT_SCHEMA, "key": key, "judge": judge, "model_requested": model,
                  "effort_requested": effort, "packet_sha256": common.sha256_file(out / "packet.json"),
                  "session_dir": str(session_dir), "started": common.now_iso()}
        record = run_codex_judge(codex, record=record, model=model, prompt=prompt, work=work,
                                 out=out, timeout_s=timeout_s, scenario=scenario)
        common.write_json(verdict_path, record)
        return record
    claude = claude or common.find_claude()
    command = judge_command(claude, model, effort)
    record = {"schema": VERDICT_SCHEMA, "key": key, "judge": judge, "model_requested": model,
              "effort_requested": effort, "claude_path": claude,
              "claude_version": common.claude_version(claude), "command": command, "stdin": "packet prompt",
              "packet_sha256": common.sha256_file(out / "packet.json"), "session_dir": str(session_dir),
              "started": common.now_iso()}
    began = time.monotonic()
    try:
        proc = subprocess.run(command, cwd=work, input=prompt.encode("utf-8"), capture_output=True,
                              env=common.clean_child_env(), timeout=timeout_s,
                              creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        stdout, stderr, exit_code, timed_out = proc.stdout, proc.stderr, proc.returncode, False
    except subprocess.TimeoutExpired as error:
        stdout, stderr, exit_code, timed_out = error.stdout or b"", error.stderr or b"", None, True
    record["ended"] = common.now_iso()
    record["elapsed_ms"] = int((time.monotonic() - began) * 1000)
    common.write_bytes(out / "output.json", stdout)
    if stderr:
        common.write_bytes(out / "stderr.txt", stderr)
    output = {}
    try:
        output = json.loads(stdout)
    except ValueError:
        pass
    verdict = parse_verdict(output) if isinstance(output, dict) else None
    problems = ["judge timed out"] if timed_out else validate(verdict, scenario)
    record.update({"exit_code": exit_code, "output_sha256": common.sha256_bytes(stdout),
                   "cost_usd": output.get("total_cost_usd"), "usage": output.get("usage"),
                   "model_usage": output.get("modelUsage"), "models_used": sorted((output.get("modelUsage") or {}).keys()),
                   "session_id": output.get("session_id"), "verdict": verdict, "problems": problems,
                   "status": "valid" if not problems else "invalid"})
    common.write_json(verdict_path, record)
    return record


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--scenario", type=Path, required=True)
    parser.add_argument("--session-dir", type=Path, required=True)
    parser.add_argument("--key", required=True, help="result key, e.g. airk-C1/focus-cpu")
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--judges", default="j1,j2")
    parser.add_argument("--model", default=common.JUDGE_MODEL)
    parser.add_argument("--effort", default=common.JUDGE_EFFORT, choices=common.EFFORTS)
    parser.add_argument("--packet-only", action="store_true")
    parser.add_argument("--rerun", action="store_true")
    parser.add_argument("--claude", help="Claude Code executable (default: common.find_claude())")
    parser.add_argument("--codex", type=Path, help="Pinned Codex executable")
    parser.add_argument("--codex-sha256")
    parser.add_argument("--model-catalog", type=Path)
    parser.add_argument("--model-catalog-sha256")
    parser.add_argument("--codex-home", type=Path)
    parser.add_argument("--freeze-sha256")
    args = parser.parse_args(argv)
    pins = (args.codex, args.codex_sha256, args.model_catalog,
            args.model_catalog_sha256, args.codex_home, args.freeze_sha256)
    codex = None
    if any(value is not None for value in pins):
        if not all(value is not None for value in pins) or args.claude:
            parser.error("Codex judging requires all six pins and excludes --claude")
        if args.model not in CODEX_MODELS or args.effort != "medium":
            parser.error("Codex judging requires an explicit Astra or Sol model and medium effort")
        from run_session import CodexRunConfig

        codex = CodexRunConfig(*pins)
    scenario = common.load_scenario(args.scenario)
    if args.packet_only:
        print(json.dumps(build_packet(scenario, args.session_dir, args.key), indent=1, ensure_ascii=False))
        return 0
    status = 0
    for name in args.judges.split(","):
        record = run_judge(scenario=scenario, session_dir=args.session_dir, key=args.key, out_dir=args.out_dir,
                           judge=name, model=args.model, effort=args.effort, rerun=args.rerun,
                           claude=args.claude, codex=codex)
        print(json.dumps({"judge": name, "status": record["status"], "cost_usd": record.get("cost_usd"),
                          "problems": record["problems"]}))
        status |= record["status"] != "valid"
    return status


if __name__ == "__main__":
    sys.exit(main())
