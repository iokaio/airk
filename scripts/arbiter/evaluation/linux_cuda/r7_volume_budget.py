# SPDX-License-Identifier: Apache-2.0
"""Apply the approved C: allowance while retaining the signed R7 guest limits."""
from datetime import datetime, timezone
import json
from pathlib import Path
import stat

GUEST_LOG_SHA = "507b1cd3178c6d108ffd4ec3274c9735cba04f8860f23ef7b0fcb8f3e497d5bc"
ORIGINAL_ALERT_SHA = "9cfbf2ed1877be97f859d5a5a696a3aa9aa90dcb06989f477c642e75fa452cda"
HOST_START = 2_669_960_904_704
VHD_START = 60_649_635_840
HOST_CAP = 120_000_000_000
GUEST_CAP = 24 << 30
FLOOR = 100 << 30
SWAP_RESERVE = 16 << 30
CUTOFF = datetime(2026, 11, 20, 17, 10, tzinfo=timezone.utc).timestamp()


def original_guest_baseline():
    from configure_ort import CAMPAIGN, sha

    path = CAMPAIGN / "linux-cuda-r4-provider-r3-space-watch.jsonl"
    if sha(path) != GUEST_LOG_SHA:
        raise ValueError("original guest volume baseline changed")
    with path.open("rb") as stream:
        baseline = json.loads(stream.readline())
    if baseline["vhd_length"] != VHD_START:
        raise ValueError("original guest and host VHD baselines differ")
    return baseline


def check_monitor(row, now):
    age = now - datetime.fromisoformat(row["utc"]).timestamp()
    expected = {
        "backing_start": HOST_START, "vhd_start": VHD_START, "backing_budget": HOST_CAP,
        "previous_backing_budget": GUEST_CAP, "free_floor": FLOOR,
        "original_alert_log_sha256": ORIGINAL_ALERT_SHA,
    }
    if (now >= CUTOFF or not 0 <= age <= 45 or any(row[key] != value for key, value in expected.items())
            or row["within_budget"] is not True or row["has_headroom"] is not True
            or row["backing_writes_held"] is not False):
        raise ValueError("revised host volume observation is stale or outside the campaign allowance")


def read_monitor(path: Path, uid, now):
    from configure_ort import CAMPAIGN

    info = path.lstat()
    if (path != CAMPAIGN / "host-volume-watch-r4.jsonl" or path.resolve(strict=True) != path or info.st_uid != uid
            or not stat.S_ISREG(info.st_mode)):
        raise ValueError("expected the active human-owned campaign volume monitor")
    complete, _, _ = path.read_bytes().rpartition(b"\n")
    _, _, last = complete.rpartition(b"\n")
    row = json.loads(last)
    check_monitor(row, now)
    return row


def revised_capacity(baseline, sample, monitor, initial, reserve, *, admitting):
    guest_growth = max(0, baseline["guest_free"] - sample["guest_free"])
    vhd_growth = max(0, sample["vhd_length"] - VHD_START)
    host_free = min(sample["backing_free"], monitor["backing_free"])
    host_growth = max(0, HOST_START - host_free)
    additional = reserve if admitting else 0
    run_growth = max(initial["guest_free"] - sample["guest_free"], sample["vhd_length"] - initial["vhd_length"])
    return (guest_growth + additional <= GUEST_CAP and vhd_growth + additional <= GUEST_CAP
            and run_growth <= reserve and host_growth + SWAP_RESERVE + additional <= HOST_CAP
            and sample["guest_free"] >= FLOOR + GUEST_CAP - guest_growth
            and host_free >= FLOOR + HOST_CAP - host_growth)
