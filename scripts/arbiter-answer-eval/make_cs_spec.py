# SPDX-License-Identifier: Apache-2.0
"""Write the pass spec of a change-set pass for one repository.

The pass runs Haiku/medium agents and two Sonnet/medium judges, under the
frozen prompts and scenarios. Search and Parse are reused from the named
baseline when their canonical digest is unchanged. The configurations are
noModel, Focus and Focus with Compact (judged), and Focus with Large on the
named slots (timing only). The change set's Focus and Context profile names
are arguments.

``python -I -B make_cs_spec.py --repo bookbuilder --baseline baseline-bookbuilder-r2 --large A1,C2 --series cs1-bookbuilder``
writes ``<R>/passes/<series>/spec.json``.
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

CS1_BUDGET = {"calls": 12, "rounds": 3, "fanout": 12, "candidates": 32, "max_bytes": 524288,
              "input_tokens": 8192, "deadline_ms": 120000}
CLAUDE = r"C:\Users\tdotn\.vscode\extensions\anthropic.claude-code-2.1.330-win32-x64\resources\native-binary\claude.exe"


def spec(repo: str, baseline: str, large: list[str], series: str, focus: str, compact: str, large_profile: str,
         r_root: Path, note: str) -> dict:
    stages = {"focus": focus}
    return {
        "series": series, "pilot": False, "note": note,
        "scenarios_dir": str(r_root / "scenarios"), "repos": [repo],
        "scenario_ids": [f"{repo}-{slot}" for slot in sorted(common.SLOTS)],
        "freeze": str(r_root / "scenario-freeze.json"), "prompts_freeze": str(r_root / "design" / "prompts-freeze.json"),
        "settle_repos": [repo], "handle": common.NEWEST, "claude": CLAUDE,
        "agent_model": common.AGENT_MODEL, "agent_effort": common.AGENT_EFFORT,
        "judge_model": common.JUDGE_MODEL, "judge_effort": common.JUDGE_EFFORT, "judges": ["j1", "j2"],
        "concurrency": 4, "judge_concurrency": 4, "prior_series": [baseline],
        # Decision 5 (22 Nov 2026): change set 1 sends its own Answer budget in every airk_answer call.
        "answer_defaults": {"budget": dict(CS1_BUDGET)},
        "results": [
            {"configuration": "search", "path": "search", "device": "none", "stages": {}},
            {"configuration": "parse", "path": "parse", "device": "none", "stages": {}},
            {"configuration": "noModel", "path": "noModel", "device": "none", "stages": {}},
            {"configuration": "focus", "path": "focus", "device": "cpu", "stages": stages},
            {"configuration": "compact", "path": "compact", "device": "cpu", "stages": {**stages, "context": compact}},
            {"configuration": "large", "path": "large", "device": "cpu", "stages": {**stages, "context": large_profile},
             "timing_only": True, "slots": large, "answer_defaults": {"budget": dict(CS1_BUDGET)},
             "call_timeout": 180},
        ],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--repo", required=True, choices=common.REPOS)
    parser.add_argument("--baseline", required=True)
    parser.add_argument("--large", required=True, help="slots for the timing-only Large run, e.g. A1,C2")
    parser.add_argument("--series", required=True)
    parser.add_argument("--focus", default="focus-static1024-cpu-cs1-software-w16")
    parser.add_argument("--compact", default="context-compact-cpu-cs1-provisional-read")
    parser.add_argument("--large-profile", default="context-large-cpu-cs1-provisional-read")
    parser.add_argument("--note", default="")
    parser.add_argument("--r-root", type=Path, default=common.DEFAULT_R)
    args = parser.parse_args(argv)
    value = spec(args.repo, args.baseline, args.large.split(","), args.series, args.focus, args.compact,
                 args.large_profile, args.r_root, args.note)
    out = args.r_root / "passes" / args.series / "spec.json"
    common.write_json(out, value)
    print(out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
