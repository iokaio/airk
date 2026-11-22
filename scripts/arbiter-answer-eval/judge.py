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
schema for its reply. Two independent judges are two such sessions (j1, j2).
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402

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


def run_judge(*, scenario: dict, session_dir: Path, key: str, out_dir: Path, judge: str = "j1",
              model: str = common.JUDGE_MODEL, effort: str | None = common.JUDGE_EFFORT,
              claude: str | None = None, timeout_s: float = 900.0,
              rerun: bool = False) -> dict:
    out = Path(out_dir) / judge
    verdict_path = out / "verdict.json"
    if verdict_path.is_file() and not rerun:
        return common.read_json(verdict_path)
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
    args = parser.parse_args(argv)
    scenario = common.load_scenario(args.scenario)
    if args.packet_only:
        print(json.dumps(build_packet(scenario, args.session_dir, args.key), indent=1, ensure_ascii=False))
        return 0
    status = 0
    for name in args.judges.split(","):
        record = run_judge(scenario=scenario, session_dir=args.session_dir, key=args.key, out_dir=args.out_dir,
                           judge=name, model=args.model, effort=args.effort, rerun=args.rerun, claude=args.claude)
        print(json.dumps({"judge": name, "status": record["status"], "cost_usd": record.get("cost_usd"),
                          "problems": record["problems"]}))
        status |= record["status"] != "valid"
    return status


if __name__ == "__main__":
    sys.exit(main())
