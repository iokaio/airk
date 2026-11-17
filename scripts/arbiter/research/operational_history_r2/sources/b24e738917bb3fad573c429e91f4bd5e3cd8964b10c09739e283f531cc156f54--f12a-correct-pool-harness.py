# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('scripts/check-service-pool.py');s=p.read_text();pos=s.index('\ndef main():')
s=s[:pos]+'''
def semantic_recovery_mode(job, observations):
    """Require an authoritative terminal receipt or an exact deferred helper wait."""
    if job.get("kind") != "vectors" or not isinstance(job.get("id"), str):
        raise RuntimeError("Semantic recovery returned a foreign job kind or identity")
    if job.get("state") == "failed":
        if not isinstance(job.get("outcome"), dict) or job["outcome"].get("state") != "failed":
            raise RuntimeError("Failed semantic receipt lacks its original failure outcome")
        return "failed"
    if job.get("state") == "pending" and job.get("outcome") is None and job.get("attempts", 0) > 0:
        if any(row.get("job") == job["id"] and row.get("reason") == "helper_unavailable"
               and row.get("state") == "waiting" and row.get("attempts") == job["attempts"]
               and row.get("limit", 0) >= job["attempts"] for row in observations):
            return "waiting"
    return None

'''+s[pos:]
start=s.index('            # Readiness describes persisted coverage')
end=s.index('            if bridges[0].readiness()',start)
s=s[:start]+'''            # A helper wait retains its original receipt. Terminal exhaustion
            # instead requires an explicit Retry; neither state is inferred from
            # the lack of an event or from aggregate readiness.
            source_job_id = accepted["job"]["id"]
            semantic_id = None
            deadline = time.monotonic() + 180
            while True:
                events, event_cursor = event_tail(event_cursor, deadline)
                for event in events:
                    if event.get("job_kind") == "vectors" and event.get("job"):
                        if semantic_id is not None and event["job"] != semantic_id:
                            raise RuntimeError("Post-stop continuation changed its semantic job identity")
                        semantic_id = event["job"]
                if semantic_id is not None:
                    semantic = rpc({"job": {"id": semantic_id}}, deadline=deadline)["job"]
                    if semantic["id"] != semantic_id:
                        raise RuntimeError("Semantic job lookup returned a different receipt")
                    page = rpc({"console": {"after": None, "revision": None, "limit": 32}}, deadline=deadline)["page"]
                    rows = [row for row in page["projects"] if row["project"]["registration"]["binding"] == projects[0]["binding"]]
                    if page["after"] is not None or len(rows) != 1:
                        raise RuntimeError("Semantic recovery requires one complete exact project observation")
                    recovery_mode = semantic_recovery_mode(semantic, rows[0].get("recovery", []))
                    if recovery_mode is not None:
                        break
                    if semantic["state"] in ("canceled", "succeeded"):
                        raise RuntimeError("Post-stop semantic continuation did not retain unavailable work")
                clients.remaining(deadline, 180)
                time.sleep(0.2)
'''+s[end:]
s=s.replace('"Failed semantic job claimed complete coverage"','"Unavailable semantic work claimed complete coverage"')
s=s.replace('"Later semantic failure changed the successful source receipt"','"Semantic helper wait changed the successful source receipt"')
start=s.index('            # Failed semantic jobs require an explicit new request;')
end=s.index('            deadline = time.monotonic() + 180',start)
s=s[:start]+'''            if recovery_mode == "failed":
                # A terminal receipt is immutable. Only an explicit new request
                # may retry it; no uncertain inference operation is replayed.
                retried = rpc({"retry": {"id": semantic["id"], "caller": uuid.uuid4().hex,
                    "key": {"namespace": epoch["namespace"], "epoch": epoch["epoch"], "nonce": uuid.uuid4().hex}}})
                if retried.get("status") != "job" or retried["job"]["id"] == semantic["id"]:
                    raise RuntimeError("Semantic retry did not create a distinct accepted receipt")
                recovery_job_id = retried["job"]["id"]
            else:
                # Start releases the already-owned helper wait. No Retry or new
                # semantic request is authorized for this pending receipt.
                recovery_job_id = semantic["id"]
'''+s[end:]
s=s.replace('receipt = rpc({"job": {"id": accepted["job"]["id"]}}, deadline=deadline)["job"]\n                if receipt["state"] == "succeeded":\n                    break\n                if receipt["state"] in ("failed", "canceled")', 'receipt = rpc({"job": {"id": recovery_job_id}}, deadline=deadline)["job"]\n                if receipt["id"] != recovery_job_id:\n                    raise RuntimeError("Semantic recovery returned a different receipt")\n                if receipt["state"] == "succeeded":\n                    break\n                if receipt["state"] in ("failed", "canceled")')
s=s.replace('''            if rpc({"job": {"id": failed_semantic["id"]}})["job"]["state"] != "failed":
                raise RuntimeError("Explicit semantic retry rewrote the original failure receipt")''','''            original_semantic = rpc({"job": {"id": semantic["id"]}})["job"]
            if recovery_mode == "failed":
                if original_semantic != semantic:
                    raise RuntimeError("Explicit semantic retry rewrote the original failure receipt")
            elif original_semantic["id"] != semantic["id"] or original_semantic["state"] != "succeeded" or original_semantic["attempts"] <= semantic["attempts"]:
                raise RuntimeError("Helper start did not resume and complete the same semantic job")
            if rpc({"job": {"id": source_job_id}})["job"]["state"] != "succeeded":
                raise RuntimeError("Helper recovery changed the successful source receipt")''')
assert 'failed_semantic' not in s
compile(s,str(p),'exec');p.write_text(s)
