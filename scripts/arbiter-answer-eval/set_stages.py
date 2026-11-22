# SPDX-License-Identifier: Apache-2.0
"""Set the live Arbiter stages of the newest (or named) fixture lifetime to a wanted pair, then verify.

The script uses ``run_baseline.Runner.set_stages``. It disables a Context that
is not wanted, switches Focus, enables the wanted Context, and verifies the
result live (``arbiter status-legacy``). Profile names are those of
``fixture.py stage``.

``python -I -B set_stages.py --focus focus-static1024-cpu-cs1-software-w16 --context none --log <file>``
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
import run_baseline  # noqa: E402


class _Stager(run_baseline.Runner):
    def __init__(self, handle: str, r_root: Path, log_dir: Path):  # noqa: D401 - a minimal Runner
        self.spec = {"handle": handle}
        self.r_root = r_root
        self.pass_dir = log_dir
        self.pass_dir.mkdir(parents=True, exist_ok=True)
        self.log_path = log_dir / "stages-run.jsonl"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--focus", required=True, help="profile name or none")
    parser.add_argument("--context", required=True, help="profile name or none")
    parser.add_argument("--handle", default=common.NEWEST)
    parser.add_argument("--log-dir", type=Path, required=True)
    parser.add_argument("--r-root", type=Path, default=common.DEFAULT_R)
    args = parser.parse_args(argv)
    wanted = {"focus": None if args.focus == "none" else args.focus,
              "context": None if args.context == "none" else args.context}
    stager = _Stager(args.handle, args.r_root, args.log_dir)
    stager.set_stages(wanted)
    print(json.dumps({"stages": wanted}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
