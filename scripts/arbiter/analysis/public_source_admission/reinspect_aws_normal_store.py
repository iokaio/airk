# SPDX-License-Identifier: Apache-2.0
"""Inspect the already registered full AWS store in a new owned native service lifetime."""
import argparse
import json
import os
from pathlib import Path
import runpy
import sys
import time

HERE = Path(__file__).resolve().parent
normal = runpy.run_path(str(HERE / "aws_normal_service.py"))
n, require = normal["n"], normal["require"]
ORIGIN = n["CAMPAIGN"] / "aws-normal-full-r1"
SID = "S-1-5-21-3678370870-3858642417-826968105-1002"
WALL = 4 * 3600
CUTOFF_UNIX = 1795194600  # 2026-11-20T17:10:00Z campaign deadline
QUOTA = 8 << 30
VECTOR_JOB = "08f313523140e4e3e3dd1d9a94c4e9ffdf73a3bc317381a16ade443cededc4e1"
INDEX_JOB = "48085c06314597b62b59e04833c6f284375653ebcd20ade58a2ae03ec774ab56"
ORIGINAL_REVISION = "airk-r1:94fa9968172443534a235186861f7e13:18:4e07b7b5e59c2e6c62c68fd02d1e783066b848f16c8e64e964c8557cc50a10e1"
ORIGINAL_DB = "beebb5a864fdab349cfeb1b21ecb8533504a3d25201cd5fb9769bbb0159c990c"
HOST = 2 << 30
PINS = {
    "owned.json": "defd655ef7e42c7b7cb8b8845a2934dc802280b3cc4820f3d27859fb9c19e1c7",
    "limits.json": "fc9c0f42e913abe1426e8a4a89489f5b181c3129e0b5d3bf2299c6b3b1d71546",
    "commands/register.stdout": "154b27e8f8c4162608b50f9bcd4406e9f9adf2dea29594679403eab8cd474a83",
    "commands/start.stdout": "6346f434c496f0aa27dff6cc76801a019acebff0611e6be9513aa3b33524aea9",
    "commands/stop.stdout": "f77018ea040664c8c876d6fb0995068144e2ff4ba100368e856ffbd185d64712",
    "commands/set-store-quota.stdout": "f195a65fe8a862c724c02d5a1fa8749577907195eaa9f77962d878ce97678a4b",
}


def original(owner, audit, audit_sha):
    require(audit == n["CAMPAIGN"] / "aws-normal-full-r1-custody.private.json", "unexpected custody receipt")
    n["checked"](owner.plain(audit, regular=True), audit_sha)
    row = json.loads(audit.read_bytes())
    require(Path(row["root"]) == ORIGIN and row["sid"] == row["owner"] == SID
            and row["complete"] is True and row["observations"] >= 206037, "original audit incomplete")
    for name, expected in PINS.items():
        n["checked"](owner.plain(ORIGIN / name, regular=True), expected)
    prior = json.loads((ORIGIN / "owned.json").read_bytes())
    require(prior["exit_code"] == 1 and prior["owned_descendants_settled"] and not prior["timed_out"]
            and prior["child_assigned_before_resume"] and not prior["owned_job_forced_termination"]
            and prior["owned_job_final_member_pids"] == [] and prior["owned_job_final_active_processes"] == 0,
            "original service still active or forced")
    with (ORIGIN / "commands/register.stdout").open("rb") as stream:
        _, accepted = map(json.loads, stream)
    return (accepted["receipt"], json.loads((ORIGIN / "commands/start.stdout").read_bytes())["observation"],
            json.loads((ORIGIN / "commands/set-store-quota.stdout").read_bytes())["job"])


def diagnose(w, owner, args, output, member):
    reg, first, previous = original(owner, args.audit, args.audit_sha)
    n["checked"](owner.plain(ORIGIN / "control/service-1.4.sqlite3", regular=True), ORIGINAL_DB)
    menu = normal["menu"]
    n["checked"](n["CAMPAIGN"] / "aws-expanded-source-verification-r1.private.json", normal["EXPANDED"])
    tree, cases = normal["stage"]["source_closure"](w, owner, menu)
    program, pins = normal["images"]()
    control = owner.plain(ORIGIN / "control")
    runs = output / "commands"
    runs.mkdir()
    project, index = reg["request"]["registration"]["binding"], reg["index_id"]
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith(("AIRK_", "RUST_LOG", "PYTHON"))}
    deadline = float(os.environ["AIRK_AWS_INSPECT_DEADLINE"])

    def cli(label, *parts, closing=False):
        path, _elapsed = w["run_logged"]([program["airkctl"], "--output", "json", "--service-runtime-dir",
                                        control, *parts], runs, label, env, deadline - (0 if closing else 30))
        return w["read_json"](path)

    def start(label, prior):
        health = cli(label, "service", "start", "--runtime-dir", control)
        try:
            live = health["observation"]
            require(health["status"] == "health" and live["user"] == SID
                    and live["executable_sha256"] == pins["airk"]["sha256"]
                    and live["instance"] != prior["instance"]
                    and live["catalog_clock"]["namespace"] == prior["catalog_clock"]["namespace"],
                    "unexpected catalog incarnation")
            member(live["process_id"])
            return live
        except Exception:
            cli(label + "-unaccepted-stop", "service", "stop", "--runtime-dir", control, closing=True)
            raise

    def wait_job(label, job, terminal="succeeded"):
        poll = 0
        while time.monotonic() < deadline - 30:
            row = cli(f"{label}-{poll}", "jobs", "get", job["id"])["job"]
            require((row["id"], row["kind"]) == (job["id"], job["kind"]), "changed native job")
            if row["state"] == terminal:
                require(row["outcome"]["state"] == terminal, "native outcome differs")
                return row
            require(row["state"] in ("pending", "running"), "unexpected terminal job outcome")
            poll += 1
            time.sleep(min(5, max(0, deadline - time.monotonic() - 30)))
        raise TimeoutError(label)

    def tool(label, name, payload):
        return w["tool_body"](n["publication_read"](cli, w["read_json"], runs, label,
            ["tools", "call", name, "--index", index, "--args", json.dumps(payload)], deadline - 30))

    def configure(label, draft):
        path = output / (label + "-draft.json")
        w["write_json"](path, draft)
        row = cli(label, "--service-project", project, "configure", "--file", path)["job"]
        return wait_job(label + "-result", row)

    def settings(label, view):
        row = cli(label, "--service-project", project, "settings", view)
        require(row["project"] == project and isinstance(row["value"], dict), "wrong native settings project")
        return row["value"]

    live = start("reopen", first)
    try:
        # Quotable original index and vectors job identities come from the
        # authenticated stopped control journal. Never enqueue an Index.
        vector_job = cli("pending-vectors", "jobs", "get", VECTOR_JOB)["job"]
        require(vector_job["kind"] == "vectors" and vector_job["project"] == project, "wrong vector job")
        if vector_job["state"] in ("pending", "running"):
            cli("cancel-vectors", "jobs", "cancel", VECTOR_JOB)
            wait_job("vector-cancellation", vector_job, "canceled")
        else:
            require(vector_job["state"] in ("succeeded", "canceled", "failed"), "unknown vector job state")
        disabled = configure("disable-vectors", {"operation": "vectors", "enabled": False})
        last = cli("previous-configure", "jobs", "get", previous["id"])["job"]
        require(last["state"] == "failed" and last["outcome"]["code"] == "configuration_failed_reinspect",
                "original failure differs")
        indexed = cli("original-index", "jobs", "get", INDEX_JOB)["job"]
        require(indexed["state"] == "succeeded" and indexed["outcome"]["revision"] == ORIGINAL_REVISION,
                "original index differs")
        inspect = wait_job("inspect-result", cli("inspect", "--service-project", project, "inspect")["job"])
        settings("current-roots", "roots")
        settings("current-vector-status", "vector_status")
        prior_policy = settings("prior-quota-policy", "quota_policy")
        # Vector policy can request an ordinary source refresh. Observe an
        # existing source job to completion before reducing its reservation.
        status = settings("quota-after-disable", "quota_status")
        waited = set()
        while any(row["used_bytes"] + row["reserved_bytes"] > QUOTA
                  for row in [status, *status["collections"].values()]):
            require(status["used_bytes"] <= QUOTA, "existing native data exceeds the quota")
            state = cli(f"outstanding-work-{len(waited)}", "--service-project", project, "progress")
            jobs = [state.get("pending_job"), *state.get("queued_jobs", [])]
            observation = state.get("observation") or {}
            jobs.append(observation.get("job"))
            active = next((job for job in jobs if isinstance(job, dict) and job["id"] not in waited
                           and job["state"] in ("pending", "running") and job["kind"] in ("index", "reconcile")), None)
            require(active is not None, "outstanding quota has no native source job to await")
            waited.add(active["id"])
            wait_job(f"automatic-source-{len(waited)}", active)
            status = settings(f"quota-after-source-{len(waited)}", "quota_status")
        quota = output / "quota.json"
        policy = {"root_bytes": QUOTA, "default_collection_bytes": QUOTA, "collections": {}}
        w["write_json"](quota, policy)
        applied = configure("apply-quota", {"operation": "quota_policy", "path": str(quota)})
        require(settings("current-policy", "quota_policy") == policy, "8 GiB native policy not applied")
        before = tool("after-config", "airk_status", {"response_mode": "full", "include_status": True})
        require(before["semantic_readiness"]["state"] == "disabled" and before["discovery_readiness"]["state"] == "ready",
                "native source-only discovery not ready")
    finally:
        cli("stop", "service", "stop", "--runtime-dir", control, closing=True)
    reopened = start("verified-restart", live)
    try:
        after = tool("reopened-status", "airk_status", {"response_mode": "full", "include_status": True})
        scope = after["scope_observation"]
        token = after["publication"]["token"]
        require(token == before["publication"]["token"] and after["files"] == before["files"]
                and after["semantic_readiness"] == before["semantic_readiness"]
                and after["discovery_readiness"] == before["discovery_readiness"]
                and {k: v for k, v in scope.items() if k != "fingerprint"} ==
                    {k: v for k, v in before["scope_observation"].items() if k != "fingerprint"},
                "reopened source publication differs")

        def stable(label, name, payload):
            row = tool(label, name, payload)
            require(row["publication"]["token"] == token and row["scope_observation"] == scope,
                    "source publication or grant changed during census")
            return row

        observed = menu["observe"](menu["COHORT"], cases, tree, stable)
    finally:
        cli("verified-stop", "service", "stop", "--runtime-dir", control, closing=True)
    normal["images"]()
    w["write_json"](output / "reinspection.private.json", {"origin": str(ORIGIN), "source_receipts": PINS,
        "custody_sha256": args.audit_sha, "original_control_sha256": ORIGINAL_DB, "original_revision": ORIGINAL_REVISION,
        "health": live, "reopened": reopened, "project": project, "index": index, "original_vector_job": vector_job,
        "disabled": disabled, "previous_failure": last, "new_inspection": inspect, "original_quota": prior_policy,
        "quota_after_disable": status, "applied_quota": applied, "status": before, "reopened_status": after,
        "observations": observed, "scored_requests": 0, "ranker_observed": False})


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output", required=True, type=Path)
    p.add_argument("--audit", required=True, type=Path)
    p.add_argument("--audit-sha", required=True)
    p.add_argument("--worker", action="store_true")
    args = p.parse_args()
    n["checked"](n["WRAPPER"], n["WRAPPER_SHA"])
    w = runpy.run_path(str(n["WRAPPER"]))
    owner = w["owner_module"]()
    output = owner.plain(args.output)
    require(output.parent == n["CAMPAIGN"] and output.name.startswith("aws-normal-reinspect-"), "wrong output root")
    if args.worker:
        require(os.environ["AIRK_AWS_INSPECT_OWNER"] == str(output), "original job required")
        with normal["capped"](w, output, HOST) as (member, _sample):
            diagnose(w, owner, args, output, member)
        return
    require(not output.exists(), "fresh output required")
    original(owner, args.audit, args.audit_sha)
    wall = min(WALL, int(CUTOFF_UNIX - time.time()))
    require(wall > 30, "campaign deadline reached")
    output.mkdir()
    result = owner.run_evaluation_fixture([sys.executable, "-I", "-B", "-S", __file__, *sys.argv[1:], "--worker"],
        n["ROOT"], output / "outer.log", wall, os.environ | {"AIRK_AWS_INSPECT_OWNER": str(output),
        "AIRK_AWS_INSPECT_DEADLINE": str(time.monotonic() + wall)})
    w["write_json"](output / "owned.json", result)
    normal["stage"]["require_settlement"](w, result)
    print(owner.digest(output / "reinspection.private.json"))


if __name__ == "__main__":
    main()
