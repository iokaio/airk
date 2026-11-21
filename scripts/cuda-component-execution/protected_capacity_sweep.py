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
from capacity_ballast_worker import GIB, MIB, TIERS, UUID, WINDOW, require, verify
from capacity_sweep import candidate_samples, receipt, require_proof, segment_evidence

PROBE = ROOT / "scripts/arbiter-fp16-execution/probe_compact_cutlass.py"
GRAPH = "6142500349b0c9e72ef97e6c1f06ab5a81eb519942fe35d936a9d29020542581"
LARGE_GRAPH = "13da8267e7490e334b5a860ced90af8318a01f5c2d03759173feac45ca598b46"
LARGE_PROBE = ROOT / "scripts/arbiter-fp16-execution/probe_large_cutlass.py"
MAXIMUM_PROOF = "d9e4abed37173d97ce98aef07615be70dc1fa3d735ee666e5bff467883ac8986"
SMALL_PROOF = "b412ed022096de0528e947c42b38637e8dd54da1b4bfc9a7393a982274ad0a81"
# Approved change A6 (docs/arbiter/final-plan-fp16.md, section 3): device memory fit is judged against a
# model-free control from the same run, with a 2 GiB reserve, and absolute peaks stay in the receipt.
CAPACITY_RULE = "baseline_relative_a6/1"
CONTROL_SOURCE = "ballast_process_shared_usage_before_resume"
MARGIN = 16*MIB
RESERVE = 2*GIB


def baseline_relative(memory, control, tier):
    """Judge recorded candidate residency against the model-free control. Pure; no device or file access."""
    require(type(control) is int and control >= 0 and tier in TIERS, "model-free control or tier unavailable")
    shared, nonlocal_, dedicated = (memory.get(key) for key in
                                    ("peak_shared_bytes", "peak_fresh_nonlocal_bytes", "peak_dedicated_bytes"))
    shared_excess = shared - control if type(shared) is int else None
    # Admission claims non-local use is within the margin, so a missing or wholly stale counter refuses below.
    nonlocal_excess = nonlocal_ - control if type(nonlocal_) is int else None
    reserved = dedicated + RESERVE if type(dedicated) is int else None
    if shared_excess is not None and shared_excess > MARGIN:
        refusal = "shared_usage_exceeds_control_margin"
    elif nonlocal_excess is not None and nonlocal_excess > MARGIN:
        refusal = "nonlocal_usage_exceeds_control_margin"
    elif reserved is not None and reserved > tier*GIB:
        refusal = "dedicated_usage_exceeds_tier_reserve"
    elif (memory.get("verified") is not True or memory.get("shared_ceiling_bytes") != control + MARGIN
          or memory.get("continuous_samples") is not True or memory.get("unknown_live_samples") != 0
          or shared_excess is None or nonlocal_excess is None or reserved is None):
        # Includes any unreadable sample between the first and last positive dedicated observation.
        refusal = "incomplete_residency_evidence"
    else:
        refusal = None
    return {"name": CAPACITY_RULE, "control": CONTROL_SOURCE, "control_shared_bytes": control, "margin_bytes": MARGIN,
            "reserve_bytes": RESERVE, "tier_bytes": tier*GIB, "peak_shared_bytes": shared,
            "peak_fresh_nonlocal_bytes": nonlocal_, "peak_dedicated_bytes": dedicated,
            "shared_excess_bytes": shared_excess, "nonlocal_excess_bytes": nonlocal_excess,
            "dedicated_plus_reserve_bytes": reserved, "samples_verified": memory.get("verified") is True,
            "satisfied": refusal is None, "refusal": refusal}


def judge(actual, admission, tier, host_limit):
    """Evaluate one owned candidate. Live trials and the offline replay share this one implementation."""
    control = admission["sample"]["process"]["Shared Usage"]
    require(type(control) is int and control >= 0, "model-free control shared usage unavailable")
    observed = dict(limit_field="observed_limits", host_limit=host_limit, require_shared_clock=True)
    memory = candidate_samples(actual, admission, shared_ceiling=control + MARGIN, **observed)
    memory.update(segment_evidence(actual["owned"]["before_GO"]["wddm"]))
    # The superseded dedicated-only reading stays visible beside the absolute peaks.
    memory["dedicated_only_verified"] = (candidate_samples(actual, admission, **observed)["verified"]
                                         and not memory["nonlocal_conflict"])
    return memory, baseline_relative(memory, control, tier)


def require_prior_fit(prior, *, tier, model, image, build, probe_source_sha256):
    """Fixed20 needs a complete fit admitted under this same rule, tier, model, image, build and probe."""
    trial, admission = prior["trial"], prior["admission"]
    rule = trial.get("capacity_rule")
    require(prior["complete"] is True and prior["tier_gib"] == tier and trial["mode"] == "pressure-fit"
            and trial.get("model", "compact") == model
            and trial["capacity_admitted"] is True and trial["observed_outcome"] == "admitted"
            and admission["pre_resume_host"]["minimum_spare_bytes"] == 8*GIB
            and isinstance(rule, dict) and rule.get("satisfied") is True
            # Re-derive the recorded verdict from the recorded evidence instead of trusting its flags.
            and rule == baseline_relative(trial["candidate_wddm"], admission["sample"]["process"]["Shared Usage"], tier)
            and trial["image"] == image and trial["build"] == build
            and trial["probe_source_sha256"] == probe_source_sha256, "different capacity fit")


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
        require_prior_fit(receipt(args.prior_fit, args.prior_fit_sha256), tier=args.tier, model=args.model,
                          image=image, build=build, probe_source_sha256=args.probe_source_sha256)
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
        memory, rule = judge(actual, pressure.admission, args.tier, host_limit)
        complete = actual["complete"] is True and owned["exit_code"] == 0
        admitted = complete and rule["satisfied"]
        outcome = "admitted" if admitted else rule["refusal"] if complete else "native_run_incomplete"
        verify(probe, args.probe_source_sha256)
        return {"model":args.model, "typed_graph_sha256":expected_graph, "mode":args.mode, "candidate_pid":owned["owned_leader_pid"], "native_complete":complete,
                "capacity_admitted":admitted, "observed_outcome":outcome,
                "candidate_wddm":memory, "capacity_rule":rule, "native_execution":actual_pin, "image":image, "build":build,
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
