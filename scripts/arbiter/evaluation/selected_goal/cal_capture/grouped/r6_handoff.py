# SPDX-License-Identifier: Apache-2.0
"""Bind R6 to the actual Q598-r2 stopped state and the last matching native idle status."""
from pathlib import Path
import runpy

from r6_preflight import CAMPAIGN, ROOT, original, require

MODULES = {
    "reuse_installation.py": "e3989a250b10c4ee945423105af278fcb61dfc219262c3af0fb20c5fa5b91a60",
    "q598_closed_restart.py": "d7db2362db47d613c22fa4ca61922898d67bd2cce80c76542d692b5a347c4234",
    "q598_search_handoff.py": "e61c6f3da3c3562d1e26daf804d07d483cc72502f92d72f577d005e8fb9ee072",
}
RETIREMENT = "f45abb479f15087f860e0a0de0261f7ac7ff4044e88947fb0f2378a5ee4bc771"
IDLE = "30dacd28b6d15f240791d29fdbf42734be68dc9bdb85674780bb2d4d3298b84d"


def evidence(pin):
    folder = ROOT / "scripts/arbiter/analysis/development_margins"
    return {
        "sources": [pin(folder / name, digest) for name, digest in MODULES.items()],
        "retired": pin(CAMPAIGN / "compact-development-TRAIN_Q598-focus1024-run-r2/owned.json", RETIREMENT),
        "idle": pin(CAMPAIGN / "cal100-search-mode-timing-r1/commands/stage-after.stdout", IDLE),
    }


def verify(record, installation, pin):
    require(record == evidence(pin), "R6 handoff differs from its reviewed native retirement")
    reuse, closed, idle = [runpy.run_path(row["path"]) for row in record["sources"]]
    prior = record["retired"]
    search = closed["verify"](reuse, Path(prior["path"]), prior["sha256"])
    ticket = idle["predecessor"](reuse, Path(search["path"]), search["sha256"], installation)
    expected = {k: {f: installation[k][f] for f in ("path","sha256")} for k in ("ready", "closed", "status")}
    require(ticket == expected, "native restart ticket differs from retained installation")
    now = original(Path(record["idle"]["path"]), IDLE)
    require(now["configuration"]["revision"] == 13
            and all(now[k]["queued"] == now[k]["running"] == 0 for k in ("focus", "context")),
            "expected source-checked idle revision 13")
