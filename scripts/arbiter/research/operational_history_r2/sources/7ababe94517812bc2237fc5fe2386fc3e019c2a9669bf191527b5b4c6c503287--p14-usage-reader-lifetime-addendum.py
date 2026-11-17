# SPDX-License-Identifier: Apache-2.0
def usage_plain(path, *, directory=False):
    info = path.lstat()
    assert not path.is_symlink() and not getattr(info, "st_file_attributes", 0) & 0x400, "Linked usage fixture path"
    assert path.is_dir() if directory else path.is_file(), "Unexpected usage fixture entry"
    if not directory:
        assert info.st_nlink == 1, "Aliased usage fixture file"
    return info.st_dev, info.st_ino


def usage_head_token(raw, head):
    # Compare the actual native token supplied by published_status. Hash the
    # exact saved canonical bytes, never a Python reserialization of CurrentHead.
    digest = hashlib.sha256(b"AIRK-DIGEST-1\x00")
    for part in (b"current-index-head/2", raw):
        digest.update(len(part).to_bytes(8, "big"))
        digest.update(part)
    return f"airk-r1:{head['scope']['incarnation']}:{head['sequence']}:{digest.hexdigest()}"


@contextmanager
def hold_usage_reader(fixture, project, expected_publication):
    """Fixture-only real WAL reader continuity inside the held native writer.

    This does not issue a native evidence query or modify its accounting. Pinning
    starts with BEGIN plus an actual current_state read, not connection creation.
    It does not replace native ownership, private integrity or source authority.
    """
    registration = fixture.projects[project]
    # This driver's source and state layout is explicit and fictional. Normalize
    # the registration's Windows extended prefix by same-file identity, rather
    # than treating lexical drive-prefix spellings as an ownership proof.
    data = (fixture.owner / "state").resolve(strict=True)
    assert data.samefile(Path(registration["data_root"]))
    assert data.is_relative_to(fixture.owner.resolve(strict=True))
    logical = data / "collections-1.4" / registration["collection"]
    for path in (data, logical.parent, logical):
        usage_plain(path, directory=True)
    manifest_path = logical / "active.json"
    manifest_id = usage_plain(manifest_path)
    manifest_raw = manifest_path.read_bytes()
    assert len(manifest_raw) <= 65536, "Usage manifest exceeds bound"
    manifest = json.loads(manifest_raw)
    active = manifest["active"]
    assert manifest["version"] == 140 and isinstance(active, str) and len(active) == 32
    assert all(c in "0123456789abcdef" for c in active)
    incarnations = logical / "incarnations"
    selected = incarnations / active
    for path in (incarnations, selected):
        usage_plain(path, directory=True)
    selected_id = usage_plain(selected, directory=True)
    owner_path = selected / "current-owner.json"
    owner_id = usage_plain(owner_path)
    owner_raw = owner_path.read_bytes()
    assert len(owner_raw) <= 65536
    owner = json.loads(owner_raw)
    expected_scope = {"root": registration["root"], "collection": registration["collection"], "incarnation": active}
    assert owner["version"] == 140 and owner["scope"] == expected_scope
    database = selected / "current.sqlite3"
    database_id = usage_plain(database)
    # Native service readers retain these owned names. Do not create/recover a
    # missing database or sidecar, and do not borrow a different selected slot.
    for suffix in ("-wal", "-shm"):
        usage_plain(Path(str(database) + suffix))
    assert not Path(str(database) + "-journal").exists()
    with closing(sqlite3.connect(database.as_uri() + "?mode=ro", uri=True, timeout=0)) as connection:
        connection.execute("PRAGMA query_only=ON")
        assert connection.execute("PRAGMA journal_mode").fetchone()[0] == "wal"
        connection.execute("BEGIN")
        phase, raw = connection.execute("SELECT phase,head FROM current_state WHERE id=1 AND length(head)<=65536").fetchone()
        head = json.loads(raw)
        assert phase == "ready" and head["version"] == 2 and head["scope"] == expected_scope
        assert isinstance(head["sequence"], int) and not isinstance(head["sequence"], bool) and head["sequence"] > 0
        token = usage_head_token(raw, head)
        assert token == expected_publication["token"] and head["sequence"] == expected_publication["generation"]
        def recheck():
            assert usage_plain(manifest_path) == manifest_id and manifest_path.read_bytes() == manifest_raw
            assert usage_plain(selected, directory=True) == selected_id
            assert usage_plain(owner_path) == owner_id and owner_path.read_bytes() == owner_raw
            assert usage_plain(database) == database_id
        recheck()
        try:
            yield
        finally:
            recheck()
            connection.rollback()  # Close this reader before writer release/pause restoration.

