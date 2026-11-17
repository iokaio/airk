# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
p=Path('scripts/check-service-pool.py');s=p.read_text()
s=s.replace('    args = parser.parse_args()', '    parser.add_argument("--diagnostic-dir", type=Path, required=True)\n    args = parser.parse_args()\n    diagnostic_dir = args.diagnostic_dir.resolve(strict=True)\n    if not diagnostic_dir.is_dir() or diagnostic_dir.is_symlink():\n        raise RuntimeError("Diagnostic directory must be an existing owned directory")')
s=s.replace('        bridges = []', '        bridges = []\n        projects = []\n        accepted = None\n        snapshot_sequence = 0')
marker='        try:\n            deadline = time.monotonic() + 90'
pos=s.index(marker)
block='''        def snapshot(stage):
            nonlocal snapshot_sequence
            sequence = snapshot_sequence
            snapshot_sequence += 1
            result = {"stage": stage, "observed_unix_ms": int(time.time()*1000), "responses": {}}
            until = time.monotonic()+15
            def observed(label, method):
                try:
                    path = fixture / "diagnostic-request.json"
                    path.write_text(json.dumps(method), encoding="utf-8")
                    return run("service", "rpc", "--runtime-dir", runtime, "--request-file", path,
                               timeout=clients.remaining(until, 5))
                except Exception as error:
                    result["responses"][label] = {"observation_failed": True, "exception_type": type(error).__name__}
                    return None
            def job(value):
                if not isinstance(value,dict):return None
                return {key:value.get(key) for key in ["id","kind","state","attempts","outcome"]}
            if projects:
                binding = projects[0]["binding"]
                value = observed("progress", {"progress":{"project":binding}})
                if value is not None:
                    observation = value.get("observation") or {}
                    progress = observation.get("progress") or {}
                    result["responses"]["progress"] = {
                        "pending_job":job(value.get("pending_job")), "queued_jobs":[job(row) for row in value.get("queued_jobs",[])],
                        "queue_truncated":value.get("queue_truncated"), "project_paused":value.get("project_paused"),
                        "active_job":job(observation.get("job")),
                        "progress":{key:progress.get(key) for key in ["phase","sequence","paused","stalled","elapsed_ms","captured_files","extracted_files","prepared_changes","committed_changes"]}}
                value = observed("console", {"console":{"after":None,"revision":None,"limit":32}})
                if value is not None:
                    rows=[]
                    for row in value.get("page",{}).get("projects",[]):
                        if row.get("project",{}).get("registration",{}).get("binding") == binding:
                            rows.append({"pending_job":job(row.get("pending_job")),"recovery":[{key:r.get(key) for key in ["reason","state","attempts","limit","job","next_check_unix_ms"]} for r in row.get("recovery",[])],"failed_observations":row.get("failed_observations",[])})
                    result["responses"]["console"]={"selected_rows":rows,"complete":value.get("page",{}).get("after") is None}
                value = observed("events", {"events":{"project":binding,"cursor":None,"limit":32}})
                if value is not None:
                    result["responses"]["events"]={"events":[{key:row.get(key) for key in ["sequence","kind","job","job_kind","attempts","code"]} for row in value.get("page",{}).get("events",[])],"more":value.get("page",{}).get("next") is not None}
            value = observed("embedding", "embedding_status")
            if value is not None:result["responses"]["embedding"]={key:value.get(key) for key in ["status","available","worker","loads","sessions"]}
            if accepted:
                value=observed("source_job", {"job":{"id":accepted["job"]["id"]}})
                if value is not None:result["responses"]["source_job"]=job(value.get("job"))
            with (diagnostic_dir/f"snapshot-{sequence:02d}-{stage}.json").open("x",encoding="utf-8") as output:
                json.dump(result,output,indent=2)

'''
s=s[:pos]+block+s[pos:]
needle='''            deadline = time.monotonic() + 180
            while True:
                events, event_cursor = event_tail(event_cursor, deadline)'''
s=s.replace(needle,'''            deadline = time.monotonic() + 180
            snapshot("after-source")
            diagnostic_after = time.monotonic()+10
            while True:
                if diagnostic_after is not None and time.monotonic() >= diagnostic_after:
                    snapshot("waiting-event")
                    diagnostic_after = None
                events, event_cursor = event_tail(event_cursor, deadline)''')
s=s.replace('''        finally:
            for bridge in bridges:''','''        except Exception:
            try:
                snapshot("failure")
            except Exception as diagnostic_error:
                with (diagnostic_dir/"snapshot-failure-error.txt").open("x") as output:
                    output.write(type(diagnostic_error).__name__)
            raise
        finally:
            for bridge in bridges:''')
s=s.replace('''            log.close()
''','''            log.close()
            with (diagnostic_dir/"service-stderr-tail.bin").open("xb") as output:
                with (fixture/"service.stderr").open("rb") as source_log:
                    source_log.seek(max(0,source_log.seek(0,2)-65536))
                    output.write(source_log.read(65536))
''')
compile(s,'pool-diagnostic','exec')
Path('artifacts/optimizations/f12a-pool-diagnostic.py').write_text(s)
