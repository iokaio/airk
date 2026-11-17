# SPDX-License-Identifier: Apache-2.0
# Diagnostics and setup only. The measured workload below remains unchanged.
class UsageFixture(clients.Fixture):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.failed_read_local = threading.local()
        self.failed_read_lock = threading.Lock()
        self.failed_read_count = 0

    def run(self, *args, **kwargs):
        result = super().run(*args, **kwargs)
        if getattr(self.failed_read_local, "active", False):
            self.failed_read_local.last = (args, result)
        return result

    def query(self, *args, **kwargs):
        self.failed_read_local.active = True
        self.failed_read_local.last = None
        try:
            return super().query(*args, **kwargs)
        except BaseException as error:
            try:
                with self.failed_read_lock:
                    assert self.failed_read_count < 32, "Failed-read diagnostic cap reached"
                    self.failed_read_count += 1
                    sequence = self.failed_read_count
                last = self.failed_read_local.last
                command, result = last if last is not None else ((), None)
                def bounded(raw):
                    raw = raw or b""
                    if isinstance(raw, str):
                        raw = raw.encode("utf-8", errors="replace")
                    return {"utf8": raw[:65536].decode("utf-8", errors="replace"),
                            "total_bytes": len(raw), "truncated": len(raw) > 65536}
                path = self.owner / f"failed-read-{sequence:02d}.json"
                with path.open("x", encoding="utf-8") as output:
                    json.dump({"command": [str(self.binary), *map(str, command)],
                               "query": [str(value) for value in args], "options": kwargs,
                               "returncode": result.returncode if result is not None else None,
                               "stdout": bounded(result.stdout if result is not None else getattr(error, "stdout", None)),
                               "stderr": bounded(result.stderr if result is not None else getattr(error, "stderr", None)),
                               "failure": str(error)[:4096]}, output, indent=2)
                error.add_note(f"Bounded exact failed-read evidence: {path}")
            except BaseException as diagnostic_error:
                error.add_note(f"Diagnostic retention failed: {diagnostic_error}")
            raise
        finally:
            self.failed_read_local.active = False


@contextmanager
def retained_usage_fixture():
    temporary = tempfile.TemporaryDirectory(prefix="airk-usage-inputs-")
    try:
        yield temporary.name
    except BaseException as error:
        temporary._finalizer.detach()
        error.add_note(f"Owned fictional usage fixture retained: {temporary.name}")
        print(json.dumps({"status": "failed", "retained_fixture": temporary.name}), flush=True)
        raise
    else:
        temporary.cleanup()


def usage_quiescence(fixture, project):
    """One atomic read-only metadata observation; never changes audit state."""
    binding = fixture.projects[project]["binding"]
    registry = (fixture.control / "service-1.4.sqlite3").resolve(strict=True)
    assert registry.is_relative_to(fixture.owner), "Usage registry escaped fictional owner"
    with sqlite3.connect(registry.as_uri() + "?mode=ro", uri=True, timeout=2) as connection:
        row = connection.execute(
            "SELECT paused,"
            "(SELECT count(*) FROM service_jobs WHERE project=p.binding AND state IN('pending','running')),"
            "(SELECT count(*) FROM service_recovery_budget WHERE scope=?) "
            "FROM projects p WHERE binding=?", ("integrity:" + binding, binding)).fetchone()
    assert row is not None, "Usage fixture registration absent"
    assert row[0] in (0, 1) and row[1] >= 0 and row[2] in (0, 1), row
    return {"paused": bool(row[0]), "active_jobs": row[1], "audit_veto": row[2] != 0}


@contextmanager
def stable_usage_publication(fixture, project):
    """Real audited, paused and quiescent owner before fixture writer exclusion.

    Pause is only a temporary desired-state setting of this fictional fixture.
    A claimed audit must drain while unpaused; a veto is never reset or ignored.
    No measured search or any whole-read replay is submitted by this preflight.
    """
    binding = fixture.projects[project]["binding"]
    prior = usage_quiescence(fixture, project)["paused"]
    primary = None
    try:
        if prior:
            fixture.rpc({"pause": {"project": binding, "paused": False}})
        # Native Verify succeeds only with actual complete physical proof. This
        # explicit setup job is outside the later 41-search accounting window.
        fixture.wait(fixture.enqueue(project, "verify"))
        deadline = time.monotonic() + 90
        for attempt in range(4):
            while True:
                observed = usage_quiescence(fixture, project)
                assert not observed["paused"], "Unexpected pause during audit drain"
                if observed["active_jobs"] == 0:
                    assert not observed["audit_veto"], ("Unresolved audit before usage measurement", observed)
                    break
                assert time.monotonic() < deadline, ("Usage preflight drain deadline", observed)
                time.sleep(min(0.025, clients.remaining(deadline, 90)))
            fixture.rpc({"pause": {"project": binding, "paused": True}})
            observed = usage_quiescence(fixture, project)
            assert observed["paused"], ("Pause was not retained", observed)
            if observed["active_jobs"] == 0 and not observed["audit_veto"]:
                break
            # A job can be claimed between the prior observation and Pause.
            # Restore execution and drain that actual job; never leave a live
            # Verify suspended behind the writer lock or clear its veto.
            fixture.rpc({"pause": {"project": binding, "paused": False}})
            assert attempt < 3 and time.monotonic() < deadline, ("Usage preflight raced repeatedly", observed)
        with fixture.hold_publication(project):
            yield
    except BaseException as error:
        primary = error
        raise
    finally:
        try:
            fixture.rpc({"pause": {"project": binding, "paused": prior}})
        except BaseException as error:
            if primary is None:
                raise
            primary.add_note(f"Failed to restore original fixture pause state: {error}")

