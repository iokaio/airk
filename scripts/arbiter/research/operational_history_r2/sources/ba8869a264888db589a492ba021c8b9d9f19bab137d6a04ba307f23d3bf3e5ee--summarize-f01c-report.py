# SPDX-License-Identifier: Apache-2.0
"""Local aggregate inspection only; does not change the original report."""
import json
import math
import sys
from pathlib import Path


def quantiles(values):
    values = sorted(v for v in values if isinstance(v, (int, float)))
    if not values:
        return {"observed": 0}
    return {"observed": len(values), "min_ms": values[0],
            "p50_ms": values[math.ceil(len(values) * .5) - 1],
            "p95_ms": values[math.ceil(len(values) * .95) - 1],
            "max_ms": values[-1]}


def probes(summary):
    keys = ("kind", "observed", "not_observed", "usable_nonempty", "success_empty",
            "typed_no_content", "native_error", "transport_protocol_failure",
            "other_unusable", "first_usable_ms")
    return {"outcome": summary.get("observation_outcome"),
            "all_eventually_usable": summary.get("all_probes_eventually_usable"),
            "probes": [{key: row.get(key) for key in keys}
                       for row in summary.get("probes", [])]}


def sessions(rows):
    kinds = sorted({read.get("kind", "other") for row in rows for read in row.get("reads", [])})
    return {"initialize": quantiles([row.get("initialize_ms") for row in rows]),
            "warm_status": quantiles([(row.get("warm_status") or {}).get("elapsed_ms") for row in rows]),
            "probes": {kind: quantiles([read.get("elapsed_ms") for row in rows
                                       for read in row.get("reads", []) if read.get("kind") == kind])
                       for kind in kinds}}


report = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
result = {key: report.get(key) for key in ("completed", "code", "observation_outcome",
          "cleanup_failed", "source_identity_preserved", "samples", "environment", "policy")}
result["corpora"] = []
for row in report.get("corpora", []):
    initial = row.get("initial_index", {})
    reopen = row.get("reopen", {})
    item = {"alias": row.get("alias"), "source": row.get("source"),
            "preparation": row.get("before_index"),
            "initial": {"elapsed_ms": initial.get("elapsed_ms"),
                        "job_state": initial.get("job_state"),
                        "last_publication": initial.get("published"),
                        "status_observations": initial.get("status_observations"),
                        "first_sample": initial.get("during_index"),
                        "probes": probes(initial.get("probe_summary", {}))},
            "session_summary": row.get("session_summary"),
            "session_timings": sessions(row.get("sessions", [])),
            "session_probes": probes(row.get("session_probe_summary", {})),
            "reopen_status": reopen.get("status_summary"),
            "reopen_first": reopen.get("sessions", [])[:1],
            "reopen_timings": sessions(reopen.get("sessions", [])),
            "reopen_availability": {key: reopen.get("availability", {}).get(key)
                                    for key in ("independent_sessions", "stop_reason")},
            "reopen_probes": probes(reopen.get("probe_summary", {})),
            "cancellation": row.get("cancellation_after_healthy_reopen"),
            "reconcile": row.get("unchanged_reconcile")}
    result["corpora"].append(item)
print(json.dumps(result, indent=2))
