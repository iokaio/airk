# SPDX-License-Identifier: Apache-2.0
"""Count where named source files appeared in private evaluation logs, without printing any of their text.

The scan covers every raw tool result (``*.stdout.json`` and the fixture CLI logs' ``*.stdout``), every judging
packet (``packet.json``: its first reply), every agent transcript
(``stream.jsonl``: the tool results the agent received) and every brief
(``brief.md``: path mentions only). Each is checked for evidence located in
one of the ``--paths``, using ``evidence.extract``. The scan reports only
counts: results with such evidence by kind (search or context hit, read_file,
outline or symbols row, references line), the bytes of returned text, and the
log files and session folders involved. It never prints or stores the text
itself, and it never opens the named files.

``python -I -B exposure_scan.py --paths a/b.ps1,c/d.ps1 --roots <dir>[,<dir>] --out <R>/security/exposure-scan.json``
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import common  # noqa: E402
import evidence  # noqa: E402


def matching(spans: list[evidence.Span], targets: list[str]) -> list[evidence.Span]:
    return [s for s in spans if any(evidence.path_matches(s.path, t) for t in targets)]


def tally(spans: list[evidence.Span]) -> dict:
    kinds: dict[str, int] = {}
    for s in spans:
        kinds[s.kind] = kinds.get(s.kind, 0) + 1
    return {"spans": len(spans), "kinds": kinds,
            "text_bytes": sum(len(s.text.encode("utf-8")) for s in spans if s.kind in ("text", "fact"))}


def body_spans(text: str) -> list[evidence.Span]:
    try:
        body = json.loads(text)
    except ValueError:
        return []
    if isinstance(body, dict) and "content" in body:
        body = common.envelope_body(body)
    return evidence.extract(body) if body is not None else []


def scan(roots: list[Path], targets: list[str]) -> dict:
    hits = {"raw_results": [], "judge_packets": [], "transcripts": [], "briefs": []}
    for root in roots:
        for path in [*root.rglob("*.stdout.json"), *root.rglob("*.stdout")]:
            found = matching(body_spans(path.read_text(encoding="utf-8", errors="replace")), targets)
            if found:
                hits["raw_results"].append({"file": str(path), **tally(found)})
        for path in root.rglob("packet.json"):
            try:
                packet = common.read_json(path)
            except ValueError:
                continue
            text = (packet.get("first_reply") or {}).get("text") or ""
            found = matching(body_spans(text), targets)
            mentions = sum(json.dumps(packet).count(t) for t in targets)
            if found or mentions:
                hits["judge_packets"].append({"file": str(path), "path_mentions": mentions, **tally(found)})
        for path in root.rglob("stream.jsonl"):
            found = []
            for event in common.read_jsonl(path):
                if event.get("type") != "user":
                    continue
                for block in (event.get("message") or {}).get("content") or []:
                    if isinstance(block, dict) and block.get("type") == "tool_result":
                        content = block.get("content")
                        texts = [c.get("text", "") for c in content if isinstance(c, dict)] \
                            if isinstance(content, list) else [content or ""]
                        for text in texts:
                            found += matching(body_spans(text), targets)
            if found:
                hits["transcripts"].append({"file": str(path), "session_dir": str(path.parent), **tally(found)})
        for path in root.rglob("brief.md"):
            text = path.read_text(encoding="utf-8", errors="replace")
            mentions = sum(text.count(t) for t in targets)
            if mentions:
                hits["briefs"].append({"file": str(path), "path_mentions": mentions})
    totals = {k: {"files": len(v), "text_bytes": sum(x.get("text_bytes", 0) for x in v)} for k, v in hits.items()}
    sessions = sorted({str(Path(x["file"]).parents[2]) for x in hits["raw_results"] if "wrapper" in x["file"]}
                      | {x["session_dir"] for x in hits["transcripts"]})
    return {"targets": targets, "roots": [str(r) for r in roots], "totals": totals, "sessions": sessions,
            "any_text": any(t["text_bytes"] for t in totals.values()), **hits}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--paths", required=True)
    parser.add_argument("--roots", required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    report = {"schema": "airk.private.answer-eval-exposure-scan/1", "generated": common.now_iso(),
              **scan([Path(r) for r in args.roots.split(",")], args.paths.split(","))}
    common.write_json(args.out, report)
    print(json.dumps({k: report[k] for k in ("totals", "any_text")}, indent=1))
    print(json.dumps({"sessions": report["sessions"],
                      "judge_packets": [x["file"] for x in report["judge_packets"]]}, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
