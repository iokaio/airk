# SPDX-License-Identifier: Apache-2.0
"""Project both original Context inputs from one authenticated completed R8 capture."""

import argparse
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import r7_chronological_inputs as original

from r8_capture import (
    AUDIT_SHA as AUDIT,
    LIFETIME_SHAS as LIFETIMES,
    PLAN_SHA as PLAN,
    ROOT as R8_ROOT,
    SERVICE_SHA as SERVICE,
    TERMINAL_SHAS as TERMINAL,
    collect as verify_capture,
    report_verified,
)

SCHEMA = "airk.private.cal100-r8-chronological-projection/1"
read, reference, digest, require = original.read, original.reference, original.digest, original.require


def require_completed(plan_sha, lifetime_shas, terminal):
    require(plan_sha == PLAN and tuple(lifetime_shas) == LIFETIMES and tuple(terminal) == TERMINAL,
            "exact completed R8 plan, ordered lifetimes and terminal custody required")


def reports_for(plan_ref, plan, lifetimes, observed):
    ids = plan["phase_ids"]
    require([len(phase) for phase in ids] == [8] * 12 + [4]
            and [row["id"] for row in observed] == [key for phase in ids for key in phase],
            "incomplete or reordered completed R8 observations")
    reports = []
    offset = 0
    for group, lifetime_pin in enumerate(lifetimes):
        directory = R8_ROOT / f"lifetime-{group:02}"
        lifetime = read(lifetime_pin, directory / "lifetime-closure.json")
        phases = original.TWO_GROUPS[group]
        require(lifetime["plan"] == plan_ref and lifetime["phases"] == list(phases)
                and len(lifetime["phase_closures"]) == len(phases), "R8 phase custody differs")
        admissions = directory / "fixture/context-cal-admissions"
        for phase, phase_pin in zip(phases, lifetime["phase_closures"], strict=True):
            phase_owner = read(phase_pin, directory / f"phase-{phase:02}.owned.json")
            selected = observed[offset:offset + len(ids[phase])]
            rows = []
            for ordinal, evidence in enumerate(selected, start=offset):
                captures = evidence["capture_refs"]
                require(len(captures) == len(evidence["captured"]) <= 1,
                        "R8 original admission file pin missing")
                for pin, record in zip(captures, evidence["captured"], strict=True):
                    number = record["ordinal_in_private_run"]
                    require(type(number) is int and 1 <= number <= 100
                            and Path(pin["path"]) == admissions / f"record-{number:04}.json",
                            "R8 original admission file belongs to a different native lifetime")
                rows.append({"ordinal": ordinal, "question_id": evidence["id"], "captures": captures})
            require(phase_owner["phase"] == phase
                    and Path(phase_owner["transcript"]["path"]) == directory / f"phase-{phase:02}.jsonl",
                    "R8 phase native transcript pin differs")
            reports.append({
                "phase": phase, "closed_questions": len(rows), "rows": rows,
                "probes": sum(len(row["probes"]) for row in selected),
                "source_transcript": phase_owner["transcript"], "phase_owner": phase_pin,
                "pipeline_searches": sum(row["pipeline_searches"] for row in selected),
                "certificate_count": 0, "lifetime_sha256": lifetime_pin["sha256"],
                "plan_sha256": plan_ref["sha256"], "input_audit_sha256": AUDIT,
            })
            offset += len(rows)
    require(offset == len(observed) == 100, "complete original R8 population required")
    return reports


def collect(plan_sha, lifetime_shas, terminal, *, reader=verify_capture):
    """Traverse the certificate owner's verified capture once and prepare both model inputs."""
    require_completed(plan_sha, lifetime_shas, terminal)
    plan_ref = reference(R8_ROOT / "lifetime-plan.json", plan_sha)
    return project_verified(plan_ref, *reader(plan_ref, lifetime_shas, terminal), lifetime_shas=lifetime_shas)


def project_verified(plan_ref, plan, audit, pairs, *, lifetime_shas=LIFETIMES):
    """Build the projection from the certificate owner's authenticated in-process evidence."""
    require(plan_ref["sha256"] == PLAN and tuple(lifetime_shas) == LIFETIMES,
            "different authenticated R8 plan or lifetime order")
    require(plan["input_audit"]["sha256"] == AUDIT
            and audit["inputs"]["service"]["sha256"] == SERVICE,
            "R8 capture launch audit or service differs")
    observations = []
    for evidence, binding in pairs:
        original.check_scoped_evidence(evidence, binding, audit)
        observations.append(evidence)
    lifetimes = [reference(R8_ROOT / f"lifetime-{i:02}/lifetime-closure.json", sha)
                 for i, sha in enumerate(lifetime_shas)]
    return plan_ref, plan, reports_for(plan_ref, plan, lifetimes, observations), observations


def encode(value):
    return json.dumps(value, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode()


def write_projection(out, plan_ref, plan, reports, observations, *, certificate=None):
    require(out.is_absolute() and out.resolve(strict=True) == out and out.parent == R8_ROOT.parent
            and out.is_dir() and not any(out.iterdir()), "empty guarded R8 campaign output required")
    payloads = {}
    phases = []
    for report in reports:
        name = f"phase-{report['phase']:02}-independent-inputs.json"
        raw = payloads[name] = encode(report)
        phases.append({"path": str(out / name), "sha256": digest(raw)})
    models = {}
    for model, result in original.batches(plan_ref, plan, reports, observations, phases).items():
        name = f"{model}.json"
        raw = payloads[name] = encode(result)
        require(len(raw) <= 128 << 20, "R8 native compiler projection exceeds input limit")
        models[model] = {"path": str(out / name), "bytes": len(raw), "sha256": digest(raw)}
    certificate_pin = None
    if certificate is not None:
        require(certificate["complete"] is True
                and certificate["lifetime_plan_sha256"] == plan_ref["sha256"]
                and certificate["launch_audit_sha256"] == AUDIT
                and [row["id"] for row in certificate["rows"]] == [row["id"] for row in observations],
                "independent reference certificate does not cover this exact projection")
        name = "reference-preflight.json"
        raw = payloads[name] = encode(certificate)
        certificate_pin = {"path": str(out / name), "bytes": len(raw), "sha256": digest(raw)}
    receipt = {
        "schema": SCHEMA, "plan": plan_ref, "plan_sha256": plan_ref["sha256"],
        "launch_audit": plan["input_audit"], "service_sha256": SERVICE,
        "lifetimes": list(LIFETIMES), "terminal": list(TERMINAL),
        "acquired_sha256": plan["installation_mutex"]["sha256"], "models": models,
        "original_requests": len(observations), "NN_invocations": 0, "gain_certificates": 0,
        "source_sha256": digest(Path(__file__).read_bytes()),
        "reference_preflight": certificate_pin,
    }
    pins = list(payloads)
    owner = original.original.check_outputs
    receipt_name = "projection-receipt.json"
    owner(out, [*pins, receipt_name], created=False)
    for name, raw in payloads.items():
        with (out / name).open("xb") as output:
            output.write(raw)
    owner(out, pins, created=True)
    with (out / receipt_name).open("xb") as output:
        output.write(encode(receipt))
    owner(out, [receipt_name], created=True)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan-sha256", required=True)
    parser.add_argument("--lifetime-sha256", action="append", required=True)
    parser.add_argument("--terminal", nargs=3, required=True, metavar=("OUTER_SHA", "FIXTURE_SHA", "RELEASE_SHA"))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    require(sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use checked Python -I -B -S")
    require_completed(args.plan_sha256, args.lifetime_sha256, args.terminal)
    plan_ref = reference(R8_ROOT / "lifetime-plan.json", args.plan_sha256)
    plan, audit, pairs = verify_capture(plan_ref, args.lifetime_sha256, args.terminal)
    pairs = list(pairs)
    certificate = report_verified(plan_ref, audit, pairs, args.lifetime_sha256, args.terminal)
    projection = project_verified(plan_ref, plan, audit, pairs, lifetime_shas=args.lifetime_sha256)
    print(json.dumps(write_projection(args.output, *projection, certificate=certificate), allow_nan=False))


if __name__ == "__main__":
    main()
