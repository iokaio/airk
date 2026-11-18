# SPDX-License-Identifier: Apache-2.0
"""Prepare and run the existing held graph diagnostic for a fresh /6 producer."""

import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import sys
import time
import types


sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
SELF = Path(__file__).resolve()
CONTRACT = HERE / "FRESH-GRAPH.md"
MEMORY_POLICY = HERE / "compact_graph_host.py"
SOURCES = {
    "component": ROOT / "scripts/cuda-component-execution.py",
    "lease": HERE / "graph_provenance.py",
    "preserver": ROOT / "scripts/preserve-cuda-test-images.py",
    "native": ROOT / "scripts/cuda-native-execution.py",
}
FRESH_BINDING = "airk.private.cuda-fresh-finalgraph-image-binding/1"
FRESH_BUILD = "airk.private.cuda-fresh-static-build-proof/1"
BINDING_PINS = (
    "environment", "source_roster", "archive", "producer", "manifest", "runtime_producer",
    "cargo_json", "preserver_source", "aggregate_receipt", "aggregate_recipe",
    "configure_receipt", "configure_recipe", "build_receipt",
)
MAX_SOURCE = 2 << 20
MAX_CHAIN_JSON = 32 << 20


def require(condition, message):
    if not condition:
        raise ValueError(message)


def check(deadline):
    require(time.monotonic() < deadline, "original fresh graph deadline expired")


def owned_path(path):
    path = Path(path)
    require(path.is_absolute() and path.resolve(strict=True) == path and path.is_file(),
            "absolute unaliased fresh graph input required")
    for ancestor in (path, *path.parents):
        require(not ancestor.is_symlink() and not getattr(ancestor.lstat(), "st_file_attributes", 0) & 0x400,
                "reparse fresh graph input refused")
    return path


def captured(path, sha256, maximum, deadline):
    check(deadline)
    path = owned_path(path)
    with path.open("rb") as stream:
        raw = stream.read(maximum + 1)
    require(0 < len(raw) <= maximum and hashlib.sha256(raw).hexdigest() == sha256,
            "reviewed fresh graph input bytes differ: " + str(path))
    check(deadline)
    return {"path": str(path), "bytes": len(raw), "sha256": sha256}, raw


def decode(raw):
    def pairs(rows):
        result = {}
        for key, value in rows:
            require(key not in result, "duplicate fresh graph input key")
            result[key] = value
        return result

    def invalid(_):
        raise ValueError("nonfinite fresh graph input")

    return json.loads(raw, object_pairs_hook=pairs, parse_constant=invalid)


def installed_owner(path, raw, name):
    owner = types.ModuleType(name)
    owner.__file__ = str(path)
    exec(compile(raw, str(path), "exec", dont_inherit=True), owner.__dict__)
    return owner


def read_checked(row, maximum, deadline):
    actual, raw = captured(row["path"], row["sha256"], maximum, deadline)
    require(actual == row, "fresh graph FilePin differs")
    return decode(raw)


def derive_boundary(legacy, plan, binding, base, read, *, known=None):
    """Derive current held inputs from the actual preserved fresh relationships."""
    require(binding["schema"] == FRESH_BINDING and binding["proof"]["schema"] == FRESH_BUILD,
            "only fresh producer /6 graph evidence is accepted")
    rows = {}

    def add(row):
        legacy.file_pin(row)
        key = legacy.key(row["path"])
        require(key not in rows or rows[key] == row, "conflicting fresh graph boundary pin")
        rows[key] = row

    def observed_pin(path):
        path = str(Path(path))
        if known is None:
            pin = base.pin(path)
        else:
            require(legacy.key(path) in known, "prepared fresh boundary omitted a derived file")
            pin = known[legacy.key(path)]["pin"]
        require(pin["path"] == path, "current fresh graph path differs")
        return pin

    def sealed(path, sha):
        require(isinstance(sha, str), "current fresh graph input has no source seal")
        pin = observed_pin(path)
        require(pin["sha256"] == sha, "current fresh graph seal differs")
        add(pin)

    for field in (*legacy.PLAN_PINS, "fresh_bootstrap", "fresh_lease_owner"):
        add(plan[field])
    if "compact_host_policy" in plan:
        add(plan["compact_host_policy"])
    for field in BINDING_PINS:
        add(binding[field])
    for row in (*binding["source_pins"], *plan["runtime_files"]):
        add(row)
    proof = binding["proof"]
    for field in ("producer_source", "collector_source", "source_lock", "cpu_lock"):
        add(proof[field])
    for row in (*proof["ninja"], *proof["provider_dlls"]):
        add(row)
    for patch in proof["patch_inputs"]:
        add(patch["original"])
        add(patch["snapshot"])

    compiled = read(binding["producer"], MAX_CHAIN_JSON)
    configured = read(binding["configure_receipt"], MAX_CHAIN_JSON)
    configure_root = Path(binding["configure_receipt"]["path"]).parent
    configure_stage_pin = observed_pin(configure_root / "logs" / "configure.receipt.json")
    add(configure_stage_pin)
    require(read(configure_stage_pin, MAX_CHAIN_JSON) == configured["configure"],
            "fresh retained configure stage receipt differs")
    aggregate = read(binding["aggregate_receipt"], MAX_CHAIN_JSON)
    sessions = [row for row in compiled["libraries"]
                if Path(row["path"]).name == "onnxruntime_session.lib"]
    require(len(sessions) == 1, "exact fresh session archive required")
    add(sessions[0])
    for row in (*compiled["stage_receipts"], aggregate["stage_receipt"], aggregate["response"]):
        add(row)
    for stage in (configured["configure"], *compiled["stages"], aggregate["command"]):
        for row in stage["files"]:
            add(row)

    for relative, sha in plan["required_native_sources"].items():
        require(relative.startswith("crates/") and "\\" not in relative
                and all(part not in ("", ".", "..") for part in relative.split("/")),
                "invalid fresh native source path")
        sealed(ROOT / relative, sha)
    require(Path(plan["ownership_helper_root"]) == base.MAINTAINED_HELPER_ROOT,
            "fresh graph requires the maintained ownership helpers")
    for leaf, sha in base.HELPERS.items():
        sealed(base.MAINTAINED_HELPER_ROOT / leaf, sha)

    metadata = read(plan["metadata_receipt"], legacy.MAX_JSON)
    add(metadata["plan"])
    old = read(metadata["plan"], legacy.MAX_JSON)
    add(old["native_build_receipt"])
    for row in metadata["outputs"]:
        add(row)
    native = read(plan["native_input"], legacy.MAX_JSON)
    for path in native["files"].values():
        sealed(path, plan["input_seals"].get(str(Path(path))))
    for field in ("source_manifest", "numeric_fixture"):
        row = native[field]
        sealed(row["path"], row["sha256"])
    fixture = read(rows[legacy.key(native["numeric_fixture"]["path"])], legacy.MAX_JSON)
    row = fixture["reference_producer"]
    sealed(row["path"], row["sha256"])

    for row in rows.values():
        declared = plan["input_seals"].get(row["path"])
        require(declared is None or declared == row["sha256"], "fresh graph seal conflicts with derived boundary")
    require(0 < len(rows) <= legacy.MAX_FILES - 2, "fresh graph boundary cap includes preparation and plan")
    return [rows[key] for key in sorted(rows)]


class FreshGraph:
    BINDING_PINS = BINDING_PINS

    def __init__(self, component, legacy, output_base, approved, self_pin, policy_pin, clock):
        self.component = component
        self.legacy = legacy
        self.output_base = output_base
        self.approved = approved
        self.self_pin = self_pin
        self.policy_pin = policy_pin
        self.clock = clock

    def identify(self, plan):
        require(plan["wrapper"] == self.approved["component"]
                and plan["custody_owner"] == self.approved["native"]
                and plan["graph_provenance_owner"] == self.self_pin
                and plan["graph_provenance_contract"] == self.policy_pin
                and plan["graph_preserver"] == self.approved["preserver"]
                and plan["fresh_lease_owner"] == self.approved["lease"]
                and plan["fresh_bootstrap"] == self.approved["roster"],
                "reviewed fresh graph launcher or shared source differs")
        require(plan.get("compact_host_policy") == self.approved.get("memory_policy")
                and plan["limits"] == self.component.LIMITS,
                "reviewed Compact graph host policy differs")
        require(self.component.execution_mode(plan) == "graph", "explicit graph diagnostic mode required")

    def held_preserver(self, lease, plan):
        owner = self.legacy.held_preserver(lease, plan)

        def fresh_bytes(row, maximum, deadline):
            require(deadline == lease.deadline and lease.pin(row["path"]) == row
                    and 0 < row["bytes"] <= maximum, "fresh held preserver byte pin differs")
            return lease.bytes(row["path"], maximum)

        def fresh_reviewed_pin(path, sha, deadline):
            require(deadline == lease.deadline, "fresh held preserver deadline differs")
            row = lease.pin(path)
            require(row["sha256"] == sha, "fresh held preserver source pin differs")
            return row

        owner.fresh_bytes = fresh_bytes
        owner.fresh_reviewed_pin = fresh_reviewed_pin
        return owner

    def replay(self, plan, binding, lease, preserver):
        self.identify(plan)
        require(binding["schema"] == FRESH_BINDING
                and binding["preserver_source"] == plan["graph_preserver"],
                "fresh preserved graph binding differs")
        require(plan["output_guard"] == plan["graph_output_guard"]
                and lease.pin(plan["output_guard"]["path"]) == plan["graph_output_guard"],
                "fresh graph output policy must use its exact maintained owner")
        return preserver.verify_fresh_current(
            binding, plan["descriptor"], plan["native_image"], plan["native_build_receipt"],
            deadline=lease.deadline,
        )

    def output_policy_owner(self, lease, plan):
        return self.legacy.output_policy_owner(lease, plan)

    def finish_lease(self, lease, receipt):
        self.legacy.finish_lease(lease, receipt)

    def prepare(self, plan, base, path, component_seams, _started, _started_utc, _deadline):
        started, started_utc, deadline_utc, deadline = self.clock
        check(deadline)
        require(not Path(path).exists(), "fresh graph preparation output already exists")
        plan["graph_provenance_owner"] = self.self_pin
        plan["graph_provenance_contract"] = self.policy_pin
        plan["fresh_bootstrap"] = self.approved["roster"]
        plan["fresh_lease_owner"] = self.approved["lease"]
        if memory_policy := self.approved.get("memory_policy"):
            plan["compact_host_policy"] = memory_policy
            plan["input_seals"][memory_policy["path"]] = memory_policy["sha256"]
        self.identify(plan)
        for row in (self.self_pin, self.policy_pin, self.approved["roster"], self.approved["lease"]):
            plan["input_seals"][row["path"]] = row["sha256"]
        component_seams.guard(plan, path, deadline=deadline)
        read = lambda row, maximum: read_checked(row, maximum, deadline)
        packet = read(plan["image_packet"], self.legacy.MAX_JSON)
        binding = packet["static_graph_binding"]
        boundary = derive_boundary(self.legacy, plan, binding, base, read)
        with self.legacy.Lease(plan, boundary, deadline) as lease:
            require(lease.json(plan["image_packet"]["path"]) == packet,
                    "held fresh image packet differs from bootstrap")
            # This unchanged preflight dispatches the complete /6 verifier once.
            component_seams.preflight(plan, base, before=True, deadline=deadline)
            self.replay(plan, binding, lease, self.held_preserver(lease, plan))
            lease.refresh()
            records = lease.records()
        finished_utc = dt.datetime.now(dt.timezone.utc)
        finished = time.monotonic()
        require(finished < deadline and finished_utc < deadline_utc, "original fresh graph preparation expired")
        value = dict(
            schema=self.legacy.SCHEMA, status="PREPARED_UNQUALIFIED", plan=plan,
            preparation_source=plan["graph_provenance_owner"], parent_source=plan["wrapper"],
            preserver_source=plan["graph_preserver"], metadata_receipt=plan["metadata_receipt"],
            image_packet=plan["image_packet"], binding=binding, boundary=records,
            loaded_sources=list(lease.loaded.values()), identity=lease.identity,
            started_monotonic=started, finished_monotonic=finished, elapsed_seconds=finished-started,
            started_utc=started_utc.isoformat(), finished_utc=finished_utc.isoformat(),
            original_deadline=deadline_utc.isoformat(), preparation_deadline_monotonic=deadline,
            qualification=False, product_admission=False, whole_build_reverified=True,
            runtime_admitted=False, opened_handles=len(lease.files), closed_handles=lease.closed_count,
            handle_close_errors=lease.close_errors, receipt_retention_excluded_from_elapsed=True,
        )
        component_seams.write_new(plan, Path(path), value)
        return base.pin(path)

    def open_prepared(self, plan, plan_pin, prepared, preparation_pin, deadline):
        self.identify(plan)
        require(plan == self.legacy.execution_plan(prepared, preparation_pin),
                "fresh graph plan differs from its reviewed preparation")
        expected = {}
        for row in prepared["boundary"]:
            key = self.legacy.key(row["pin"]["path"])
            require(key not in expected, "duplicate fresh prepared boundary object")
            expected[key] = row
        binding = prepared["binding"]
        pins = [row["pin"] for row in expected.values()] + [preparation_pin]
        if plan_pin is not None:
            pins.append(plan_pin)
        lease = self.legacy.Lease(plan, pins, deadline, expected=expected)
        try:
            require(lease.json(preparation_pin["path"]) == prepared, "held fresh preparation differs")
            if plan_pin is not None:
                require(lease.json(plan_pin["path"]) == plan, "held fresh plan differs")
            packet = lease.json(plan["image_packet"]["path"])
            require(packet["static_graph_binding"] == binding, "held fresh image packet binding differs")
            lease.base = self.legacy.execution_base(lease)

            def read(row, maximum):
                require(lease.pin(row["path"]) == row, "derived fresh document pin differs")
                return lease.json(row["path"], maximum)

            derived = derive_boundary(self.legacy, prepared["plan"], binding, lease.base, read, known=expected)
            require(derived == [expected[key]["pin"] for key in sorted(expected)],
                    "prepared fresh graph boundary has omissions or extras")
            self.replay(plan, binding, lease, self.held_preserver(lease, plan))
            lease.plan_pin, lease.binding, lease.prepared = plan_pin, binding, prepared
            return lease
        except BaseException as error:
            try:
                lease.close()
            except BaseException as close_error:
                error.add_note("Fresh graph boundary close failed: " + str(close_error))
            raise

    def prepare_plan(self, args):
        _, _, _, deadline = self.clock
        pin, raw = captured(args.graph_preparation, args.graph_preparation_sha256,
                            self.legacy.MAX_JSON, deadline)
        prepared = self.legacy.strict_json(raw)
        plan = self.legacy.execution_plan(prepared, pin)
        with self.open_prepared(plan, None, prepared, pin, deadline) as lease:
            self.component.preflight(plan, lease.base, before=True, deadline=deadline, graph_lease=lease)
            lease.refresh()
        check(deadline)
        self.component.write_new(plan, args.plan, plan)
        return self.output_base.pin(args.plan)

    def run(self, args):
        started, started_utc, cutoff_utc, deadline = self.clock
        plan_pin, raw = captured(args.plan, args.plan_sha256, self.legacy.MAX_JSON, deadline)
        plan = self.legacy.strict_json(raw)
        preparation_pin = plan["graph_preparation"]
        prepared = read_checked(preparation_pin, self.legacy.MAX_JSON, deadline)
        with self.open_prepared(plan, plan_pin, prepared, preparation_pin, deadline) as lease:
            context = (started, started_utc, cutoff_utc, deadline, lease, self, self.output_base)
            return self.component.run_loaded(args, graph=True, graph_context=context)


def parse(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--prepare-graph-provenance", action="store_true")
    modes.add_argument("--prepare-graph-diagnostic", action="store_true")
    modes.add_argument("--execute-reviewed-graph-diagnostic", action="store_true")
    parser.add_argument("--approved-script-sha256", required=True)
    parser.add_argument("--approved-sources", type=Path, required=True)
    parser.add_argument("--approved-sources-sha256", required=True)
    parser.add_argument("--fresh-contract-sha256", required=True)
    parser.add_argument("--compact-host-policy-sha256", help="Use the reviewed 32 GiB Compact graph host policy")
    parser.add_argument("--metadata-receipt", type=Path)
    parser.add_argument("--metadata-receipt-sha256")
    parser.add_argument("--image-packet", type=Path)
    parser.add_argument("--image-packet-sha256")
    parser.add_argument("--powershell", type=Path)
    parser.add_argument("--output-root", type=Path)
    parser.add_argument("--graph-preparation", type=Path)
    parser.add_argument("--graph-preparation-sha256")
    parser.add_argument("--plan", type=Path)
    parser.add_argument("--plan-sha256")
    args = parser.parse_args(argv)
    preparation = ("metadata_receipt", "metadata_receipt_sha256", "image_packet",
                   "image_packet_sha256", "powershell", "output_root", "graph_preparation")
    required = (preparation if args.prepare_graph_provenance else
                ("graph_preparation", "graph_preparation_sha256", "plan") if args.prepare_graph_diagnostic else
                ("plan", "plan_sha256"))
    optional = set(preparation) | {"graph_preparation_sha256", "plan", "plan_sha256"}
    if {name for name in optional if getattr(args, name) is not None} != set(required):
        parser.error("supply only the exact inputs for the selected fresh graph operation")
    return args


def main(argv=None):
    started = time.monotonic()
    started_utc = dt.datetime.now(dt.timezone.utc)
    args = parse(argv)
    require(os.name == "nt", "fresh held graph requires the verified human Windows path")
    campaign = dt.datetime(2026, 11, 19, 5, 29, 42, tzinfo=dt.timezone.utc)
    cutoff_utc = min(started_utc + dt.timedelta(seconds=600 if args.prepare_graph_provenance else 120),
                     campaign)
    deadline = started + (cutoff_utc-started_utc).total_seconds()
    self_pin, _ = captured(SELF, args.approved_script_sha256, MAX_SOURCE, deadline)
    policy_pin, _ = captured(CONTRACT, args.fresh_contract_sha256, MAX_SOURCE, deadline)
    roster_pin, raw = captured(args.approved_sources, args.approved_sources_sha256, MAX_SOURCE, deadline)
    roster = decode(raw)
    require(isinstance(roster, dict) and set(roster) == set(SOURCES),
            "exact fresh graph bootstrap source roster required")
    approved, bodies = {"roster": roster_pin}, {}
    for role, path in SOURCES.items():
        approved[role], bodies[role] = captured(path, roster[role], MAX_SOURCE, deadline)
    if args.compact_host_policy_sha256:
        approved["memory_policy"], raw = captured(
            MEMORY_POLICY, args.compact_host_policy_sha256, MAX_SOURCE, deadline)
        memory_policy = installed_owner(MEMORY_POLICY, raw, "fresh_graph_compact_host_policy")
        component = memory_policy.load_graph_runner(SOURCES["component"], bodies["component"])
    else:
        component = installed_owner(SOURCES["component"], bodies["component"], "fresh_graph_component")
    legacy = installed_owner(SOURCES["lease"], bodies["lease"], "fresh_graph_lease")
    base = installed_owner(SOURCES["native"], bodies["native"], "fresh_graph_native")
    require(base.CAMPAIGN == campaign and base.SID == legacy.SID == component.SID,
            "fresh graph owners disagree on campaign or execution identity")
    original_base = component.base
    original_bind = component.bind_graph_hash_deadline

    def approved_base():
        check(deadline)
        raw = captured(SOURCES["native"], approved["native"]["sha256"], MAX_SOURCE, deadline)[1]
        require(raw == bodies["native"], "reviewed fresh native helper differs")
        return installed_owner(SOURCES["native"], raw, "fresh_graph_native_use")

    component.base = approved_base
    component.bind_graph_hash_deadline = lambda owner, bound: original_bind(owner, min(bound, deadline))
    original_bind(base, deadline)
    adapter = FreshGraph(component, legacy, base, approved, self_pin, policy_pin,
                         (started, started_utc, cutoff_utc, deadline))
    original_graph_owner = component.graph_owner
    if args.prepare_graph_provenance:
        component.graph_owner = lambda owner, plan: adapter
    try:
        if args.prepare_graph_provenance:
            result = component.prepare(args, graph=True, provenance=True)
        elif args.prepare_graph_diagnostic:
            result = adapter.prepare_plan(args)
        else:
            result = adapter.run(args)
            result = {key: result[key] for key in
                      ("complete", "elapsed_seconds", "graph_metadata_observed", "selected_logs_complete")}
    finally:
        component.base = original_base
        component.graph_owner = original_graph_owner
        component.bind_graph_hash_deadline = original_bind
    print(json.dumps(result))


if __name__ == "__main__":
    main()
