# SPDX-License-Identifier: Apache-2.0
"""Measure a protected CUDA model on the existing A6000 under verified WDDM pressure."""
import argparse
from contextlib import ExitStack
import hashlib
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
from capacity_sweep import candidate_samples, receipt, require_proof, segment_evidence

PROBE = ROOT / "scripts/arbiter-fp16-execution/probe_compact_cutlass.py"
GRAPH = "6142500349b0c9e72ef97e6c1f06ab5a81eb519942fe35d936a9d29020542581"
LARGE_GRAPH = "13da8267e7490e334b5a860ced90af8318a01f5c2d03759173feac45ca598b46"
LARGE_PROBE = ROOT / "scripts/arbiter-fp16-execution/probe_large_cutlass.py"
MAXIMUM_PROOF = "d9e4abed37173d97ce98aef07615be70dc1fa3d735ee666e5bff467883ac8986"
SMALL_PROOF = "b412ed022096de0528e947c42b38637e8dd54da1b4bfc9a7393a982274ad0a81"


class PressureTrial:
    def __init__(self, tier, authorize, expected_graph=GRAPH):
        require(expected_graph in (GRAPH, LARGE_GRAPH), "unregistered pressure graph")
        self.expected_graph = expected_graph
        self.tier = tier
        self.authorize = authorize
        self.nonce = str(uuid.uuid4())
        self.admission = None
        self.payload = None

    def grant(self, pid, deadline, image, graph, expires_ns, baseline=None):
        require(self.payload is None and graph == self.expected_graph and self.tier in TIERS, "invalid pressure grant")
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


def verify_host_grant(grant, native, host_limit, campaign_ceiling=32*GIB):
    require((host_limit, campaign_ceiling) in {(12*GIB,32*GIB), (16*GIB,32*GIB), (22*GIB,48*GIB)},
            "unregistered pressure host grant")
    owned = native["owned"]
    limits = owned["suspended_assignment"]["observed_limits"]
    require(grant["schema"] == "airk.private.cuda-pressure-host-admission/1"
            and grant["device_grant_sha256"] == native["pressure_grant"]["sha256"]
            and grant["candidate_pid"] == owned["owned_leader_pid"] and grant["mode"] == native["mode"]
            and grant["executable_sha256"] == native["image"]["sha256"]
            and grant["campaign_host_ceiling_bytes"] == native["original_limits"]["campaign_host_ceiling_bytes"] == campaign_ceiling
            and grant["limits"] == limits and limits["process_memory_limit_bytes"] == limits["job_memory_limit_bytes"]
            == native["original_limits"]["host_bytes"] == host_limit
            and 0 < owned["peak_host_committed_bytes"] <= host_limit,
            "actual host grant, enforced Job bounds or measured peak differs")


def launch(args):
    large = args.model == "large"
    probe = LARGE_PROBE if large else PROBE
    expected_graph = LARGE_GRAPH if large else GRAPH
    verify(probe, args.probe_source_sha256)
    original = runpy.run_path(str(probe))
    graph = receipt(args.graph_receipt, args.graph_sha256)
    observed = graph["result"]["result"] if large else graph["result"]
    require(graph["complete"] and graph["mode"] == "graph" and observed["typed_graph_sha256"] == expected_graph
            and (not large or graph.get("direct") is True),
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
                and trial.get("model", "compact") == args.model
                and trial["capacity_admitted"] is True and trial["observed_outcome"] == "admitted"
                and prior["admission"]["pre_resume_host"]["minimum_spare_bytes"] == 8*GIB
                and trial["candidate_wddm"]["unknown_shared_samples"] == 0
                and trial["candidate_wddm"].get("nonlocal_conflict") is False
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
    if large:
        require(args.operand_receipt and args.operand_sha256, "Large same-image operand required")
        options.direct, options.width, options.separate_native_window = True, 8192, args.mode == "pressure-fixed"
        options.operand_receipt, options.operand_sha256 = args.operand_receipt, args.operand_sha256
    else:
        require(not args.operand_receipt and not args.operand_sha256, "unexpected Compact operand override")
    host_limit = (22 if large else 12 if args.mode == "pressure-fit" else 16)*GIB
    def candidate(authorize):
        pressure = PressureTrial(args.tier, authorize, expected_graph)
        destination = options.output/"execution.json"
        try:
            if large:
                with ExitStack() as retained_inputs:
                    original["run"](options, retained_inputs, pressure=pressure)
            else:
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
        host_pin = actual["pressure_host_grant"]
        require(Path(host_pin["path"]) == options.output/"temp"/"pressure-host-admission.json",
                "unexpected pressure host admission path")
        host_path = Path(host_pin["path"])
        require(host_path.is_file() and not host_path.is_symlink() and not host_path.parent.is_junction()
                and host_path.stat().st_nlink == 1 and host_path.stat().st_size <= 8192,
                "foreign pressure host grant")
        host_raw = host_path.read_bytes()
        require(hashlib.sha256(host_raw).hexdigest() == host_pin["sha256"], "changed pressure host grant")
        verify_host_grant(json.loads(host_raw),actual,host_limit,(48 if large else 32)*GIB)
        memory = candidate_samples(actual, pressure.admission, limit_field="observed_limits", host_limit=host_limit, require_shared_clock=True)
        memory.update(segment_evidence(owned["before_GO"]["wddm"]))
        memory["verified"] = memory["verified"] and not memory["nonlocal_conflict"]
        complete = actual["complete"] is True and owned["exit_code"] == 0
        admitted = complete and memory["verified"] and memory["peak_dedicated_bytes"] <= args.tier*GIB
        if admitted:
            outcome = "admitted"
        elif complete and memory.get("peak_shared_bytes") is not None and memory["peak_shared_bytes"] > 0:
            outcome = "observed_shared_usage"
        elif complete and memory["nonlocal_conflict"]:
            outcome = "observed_nonlocal_usage"
        elif complete and memory.get("peak_dedicated_bytes") is not None and memory["peak_dedicated_bytes"] > args.tier*GIB:
            outcome = "observed_dedicated_usage_exceeded_tier"
        elif complete:
            outcome = "incomplete_residency_evidence"
        else:
            outcome = "native_run_incomplete"
        verify(probe, args.probe_source_sha256)
        return {"model":args.model, "typed_graph_sha256":expected_graph, "mode":args.mode, "candidate_pid":owned["owned_leader_pid"], "native_complete":complete,
                "capacity_admitted":admitted, "observed_outcome":outcome,
                "candidate_wddm":memory, "native_execution":actual_pin, "image":image, "build":build,
                "probe_source_sha256":args.probe_source_sha256, "failure":actual.get("error"),
                "host_limit_bytes":host_limit, "pressure_host_grant":host_pin,
                "prior_fit":pin(args.prior_fit) if args.prior_fit else None}
    require(args.concurrent_host_bytes >= 0, "negative concurrent reservation")
    run(args.output, args.tier, (48-args.tier)*GIB, trial=candidate,
        pressure_host_reserve=host_limit+args.concurrent_host_bytes, candidate_limit=host_limit,
        trial_sources=(Path(__file__),probe,HERE/"capacity_sweep.py",HERE/"capacity_ballast.py",
                       HERE/"capacity_ballast_worker.py",HERE/"ballast_host_commit.py",
                       HERE/"wddm_capacity_snapshot.py",HERE/"wddm_process_samples.py"))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("pressure-fit","pressure-fixed"))
    parser.add_argument("--model", choices=("compact","large"), default="compact")
    parser.add_argument("--operand-receipt", type=Path)
    parser.add_argument("--operand-sha256")
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
