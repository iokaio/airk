# SPDX-License-Identifier: Apache-2.0
"""Ignored selected metadata transport fixture, NOT installer/editor qualification.

Reuses the frozen, compiled-shape handler fixture. Exercises only status,
diagnostics and capabilities, with two real indexed documents and native grants.
No credential-bearing CONTROL response is requested or captured.
Lead must explicitly queue execution against a pinned actual native pair.
"""
import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import time
import uuid

SPEC = importlib.util.spec_from_file_location("metadata_scoped_fixture", Path(__file__).with_name("check-p6f-scoped-parity.py"))
P = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(P)
# This is an isolated interpreter/module instance, never an edit to the frozen
# campaign. Its inherited MCP and native command admission share this ceiling.
P.MAX_CALLS = 119  # final native shutdown is also counted within total120
MAX_SECONDS = 120


class Fixture(P.ScopedFixture):
    def __init__(self, runtime, owner, report, started):
        super().__init__(runtime, owner, command_timeout=15, job_timeout=60)
        self.report = report
        self.deadline = started + 90  # reserve 30 seconds for owned teardown

    def exact_rpc(self, method):
        result = super().exact_rpc(method)
        P.retain_read_comparison(self.report, "metadata_control_" + str(len(self.report.get("read_comparisons", {}).get("entries", []))),
                                 {"request": method, "response": result}, mode="observe")
        return result

    def stop_owned(self):
        assert self.child is not None
        self.run("service", "shutdown", "--runtime-dir", self.control, timeout=8)
        self.child.wait(timeout=P.CLIENTS.remaining(self.deadline, 5))
        assert self.child.returncode == 0
        self.report["cleanup"].append({"kind": "initial_service", "pid": self.child.pid, "returncode": self.child.returncode, "forced": False})
        self.child = None


class Mcp(P.Mcp):
    def __init__(self, fixture, selection):
        fixture.calls += 1  # actual initialize exchange, not hidden overhead
        assert fixture.calls <= P.MAX_CALLS
        super().__init__(fixture, selection)


def dispose(child, kind, report, fixture=None):
    """Only handles created by this fixture; no PID discovery or broad kill."""
    forced = False
    error = None
    try:
        if child.poll() is None:
            if kind == "service":
                fixture.calls += 1
                P.CLIENTS.Fixture.run(fixture, "service", "shutdown", "--runtime-dir", fixture.control, check=False, timeout=5)
            elif child.stdin is not None:
                child.stdin.close()
            child.wait(timeout=3)
    except BaseException as failure:
        error = type(failure).__name__
        if child.poll() is None:
            forced = True
            try:
                child.kill()
                child.wait(timeout=2)
            except BaseException as kill_failure:
                error = type(kill_failure).__name__
    report["cleanup"].append({"kind": kind, "pid": child.pid, "returncode": child.poll(), "forced": forced, "error_class": error})


def refusal(envelope, selection):
    assert envelope.get("isError") is True, "Scoped refusal became a successful metadata/unknown fallback"
    value = P.body(envelope)
    assert isinstance(value.get("error"), dict) and value["error"].get("code")
    assert value.get("health") != "unknown" and value.get("status") != "unknown"
    assert not any(value.get(key) for key in ("hits", "evidence", "paths", "files")), "Refused metadata carried evidence/counts"
    requested = value.get("requested_scope")
    if requested is not None:
        assert requested["confirmed"] is False
        actual = requested["selection"]
        # Frontends may refresh catalog clock, never requested authority.
        for key in ("schema", "context_key", "namespace", "grant_id", "expected_grant_revision", "index_id", "project_ids"):
            assert actual[key] == selection[key]
    else:
        P.require_scope(value, selection)
    return value


def run(args):
    started = time.monotonic()
    runtime = P.CLIENTS.runtime_root(args.runtime_dir)
    manifest = runtime / "airk-install.json"
    assert P.digest(manifest) == args.expected_runtime_sha256
    inventory = json.loads(manifest.read_text(encoding="utf-8"))
    assert 1 <= len(inventory["files"]) <= 4096
    for name, expected in inventory["files"].items():
        path = runtime / name
        assert path.resolve(strict=True).is_relative_to(runtime) and not path.is_symlink()
        assert P.digest(path) == expected
    aggregate_sources = [P.REPO / path for path in (
        "crates/airk-kernel/src/current/runtime.rs", "crates/airk-kernel/src/scoped_reads.rs", "crates/airk-mcp/src/lib.rs")]
    inputs = [Path(__file__), Path(P.__file__), Path(P.CLIENTS.__file__), Path(P.CATALOG_PROJECTION.__file__), Path(P.PRIVATE_PROBE.__file__), *aggregate_sources]
    hashes = {str(path): P.digest(path) for path in inputs}
    report = {"schema": 1, "fixture_kind": P.KIND, "purpose": "selected_status_diagnostics_capabilities_native_transport_only",
              "runtime_manifest_sha256": args.expected_runtime_sha256, "source_hashes": hashes,
              "normalization_excluded_paths": P.BASE_POINTERS, "max_client_calls": 120, "max_seconds_including_cleanup": MAX_SECONDS,
              "cases": [], "native_inputs": [], "cleanup": [], "passed": False,
              "partial_count_qualified": False, "full_count_qualified": False,
              "partial_count_limitation": {"code": "unsupported_aggregate",
                  "reason": "Published whole-index counts/readiness require frozen full-source authority; a path subset receives a typed refusal, never invented subset counts or a corpus scan.",
                  "owning_source": "crates/airk-kernel/src/scoped_reads.rs::SessionScope::require_index_metadata",
                  "owning_test": "current_metadata_handlers_accept_full_source_and_refuse_subset_or_new_alias",
                  "source_hashes": {str(path): hashes[str(path)] for path in aggregate_sources}},
              "installer_qualified": False, "installed_editor_qualified": False, "cross_platform_qualified": False}
    with tempfile.TemporaryDirectory(prefix="airk-metadata-parity-", dir=args.fixture_parent) as temporary:
        owner = Path(temporary).resolve(strict=True)
        assert owner.parent == args.fixture_parent.resolve(strict=True)
        root, extra, data = (owner / name for name in ("source", "external", "data"))
        extra.mkdir()
        (owner / "decoy-cwd").mkdir()
        originals = {}
        for subset in ("public", "private"):
            path = root / subset / "memo.py"
            path.parent.mkdir(parents=True)
            path.write_text("def " + subset + "_value():\n    return 1\n", encoding="utf-8")
            originals[path] = P.digest(path)
        fixture = Fixture(runtime, owner, report, started)
        clients = []
        try:
            fixture.start()
            fixture.stop_owned()
            report["seed"] = P.seed_handler_catalog(fixture)
            fixture.start()  # actual strict native compiled-shape open
            context = hashlib.sha256(b"owned-metadata-parity-context/1").hexdigest()
            host = fixture.host(context)
            assert host["namespace"] == report["seed"]["namespace"]
            caller, setup = fixture.setup(root, extra, data, context)
            fixture.job("shared", "index")
            # Drain already accepted derived work, never enqueue/retry/cancel it.
            P.settle_accepted_source_work(fixture, setup["index_id"], setup["request"]["registration"]["binding"], report)
            fixture.exact_rpc({"pause": {"project": fixture.projects["shared"]["binding"], "paused": True}})
            index = setup["index_id"]
            sources = list(zip(setup["request"]["sources"], setup["source_ids"]))
            primary = next(identifier for source, identifier in sources if source["owner"])
            projects = {name: "prj_" + uuid.uuid4().hex for name in ("partial", "full")}
            for name, project in projects.items():
                fixture.apply({"operation": "create_project", "id": project, "name": "Metadata " + name}, caller, context)
                native = fixture.detail({"kind": "project", "id": project})
                scope = ([{"source_root_id": primary, "kind": "subtree", "path": "public"}] if name == "partial"
                         else [{"source_root_id": identifier, "kind": "subtree", "path": ""} for _source, identifier in sources])
                fixture.apply({"operation": "set_membership", "membership": {"project_id": project, "index_id": index, "revision": 1, "scope": scope},
                               "expected_scope_revision": native["scope_revision"], "expected_membership_revision": None}, caller, context)
            grant = P.grant_selection(fixture, context, sorted(projects.values()), index)
            selected = {name: {**copy.deepcopy(grant), "project_ids": [project]} for name, project in projects.items()}
            counts = {}
            for name, selection in selected.items():
                mcp = Mcp(fixture, selection)
                clients.append(mcp)
                for tool in ("airk_status", "airk_diagnostics", "airk_capabilities"):
                    arguments = {"response_mode": "full", "reply_detail": "rich"}
                    report["native_inputs"].append({"scope": selection, "native_tool": tool, "public_mcp_tool": tool.removeprefix("airk_"), "arguments": arguments})
                    native = fixture.call_selected(selection, tool, arguments)
                    label = name + "_" + tool
                    P.retain_read_comparison(report, label + "_baseline", native, mode="observe")
                    value = P.body(native)
                    if native.get("isError") is True:
                        refusal(native, selection)
                        assert value["error"]["code"] == "scoped_read_unavailable", "Metadata failed outside its canonical scoped-refusal contract"
                        P.require_scope(value, selection)
                        if tool in ("airk_status", "airk_diagnostics") and name == "partial":
                            assert not any(key in value for key in ("files", "chunks", "source_roots", "semantic_readiness")), "Partial aggregate refusal exposed whole-index fields"
                        assert tool != "airk_status" or name == "partial", "Full-source status must positively count this index"
                    else:
                        P.require_scope(value, selection)
                        assert name != "partial" or tool not in ("airk_status", "airk_diagnostics"), "Partial aggregate unexpectedly exposed whole-index metadata"
                    P.retain_read_comparison(report, label + "_public_mcp", mcp.call(tool, arguments), native, mode="envelope")
                    P.retain_read_comparison(report, label + "_interactive_shell", fixture.shell_selected(selection, tool, arguments), value)
                    if tool == "airk_status":
                        if name == "full":
                            assert type(value.get("files")) is int and value["files"] == 2
                            counts[name] = value["files"]
                        # Status always emits JSON; there is no --json option.
                        P.retain_read_comparison(report, label + "_convenience", fixture.alias_selected(selection, ["status", "--reply-detail", "rich"]), native, mode="envelope")
                    report["cases"].append({"id": label, "canonical_refusal": native.get("isError") is True, "passed": True})
            assert counts == {"full": 2}, "Full-source metadata did not positively count the indexed documents"
            report["positive_full_source_counts"] = counts
            report["full_count_qualified"] = True
            fixture.apply({"operation": "revoke_grant", "id": grant["grant_id"], "expected_revision": grant["expected_grant_revision"]}, caller, context)
            existing = clients[0].call("airk_status", {"response_mode": "full", "reply_detail": "rich"})
            P.retain_read_comparison(report, "revoked_existing_mcp_status", existing, mode="observe")
            existing_body = refusal(existing, selected["partial"])
            fresh = fixture.call_selected(selected["partial"], "airk_status", {"response_mode": "full", "reply_detail": "rich"})
            P.retain_read_comparison(report, "revoked_new_cli_status", fresh, mode="observe")
            fresh_body = refusal(fresh, selected["partial"])
            assert "query_submitted" in existing_body["error"] and "query_submitted" in fresh_body["error"]
            assert fresh_body["error"]["query_submitted"] is False, "New rejected brokerage must preserve known non-submission"
            if existing_body["error"]["query_submitted"] is None:
                assert existing_body["error"]["fresh_brokerage_required"] is True and existing_body["error"]["query_replay_safe"] is False
            else:
                assert existing_body["error"]["query_submitted"] is False
            report["revocation_delivery_phases"] = {"existing_mcp": existing_body, "new_cli": fresh_body,
                                                     "whole_errors_compared_for_equality": False, "both_refused": True}
            assert all(P.digest(path) == expected for path, expected in originals.items())
            assert next(extra.iterdir(), None) is None
            assert all(P.digest(P.REPO / path) == expected for path, expected in report["seed"]["schema_inputs"].items())
            assert all(P.digest(Path(path)) == expected for path, expected in hashes.items())
            assert P.digest(manifest) == args.expected_runtime_sha256
            assert all(P.digest(runtime / name) == expected for name, expected in inventory["files"].items())
            report["passed"] = True
        except BaseException as error:
            report["failure"] = {"class": type(error).__name__, "message": str(error)[:2048]}
        finally:
            for client in clients:
                dispose(client.child, "mcp", report)
            if fixture.child is not None:
                dispose(fixture.child, "service", report, fixture)
            fixture.log.close()
            report.update(native_calls=fixture.calls, elapsed_seconds=round(time.monotonic() - started, 3),
                          source_bytes_preserved=all(path.is_file() and P.digest(path) == expected for path, expected in originals.items()))
            report["all_owned_children_exited"] = all(item["returncode"] == 0 and not item["forced"] and item["error_class"] is None
                                                       for item in report["cleanup"] if item["kind"] != "initial_service")
            report["passed"] = (report["passed"] and report["all_owned_children_exited"] and report["source_bytes_preserved"]
                                and report["native_calls"] <= 120 and report["elapsed_seconds"] <= MAX_SECONDS)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--expected-runtime-sha256", required=True)
    parser.add_argument("--fixture-parent", type=Path, default=P.REPO / "target")
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    try:
        report = run(args)
    except BaseException as error:
        report = {"schema": 1, "fixture_kind": P.KIND, "passed": False,
                  "failure": {"class": type(error).__name__, "message": str(error)[:2048]}, "installer_qualified": False}
    args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"passed": report["passed"], "report": str(args.report), "native_calls": report.get("native_calls")}))
    if not report["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
