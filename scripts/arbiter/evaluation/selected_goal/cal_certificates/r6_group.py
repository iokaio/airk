# SPDX-License-Identifier: Apache-2.0
"""R6 fixed original groups and independently checked 1,024-dispatch ceilings."""
from grouped_closure import TWO_GROUPS, check_clocks, check_mutex, read, require


def check_group(directory, lifetime, group, plan, previous):
    phases = list(TWO_GROUPS[group])
    require(lifetime["schema"] == "airk.private.cal100-group-lifetime/1" and lifetime["group"] == group
            and lifetime["phases"] == phases and len(lifetime["phase_closures"]) == len(phases)
            and type(lifetime["end_ms"]) is int and previous <= lifetime["start_ms"] < lifetime["end_ms"],
            "wrong native group, overlapping lifetime or incomplete original phase closure")
    owners = [read(pin,directory/f"phase-{phase:02}.owned.json")
              for phase,pin in zip(phases,lifetime["phase_closures"],strict=True)]
    check_clocks(lifetime,owners,plan["deadline_ms"])
    dispatch = lifetime["dispatch"]
    require(type(dispatch["focus_dispatches"]) is int and 0 <= dispatch["focus_dispatches"] <= 1024
            and type(dispatch["records"]) is int and 0 <= dispatch["records"] <= 8192,
            "native group exceeded its own recorder limits")
    return lifetime["phase_closures"]
