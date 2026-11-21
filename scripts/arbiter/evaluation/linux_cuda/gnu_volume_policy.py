# SPDX-License-Identifier: Apache-2.0
"""Admit GNU Cargo, packaging and boot writes under the recorded C:, D:, guest and VHD limits."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import stat
import time

# Recorded authorization: docs/agent-current-work.md, extension restart checkpoint of 20 November 2026.
C_BASELINE = 2_669_960_904_704
C_GROWTH_ALLOWANCE = 120_000_000_000
D_BASELINE = 1_233_246_265_344
D_GROWTH_BUDGET = 195_421_011_968
FREE_FLOOR = 107_374_182_400
SWAP_RESERVE = 17_179_869_184
GUEST_FREE_BASELINE = 967_378_755_584
VHD_BASELINE = 60_649_635_840
# The two retained separate caps. Whether either may rise inside the C: allowance is an open user
# decision. Each has this one definition, so a recorded decision changes one constant.
GUEST_GROWTH_CAP = 25_769_803_776
VHD_GROWTH_CAP = 25_769_803_776
# Facts the host watcher writes into every row of a log started with -RevisedBackingLogSha256.
ORIGINAL_C_BUDGET = 25_769_803_776
ORIGINAL_ALERT_LOG_SHA256 = "9cfbf2ed1877be97f859d5a5a696a3aa9aa90dcb06989f477c642e75fa452cda"
GUEST_BASELINE_LOG = "linux-cuda-r4-provider-r3-space-watch.jsonl"
GUEST_BASELINE_LOG_SHA256 = "507b1cd3178c6d108ffd4ec3274c9735cba04f8860f23ef7b0fcb8f3e497d5bc"
MAX_ROW_AGE_SECONDS = 45
MAX_WINDOW_SECONDS = 24 * 3600
LOG_NAME = re.compile(r"host-volume-watch-r[1-9][0-9]*\.jsonl")
ROW_FACTS = {
    "windows_start": D_BASELINE, "windows_budget": D_GROWTH_BUDGET,
    "backing_start": C_BASELINE, "backing_budget": C_GROWTH_ALLOWANCE,
    "vhd_start": VHD_BASELINE, "free_floor": FREE_FLOOR,
    "previous_backing_budget": ORIGINAL_C_BUDGET,
    "original_alert_log_sha256": ORIGINAL_ALERT_LOG_SHA256,
}
_ACTIVE = None


def instant(text):
    """Read an ISO-8601 time that states its own UTC offset."""
    value = datetime.fromisoformat(text) if isinstance(text, str) else None
    if value is None or value.tzinfo is None:
        raise ValueError("a time with an explicit UTC offset is required")
    return value.timestamp()


def row_digest(line):
    return hashlib.sha256(line.rstrip(b"\r\n")).hexdigest()


def rows(data):
    """Return the first and the last complete row. A row still being written is ignored."""
    complete, newline, _ = data.rpartition(b"\n")
    if not newline or not complete.strip():
        raise ValueError("volume monitor has no complete row")
    return complete.partition(b"\n")[0], complete.rpartition(b"\n")[2]


def check_row(row):
    if not isinstance(row, dict):
        raise ValueError("volume monitor row is not an object")
    if any(key not in row or type(row[key]) is not type(value) or row[key] != value for key, value in ROW_FACTS.items()):
        raise ValueError("volume monitor was not started with the recorded baselines and allowances")
    if any(type(row.get(key)) is not int for key in ("windows_free", "backing_free", "vhd_bytes")):
        raise ValueError("volume monitor row lacks its three observations")
    if row.get("backing_writes_held") is not False:
        raise ValueError("volume monitor holds C: writes")
    if row.get("within_budget") is not True or row.get("has_headroom") is not True:
        raise ValueError("volume monitor reports a budget or free-space breach")


def check_monitor(first, last, now, not_after):
    check_row(first)
    check_row(last)
    started, seen = instant(first.get("utc")), instant(last.get("utc"))
    if not started <= seen or not 0 <= now - seen <= MAX_ROW_AGE_SECONDS:
        raise ValueError("volume monitor observation is stale")
    if now >= not_after:
        raise ValueError("volume policy window is closed")
    if not 0 < not_after - started <= MAX_WINDOW_SECONDS:
        raise ValueError("volume policy window must end within one day of the monitor's first row")


def read_monitor(path, campaign, uid, first_row_sha256, now, not_after):
    info = path.lstat()
    if (path.parent != campaign or LOG_NAME.fullmatch(path.name) is None or path.resolve(strict=True) != path
            or info.st_uid != uid or not stat.S_ISREG(info.st_mode)):
        raise ValueError("expected the active human-owned campaign volume monitor")
    first, last = rows(path.read_bytes())
    if row_digest(first) != first_row_sha256:
        raise ValueError("volume monitor first row differs from the run's binding")
    first, last = json.loads(first), json.loads(last)
    check_monitor(first, last, now, not_after)
    return first, last


def guest_baseline(data):
    """Tie the two guest baselines to the retained first observation of the campaign."""
    if hashlib.sha256(data).hexdigest() != GUEST_BASELINE_LOG_SHA256:
        raise ValueError("original guest volume baseline changed")
    first = json.loads(data.partition(b"\n")[0])
    if first["guest_free"] != GUEST_FREE_BASELINE or first["vhd_length"] != VHD_BASELINE:
        raise ValueError("original guest and VHD baselines differ from the recorded authorization")
    return first


def host_use(c_free, d_free, vhd_length):
    vhd_growth = max(0, vhd_length - VHD_BASELINE)
    c_growth = max(0, C_BASELINE - c_free, vhd_growth)
    d_growth = max(0, D_BASELINE - d_free)
    return {"vhd_growth": vhd_growth, "vhd_remaining": VHD_GROWTH_CAP - vhd_growth,
            "c_growth": c_growth, "c_remaining": C_GROWTH_ALLOWANCE - SWAP_RESERVE - c_growth,
            "d_growth": d_growth, "d_remaining": D_GROWTH_BUDGET - d_growth}


def host_refusals(c_free, d_free, vhd_length, additional):
    use = host_use(c_free, d_free, vhd_length)
    found = []
    if use["vhd_growth"] + additional > VHD_GROWTH_CAP:
        found.append("VHD growth cap")
    if use["c_growth"] + SWAP_RESERVE + additional > C_GROWTH_ALLOWANCE:
        found.append("C: growth allowance with the swap reserve")
    if use["d_growth"] > D_GROWTH_BUDGET:
        found.append("D: growth budget")
    if c_free < FREE_FLOOR + max(0, C_GROWTH_ALLOWANCE - use["c_growth"]):
        found.append("C: free-space floor")
    if d_free < FREE_FLOOR + max(0, D_GROWTH_BUDGET - use["d_growth"]):
        found.append("D: free-space floor")
    return found


def guest_use(guest_free):
    growth = max(0, GUEST_FREE_BASELINE - guest_free)
    return {"guest_growth": growth, "guest_remaining": GUEST_GROWTH_CAP - growth}


def guest_refusals(guest_free, additional):
    growth = guest_use(guest_free)["guest_growth"]
    found = []
    if growth + additional > GUEST_GROWTH_CAP:
        found.append("guest free-space growth cap")
    if guest_free < FREE_FLOOR + max(0, GUEST_GROWTH_CAP - growth):
        found.append("guest free-space floor")
    return found


def observed(sample, monitor):
    """Take the less favourable of the guest's and the watcher's reading of each host fact."""
    return (min(sample["backing_free"], monitor["backing_free"]),
            min(sample.get("windows_free", monitor["windows_free"]), monitor["windows_free"]),
            max(sample["vhd_length"], monitor["vhd_bytes"]))


def headroom(sample, monitor):
    return {**guest_use(sample["guest_free"]), **host_use(*observed(sample, monitor))}


def refusals(sample, monitor, initial, reserve, *, admitting):
    """Name every limit this sample breaks. `reserve` None is an observer without a reservation."""
    additional = reserve if admitting and reserve is not None else 0
    found = guest_refusals(sample["guest_free"], additional) + host_refusals(*observed(sample, monitor), additional)
    run_growth = max(initial["guest_free"] - sample["guest_free"], sample["vhd_length"] - initial["vhd_length"])
    if reserve is not None and run_growth > reserve:
        found.append("storage reservation of this run")
    return found


class Binding:
    """One run's watcher log, the digest of that log's first row, and the time the run must end by."""

    def __init__(self, log, first_row_sha256, not_after):
        if re.fullmatch(r"[0-9a-f]{64}", first_row_sha256) is None:
            raise ValueError("first-row SHA-256 must be 64 lowercase hexadecimal digits")
        self.log, self.first_row_sha256, self.not_after = Path(log), first_row_sha256, not_after

    def bounded(self, timeout, *, clock=time.time):
        remaining = min(timeout, self.not_after - clock())
        if remaining <= 0:
            raise ValueError("volume policy window is closed")
        return remaining

    def guard(self, reserve, uid, *, clock=time.time, campaign=None, d_free=None):
        """Return the check `shipping_build.monitored` applies to every sample."""
        if campaign is None:
            from configure_ort import CAMPAIGN as campaign
        guest_baseline((campaign / GUEST_BASELINE_LOG).read_bytes())
        if d_free is None:
            import shutil
            d_free = lambda: shutil.disk_usage(campaign).free

        def fits(sample, initial, admitting):
            _, monitor = read_monitor(self.log, campaign, uid, self.first_row_sha256, clock(), self.not_after)
            sample["host_monitor"] = monitor
            sample["windows_free"] = d_free()
            sample["volume_refusals"] = refusals(sample, monitor, initial, reserve, admitting=admitting)
            if admitting and sample["volume_refusals"]:
                # Nothing has started yet, so name the limits instead of returning a bare refusal.
                raise ValueError("volume policy refuses admission: " + "; ".join(sample["volume_refusals"]))
            return not sample["volume_refusals"]
        return fits

    def admit(self, sample, reserve, uid, **environment):
        self.guard(reserve, uid, **environment)(sample, sample, True)
        return sample

    def receipt(self):
        return {"volume_log": str(self.log), "first_row_sha256": self.first_row_sha256,
                "not_after": datetime.fromtimestamp(self.not_after, timezone.utc).isoformat(),
                "c_growth_allowance": C_GROWTH_ALLOWANCE, "d_growth_budget": D_GROWTH_BUDGET,
                "free_floor": FREE_FLOOR, "guest_growth_cap": GUEST_GROWTH_CAP, "vhd_growth_cap": VHD_GROWTH_CAP,
                "swap_reserve": SWAP_RESERVE}


def arguments(parser):
    parser.add_argument("--volume-log", type=Path, help="active host-volume-watch-rN.jsonl of this run")
    parser.add_argument("--volume-log-first-row-sha256", help="SHA-256 of that log's first row")
    parser.add_argument("--not-after", help="ISO-8601 end of this run with its UTC offset")


def selected(options):
    """Return the binding the arguments describe, or None when none of the three was given."""
    values = (options.volume_log, options.volume_log_first_row_sha256, options.not_after)
    if all(value is None for value in values):
        return None
    if any(value is None for value in values):
        raise ValueError("the volume policy needs its log, first-row SHA-256 and end time together")
    return Binding(options.volume_log, options.volume_log_first_row_sha256, instant(options.not_after))


def activate(binding):
    """Make one binding the default of `shipping_build.monitored` for this process."""
    global _ACTIVE
    if _ACTIVE is not None or binding is None:
        raise ValueError("one volume policy binding per process")
    _ACTIVE = binding


def active():
    return _ACTIVE


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description="Print the first-row binding of a host volume watcher log.")
    cli.add_argument("--volume-log", type=Path, required=True)
    chosen = cli.parse_args().volume_log
    head = rows(chosen.read_bytes())[0]
    check_row(json.loads(head))
    print(json.dumps({"volume_log": str(chosen), "first_row_sha256": row_digest(head), "first_row": json.loads(head)},
                     sort_keys=True))
