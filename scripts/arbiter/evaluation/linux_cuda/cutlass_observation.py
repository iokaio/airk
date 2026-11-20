# SPDX-License-Identifier: Apache-2.0
"""Validate a protected Compact or Large graph-only result independent of the process supervisor."""
import uuid

from cutlass_references import GRAPH


LIMITS = {"compact": (32 << 30, 24 << 30), "large": (48 << 30, 44 << 30)}


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


SCIENTIFIC = "3c123dc7db0f0bd80ad2d35b68ca8d7bdbbf3380db7dfaad3513fadbec0f48cc"
WEIGHTS = "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3"
AGGREGATE = "fab63c38d3f55a29d13ddcbf8996c31c805e34b005a25d1bbe2f3f84128704fd"
ADAPTER = "GPU-08e2b3f6-6a39-3456-77cb-a57287a702ec"


def confirm(value, pid, backing, driver, native_log, model):
    host, _ = LIMITS[model]
    require(value["pid"] == pid and value["uid"] == 1000 and value["descriptor_sha256"] == SCIENTIFIC
            and value["private_elf_bytes"] == backing and value["address_space"] == host - backing
            and value["host_limit"] == host and value["control_reacquired"] is True
            and value["native_forwards"] == 0 and value["qualification"] is False, "native custody differs")
    native = value["native"]
    if model == "compact":
        result = native
        expected_graph, expected_weights = GRAPH, WEIGHTS
        phases = ("adapter", "before", "after", "after_session_release")
        require(native["schema"] == "airk.private.compact-protected-cutlass-model/1"
                and native["product_admission"] is False and native["cal_native_forwards"] == 0
                and all(native[k] is None for k in ("cal", "timing", "pressure")), "invalid Compact graph evidence")
    else:
        result = native["result"]
        expected_graph = "c1b5776144c6d9af07d08552cae40e491d8f866e89c53bab567adeeb32ca75e4"
        expected_weights = "5b053f02795e72ff88123b28f89d3f2c89da055e9ed2228bc60809dd0a2b4270"
        phases = ("adapter", "before", "live", "released")
    require(result["model_sha256"] == expected_graph and result["weight_sha256"] == expected_weights
            and result["route_refusal"] is None and result["score"] is None and native["qualification"] is False
            and b"SdpaKernel=" not in native_log, "protected graph identity, route or zero-forward evidence differs")
    require(result["graph"]["complete"] is True and native["placement"]["complete"] is True
            and result["graph"]["rows"] and native["placement"]["nodes"], "incomplete native graph or placement")
    expected = list(uuid.UUID(ADAPTER[4:]).bytes)
    for phase in phases:
        snapshot = native[phase]
        require(snapshot["uuid"] == expected and snapshot["driver_image_sha256"] == driver
                and (snapshot["compute_major"], snapshot["compute_minor"]) == (8, 6),
                "unexpected physical adapter, driver or architecture")
    return result
