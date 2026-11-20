# SPDX-License-Identifier: Apache-2.0
"""Measure a protected Compact CUDA candidate on the existing A6000 under verified WDDM pressure."""
import argparse
import json
from pathlib import Path
import runpy
import sys
import time
import uuid

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from capacity_ballast import CAMPAIGN, ROOT, pin, run
from capacity_ballast_worker import GIB, TIERS, UUID, WINDOW, require, verify
from capacity_sweep import candidate_samples, receipt, require_proof

PROBE = ROOT / "scripts/arbiter-fp16-execution/probe_compact_cutlass.py"
GRAPH = "6142500349b0c9e72ef97e6c1f06ab5a81eb519942fe35d936a9d29020542581"
MAXIMUM_PROOF = "d9e4abed37173d97ce98aef07615be70dc1fa3d735ee666e5bff467883ac8986"
SMALL_PROOF = "b412ed022096de0528e947c42b38637e8dd54da1b4bfc9a7393a982274ad0a81"


class PressureTrial:
    def __init__(self, tier, authorize):
        self.tier = tier
        self.authorize = authorize
        self.nonce = str(uuid.uuid4())
        self.admission = None
        self.payload = None

    def grant(self, pid, deadline, image, graph, expires_ns, baseline=None):
        require(self.payload is None and graph == GRAPH and self.tier in TIERS, "invalid pressure grant")
        admission = self.authorize(pid, deadline, baseline) if baseline is not None else self.authorize(pid, deadline)
        snapshot = admission["snapshot"]
        require(admission["candidate_pid"] == pid and snapshot["nvidia_smi_mib"]["uuid"] == "GPU-" + UUID
                and snapshot["nvidia_smi_mib"]["driver_model.current"] == "WDDM"
                and snapshot["nvidia_smi_mib"]["driver_version"] == "597.06", "foreign pressure admission")
        available = admission["bytes"]
        floor = self.tier * GIB
        require(set(available) == {"nvidia_free", "wddm_free", "cuda_free_after_touch"}
                and all(type(v) is int and v >= floor for v in available.values())
                and min(available["nvidia_free"], available["wddm_free"]) <= floor + WINDOW
                and admission["observed_monotonic_ns"] <= time.monotonic_ns() < expires_ns,
                "original available-memory tier or expiry differs")
        self.admission = admission
        self.payload = {"schema": "airk.private.cuda-pressure-admission/1", "nonce": self.nonce,
                        "candidate_pid": pid, "tier_gib": self.tier, "typed_graph_sha256": graph,
                        "native_image_sha256": image, "adapter_uuid": snapshot["nvidia_smi_mib"]["uuid"],
                        "adapter_luid": snapshot["dxgi"]["luid"], "available_bytes": available,
                        "observed_monotonic_ns": admission["observed_monotonic_ns"],
                        "expires_monotonic_ns": expires_ns}
        return self.payload

    def verify_native(self, observed, grant_sha):
        grant = self.payload
        require(grant is not None and observed["grant_sha256"] == grant_sha
                and observed["native_free_bytes"] >= self.tier * GIB
                and observed["arena_limit_bytes"] == self.tier * GIB
                and observed["pre_resume_available_bytes"] == grant["available_bytes"]
                and all(observed[key] == grant[key] for key in ("adapter_uuid", "adapter_luid", "tier_gib",
                        "observed_monotonic_ns", "expires_monotonic_ns")), "native pressure grant differs")


def launch(args):
    verify(PROBE, args.probe_source_sha256)
    original = runpy.run_path(str(PROBE))
    graph = receipt(args.graph_receipt, args.graph_sha256)
    require(graph["complete"] and graph["mode"] == "graph" and graph["result"]["typed_graph_sha256"] == GRAPH,
            "settled candidate graph required")
    small = CAMPAIGN / "compact-capacity-small-r2/execution.json"
    require_proof(receipt(small, SMALL_PROOF), True)
    build, image = graph["build"], graph["image"]
    retained = receipt(args.lineage, args.lineage_sha256)
    require(retained["complete"] and retained["image"] == image and retained["build"] == build,
            "sealed Cargo lineage differs from graph proof")
    if args.mode == "pressure-fixed":
        require(args.prior_fit and args.prior_fit_sha256, "original admitted pressure fit required")
        prior = receipt(args.prior_fit, args.prior_fit_sha256)
        trial = prior["trial"]
        require(prior["complete"] is True and prior["tier_gib"] == args.tier and trial["mode"] == "pressure-fit"
                and trial["capacity_admitted"] is True and trial["observed_outcome"] == "admitted"
                and prior["admission"]["pre_resume_host"]["minimum_spare_bytes"] == 8*GIB
                and trial["candidate_wddm"]["unknown_shared_samples"] == 0
                and trial["image"] == image and trial["build"] == build
                and trial["probe_source_sha256"] == args.probe_source_sha256, "different capacity fit")
    else:
        require(not args.prior_fit and not args.prior_fit_sha256, "unexpected prior fit")
    powershell = Path("C:/Program Files/PowerShell/7/pwsh.exe")
    options = argparse.Namespace(mode=args.mode, case="8192", output=args.output.with_name(args.output.name+"-native"),
        image=Path(image["path"]), image_sha256=image["sha256"], build=Path(build["path"]), build_sha256=build["sha256"],
        snapshot=Path(build["path"]).parent/"retained-native-sources", graph_receipt=args.graph_receipt,
        graph_sha256=args.graph_sha256, lineage=args.lineage, lineage_sha256=args.lineage_sha256,
        powershell=powershell, powershell_sha256="e94166ed9f376559c1359bafb08bc49e463a494c330f67644ec1d9f1ffeb5ee5",
        maximum_receipt_sha256=MAXIMUM_PROOF)
    def candidate(authorize):
        pressure = PressureTrial(args.tier, authorize)
        destination = options.output/"execution.json"
        try:
            original["run"](options, pressure=pressure)
        except ValueError:
            if not destination.is_file():
                raise
        actual_pin = pin(destination)
        actual = receipt(destination, actual_pin["sha256"])
        owned = actual.get("owned") or {}
        require(pressure.admission is not None and owned.get("owned_leader_pid") == pressure.payload["candidate_pid"]
                and owned.get("owned_descendants_settled") is True and owned.get("owned_job_final_active_processes") == 0
                and owned.get("owned_job_final_member_pids") == [] and not owned["owned_job_forced_termination"]
                and not owned["timed_out"] and actual["mode"] == args.mode and actual["image"] == image
                and actual["build"] == build, "native pressure process or retained evidence differs")
        memory = candidate_samples(actual, pressure.admission, limit_field="observed_limits", host_limit=16*GIB, require_shared_clock=True)
        complete = actual["complete"] is True and owned["exit_code"] == 0
        admitted = complete and memory["verified"] and memory["peak_dedicated_bytes"] <= args.tier*GIB
        if admitted:
            outcome = "admitted"
        elif complete and memory.get("peak_shared_bytes") is not None and memory["peak_shared_bytes"] > 0:
            outcome = "observed_shared_usage"
        elif complete and memory.get("peak_dedicated_bytes") is not None and memory["peak_dedicated_bytes"] > args.tier*GIB:
            outcome = "observed_dedicated_usage_exceeded_tier"
        elif complete:
            outcome = "incomplete_residency_evidence"
        else:
            outcome = "native_run_incomplete"
        verify(PROBE, args.probe_source_sha256)
        return {"mode":args.mode, "candidate_pid":owned["owned_leader_pid"], "native_complete":complete,
                "capacity_admitted":admitted, "observed_outcome":outcome,
                "candidate_wddm":memory, "native_execution":actual_pin, "image":image, "build":build,
                "probe_source_sha256":args.probe_source_sha256, "failure":actual.get("error"),
                "prior_fit":pin(args.prior_fit) if args.prior_fit else None}
    require(args.concurrent_host_bytes >= 0, "negative concurrent reservation")
    run(args.output, args.tier, (48-args.tier)*GIB, trial=candidate,
        pressure_host_reserve=16*GIB+args.concurrent_host_bytes)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("pressure-fit","pressure-fixed"))
    parser.add_argument("--tier", type=int, choices=TIERS, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--concurrent-host-bytes", type=int, required=True)
    parser.add_argument("--probe-source-sha256", required=True)
    parser.add_argument("--graph-receipt", type=Path, required=True)
    parser.add_argument("--graph-sha256", required=True)
    parser.add_argument("--lineage", type=Path, required=True)
    parser.add_argument("--lineage-sha256", required=True)
    parser.add_argument("--prior-fit", type=Path)
    parser.add_argument("--prior-fit-sha256")
    launch(parser.parse_args())
