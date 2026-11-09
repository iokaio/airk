# SPDX-License-Identifier: Apache-2.0
"""Step S3: capture the retained `airk-state/1` SQLite schema fixtures
(docs/simplify-plan.md section 5.2, S3; compatibility inventory Appendix D).

The fixtures record what the *baseline's own* code writes to every persisted
SQLite database of an `airk-state/1` installation. The script never builds or
reads the main checkout's sources or target directory:

1. `git worktree add --detach` the requested revision (default 43d7005, the
   1.8.16 baseline) under the system temporary directory;
2. write a small test-only capture driver into that worktree only (appended
   test functions in existing airk-store test modules plus one integration
   test file). Each driver test returns immediately unless
   AIRK_S3_CAPTURE_DIR is set;
3. run one scoped `cargo test -p airk-store` with a separate --target-dir and
   AIRK_S3_CAPTURE_DIR pointing at a temporary raw-dump directory;
4. render each raw `sqlite_master` dump as `<db>.objects.json` (type, name,
   tbl_name, sql; sorted by type, name, tbl_name) and `<db>.meta.json`
   (PRAGMA user_version and application_id), plus manifest.json. The only
   normalization is CRLF -> LF inside `sql`, the same normalization the
   registry reader applies (`schema_objects`);
5. remove the worktree.

    python scripts/dev/s3_capture_schema.py                     # write fixtures
    python scripts/dev/s3_capture_schema.py --check             # recapture and diff
    python scripts/dev/s3_capture_schema.py --raw-dir DIR       # render saved raw dumps only
    python scripts/dev/s3_capture_schema.py --print-container-entries

`rootpage` is deliberately not captured: it depends on allocation order, not
on the schema. Exact on-disk bytes of `sql` are not claimed either, because a
CRLF checkout embeds CRLF from include_str!'d .sql files.
"""
import argparse
import difflib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

DEFAULT_REVISION = "43d7005"
FIXTURE_DIRECTORY = "fixtures/schema/airk-state-1"
MANIFEST_SCHEMA = "airk.s3-schema-capture/1"
ENVIRONMENT = "AIRK_S3_CAPTURE_DIR"
TEST_PREFIX = "s3_capture_"
COLLECTION_TEST_TARGET = "s3_capture_schema"

ARMED = "; then runtime_contract::arm_in_transaction (registry user_version 13)"
# Reviewed label catalog: group -> [(label, how the baseline created it)].
GROUPS = {
    "registry": [
        ("service-1.4.schema11", "Fresh ServiceRegistry::open (registry steps 1-11) at CatalogGenesis::begin admission"),
        ("catalog-schema11-backup", "CatalogGenesis::backup qualified backup catalog-schema11-<operation>-<slot>.sqlite3"),
        ("service-1.4.authority0", "CatalogGenesis::migrate (user_version 12, authority 0, inactive)"),
        ("service-1.4.authority1", "promote_authority_v1, record_authority_v1_readiness, settle_authority_v1; reopened by ServiceRegistry::open"),
        ("service-1.4.policy1", "settled registry, then apply_policy_v1_layout"),
        ("service-1.4.policy2", "settled registry, then apply_split_v2_layout"),
        ("service-1.4.policy3", "settled registry, then apply_split_extraction_v3_layout"),
        ("service-1.4.authority1-armed", "settled registry" + ARMED),
        ("service-1.4.policy1-armed", "settled registry" + ARMED + ", then apply_policy_v1_layout"),
        ("service-1.4.policy2-armed", "settled registry" + ARMED + ", then apply_split_v2_layout"),
        ("service-1.4.policy3-armed", "settled registry" + ARMED + ", then apply_split_extraction_v3_layout"),
    ],
    "adopted": [
        ("service-1.4.adopted-policy%d" % policy,
         "completed_authority_fixture%s, then apply_adopted_authority_layout (user_version 13)"
         % ("" if policy == 0 else ", policy layer %d" % policy))
        for policy in range(4)
    ],
    "current": [
        ("current.service", "collections-1.4 CurrentWriter::initialize (service WAL, user_version 140)"),
        ("current.standalone", "collections-1.2 CurrentWriter::initialize (rollback journal, user_version 126)"),
    ],
    "collection": [
        ("scheduler-1.2", "scheduler::Queue::enqueue on a fresh data root"),
        ("usage.base", "UsageStore::open on a fresh managed collection"),
        ("usage.supplemental", "UsageStore::record then record_supplement (lazy supplemental tables)"),
    ],
}
GROUP_ORDER = ("registry", "adopted", "current", "collection")

# Driver placement per source layout. The first existing candidate is used.
TARGETS = {
    "registry": ("crates/airk-store/src/service_catalog_genesis_tests.rs",
                 "crates/airk-store/src/service/registry/catalog/genesis/tests.rs"),
    "adopted": ("crates/airk-store/src/service_catalog_adopted_authority_tests.rs",
                "crates/airk-store/src/service/registry/catalog/adopted_authority/tests.rs"),
    "current": ("crates/airk-store/src/current/service_publication_tests.rs",
                "crates/airk-store/src/current/tests/service_publication_tests.rs"),
    "collection": ("crates/airk-store/tests/%s.rs" % COLLECTION_TEST_TARGET,),
}
TEST_NAMES = {
    "registry": "s3_capture_registry_layers",
    "adopted": "s3_capture_adopted_layouts",
    "current": "s3_capture_current_layouts",
    "collection": "s3_capture_collection_databases",
}

DUMP = r'''
fn s3_dump(connection: &rusqlite::Connection, label: &str) -> anyhow::Result<()> {
    let Some(out) = std::env::var_os("AIRK_S3_CAPTURE_DIR") else {
        return Ok(());
    };
    let mut statement = connection.prepare("SELECT type,name,tbl_name,sql FROM sqlite_master")?;
    let objects = statement
        .query_map([], |row| {
            Ok(serde_json::json!({
                "type": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "tbl_name": row.get::<_, String>(2)?,
                "sql": row.get::<_, Option<String>>(3)?,
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let value = serde_json::json!({
        "label": label,
        "user_version": connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?,
        "application_id": connection.pragma_query_value(None, "application_id", |row| row.get::<_, i64>(0))?,
        "sqlite_version": connection.query_row("SELECT sqlite_version()", [], |row| row.get::<_, String>(0))?,
        "objects": objects,
    });
    std::fs::write(
        std::path::Path::new(&out).join(format!("{label}.raw.json")),
        serde_json::to_vec_pretty(&value)?,
    )?;
    Ok(())
}
'''

LAYERS = r'''[
        ("policy1", super::super::apply_policy_v1_layout),
        ("policy2", super::super::apply_split_v2_layout),
        ("policy3", super::super::apply_split_extraction_v3_layout),
    ]'''

DRIVERS = {
    # Appended to genesis::tests: begin(), registry() and backup are in scope.
    "registry": r'''
struct S3Producer {
    root: std::path::PathBuf,
    operation: Incarnation,
}

impl activation::AuthorityV1ReadinessProducer for S3Producer {
    fn required_predecessor_identities(&self) -> anyhow::Result<Vec<Sha256Digest>> {
        Ok(Vec::new())
    }
    fn qualify(
        &self,
        expected: &activation::AuthorityV1CandidateObservation,
    ) -> anyhow::Result<activation::AuthorityV1NativeQualification> {
        Ok(activation::AuthorityV1NativeQualification {
            candidate: observe_authority_v1(&self.root, self.operation, expected.nonce)?,
            predecessor_refusals: Vec::new(),
        })
    }
}

fn s3_settled(
    root: &std::path::Path,
    record: bool,
) -> anyhow::Result<super::super::super::ServiceRegistry> {
    use anyhow::Context as _;
    let mut genesis = begin(root)?;
    let receipt = genesis.receipt()?;
    if record {
        s3_dump(&registry(&receipt.root, false)?, "service-1.4.schema11")?;
    }
    let backed = genesis.backup()?;
    if record {
        let attempt = backed
            .backups
            .iter()
            .find(|attempt| attempt.complete)
            .context("qualified genesis backup missing")?;
        let path = backup::validate(&backed.root, backed.operation, attempt)?;
        let saved =
            rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        s3_dump(&saved, "catalog-schema11-backup")?;
    }
    genesis.migrate()?;
    if record {
        s3_dump(&registry(&receipt.root, false)?, "service-1.4.authority0")?;
    }
    genesis.promote_authority_v1()?;
    genesis.record_authority_v1_readiness(
        Incarnation::from_bytes([119; 16]),
        &S3Producer {
            root: receipt.root.clone(),
            operation: receipt.operation,
        },
    )?;
    anyhow::ensure!(
        genesis.settle_authority_v1()?.stage == GenesisStage::Settled,
        "genesis did not settle"
    );
    drop(genesis);
    super::super::super::ServiceRegistry::open(root, receipt.namespace)
}

#[test]
fn s3_capture_registry_layers() -> anyhow::Result<()> {
    if std::env::var_os("AIRK_S3_CAPTURE_DIR").is_none() {
        return Ok(());
    }
    let layers: [(&str, fn(&rusqlite::Connection) -> anyhow::Result<()>); 3] = LAYERS;
    for armed in [false, true] {
        let temp = tempfile::tempdir()?;
        let mut saved = s3_settled(temp.path(), !armed)?;
        let suffix = if armed { "-armed" } else { "" };
        if armed {
            let tx = saved.connection.transaction()?;
            crate::service::registry::runtime_contract::arm_in_transaction(&tx)?;
            tx.commit()?;
        }
        s3_dump(&saved.connection, &format!("service-1.4.authority1{suffix}"))?;
        for (name, apply) in layers {
            let tx = saved.connection.transaction()?;
            apply(&tx)?;
            tx.commit()?;
            s3_dump(&saved.connection, &format!("service-1.4.{name}{suffix}"))?;
        }
    }
    Ok(())
}
'''.replace("LAYERS", LAYERS),
    # Appended to adopted_authority::tests: fixture() is in scope.
    "adopted": r'''
#[test]
fn s3_capture_adopted_layouts() -> anyhow::Result<()> {
    if std::env::var_os("AIRK_S3_CAPTURE_DIR").is_none() {
        return Ok(());
    }
    let layers: [fn(&rusqlite::Connection) -> anyhow::Result<()>; 3] = [
        super::super::apply_policy_v1_layout,
        super::super::apply_split_v2_layout,
        super::super::apply_split_extraction_v3_layout,
    ];
    for policy in 0..=3usize {
        let (_temp, mut registry, _native) = fixture()?;
        let tx = registry.connection.transaction()?;
        if policy > 0 {
            layers[policy - 1](&tx)?;
        }
        super::super::apply_adopted_authority_layout(&tx)?;
        tx.commit()?;
        super::super::validate_existing(&registry.connection)?;
        s3_dump(
            &registry.connection,
            &format!("service-1.4.adopted-policy{policy}"),
        )?;
    }
    Ok(())
}
''',
    # Appended to current::tests::service_publication_tests.
    "current": r'''
#[test]
fn s3_capture_current_layouts() -> anyhow::Result<()> {
    if std::env::var_os("AIRK_S3_CAPTURE_DIR").is_none() {
        return Ok(());
    }
    let service = service_fixture()?;
    let standalone = Fixture::new()?;
    for (label, fixture) in [("current.service", &service), ("current.standalone", &standalone)] {
        let path = fixture.store.directory().join("current.sqlite3");
        // The canonical reader handles the service WAL VFS and version checks.
        let connection = crate::current::database::open_reader(&path)?;
        s3_dump(&connection, label)?;
    }
    Ok(())
}
''',
    # A new integration test file; the observation mirrors tests/usage.rs.
    "collection": r'''// SPDX-License-Identifier: Apache-2.0
//! S3 schema capture driver. Written into a disposable worktree by
//! scripts/dev/s3_capture_schema.py; never committed.
use airk_core::{Snapshot, SnapshotStore, usage::*};
use airk_store::{
    FileStore,
    scheduler::Queue,
    usage::{self, UsageStore},
};
use anyhow::Context as _;
DUMP
fn read_only(path: &std::path::Path) -> anyhow::Result<rusqlite::Connection> {
    Ok(rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)
}

fn observation(scope: &UsageScope, id: u8) -> UsageObservation {
    UsageObservation {
        schema_version: USAGE_SCHEMA_VERSION,
        event_id: [id; 16],
        scope: scope.clone(),
        unix_ms: usage::unix_ms(),
        generation: Some(1),
        operation: UsageOperation::Search,
        client: ClientFamily::Codex,
        content_kind: ContentKind::Markdown,
        outcome: UsageOutcome::Success,
        elapsed_us: 10,
        cache: CacheObservation::Unknown,
        returned_items: 1,
        truncated: false,
        response_representation: ResponseRepresentation::LogicalResultJson,
        response_utf8_bytes: Some(400),
        baseline_unavailable: None,
        baseline: Some(ContextBaseline {
            kind: BaselineKind::FullReturnedSources,
            generation: 1,
            representation: ResponseRepresentation::LogicalResultJson,
            utf8_bytes: 80,
            unique_sources: 1,
        }),
        index_work: None,
        resources: None,
    }
}

#[test]
fn s3_capture_collection_databases() -> anyhow::Result<()> {
    if std::env::var_os("AIRK_S3_CAPTURE_DIR").is_none() {
        return Ok(());
    }
    let data = tempfile::tempdir()?;
    Queue::new(data.path()).enqueue("project", "fixture-root", false, true, false, 1)?;
    s3_dump(&read_only(&data.path().join("scheduler-1.2.sqlite3"))?, "scheduler-1.2")?;

    let root = tempfile::tempdir()?;
    let directory = root.path().join("collections-1.2/project");
    let mut snapshot = Snapshot::empty("fixture-root".into());
    snapshot.collection = "project".into();
    FileStore::new(&directory).save(&snapshot)?;
    let scope = UsageScope {
        collection: "project".into(),
        root_binding: [7; 32],
    };
    let store = UsageStore::open(&directory, &scope, &UsagePolicy::default())?
        .context("usage recording is disabled")?;
    let database = directory.join("usage.sqlite3");
    s3_dump(&read_only(&database)?, "usage.base")?;
    store.record(&observation(&scope, 1))?;
    let event: SupplementalMeasurement = serde_json::from_value(serde_json::json!({
        "schema_version":1,"event_id":"00000000000000000000000000000001","unix_ms":usage::unix_ms(),
        "client":"codex","content_kind":"code","provenance":"client_reported","context":null,"paired_task":null,
        "session":{"model_revision_sha256":"a".repeat(64),"requests":1,"input_tokens":10,"cached_input_tokens":null,"output_tokens":null,"elapsed_ms":null,"tool_round_trips":null},
        "airk_request_ids":["01".repeat(16)]
    }))?;
    anyhow::ensure!(store.record_supplement(&event)?, "supplement was not recorded");
    s3_dump(&read_only(&database)?, "usage.supplemental")?;
    Ok(())
}
'''.replace("DUMP", DUMP),
}
MARKER = "\n// ---- S3 schema capture driver (scripts/dev/s3_capture_schema.py); never committed ----\n"


class CaptureError(Exception):
    pass


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--revision", default=DEFAULT_REVISION, help="commit to capture (default: %(default)s)")
    parser.add_argument("--output", type=Path, help="fixture directory (default: <repo>/" + FIXTURE_DIRECTORY + ")")
    parser.add_argument("--target-dir", type=Path,
                        help="Cargo target directory for the baseline build (default: <system temp>/airk-s3-target); "
                             "never the main checkout's target/")
    parser.add_argument("--cargo", default="cargo", help="Cargo executable")
    parser.add_argument("--groups", default=",".join(GROUP_ORDER),
                        help="comma-separated capture groups (default: all): " + ", ".join(GROUP_ORDER))
    parser.add_argument("--no-timings", action="store_true", help="omit cargo --timings")
    parser.add_argument("--offline", action="store_true", help="pass --offline to cargo")
    parser.add_argument("--keep-worktree", action="store_true", help="leave the temporary worktree for inspection")
    parser.add_argument("--raw-dir", type=Path,
                        help="render existing *.raw.json dumps; no worktree or cargo run")
    parser.add_argument("--save-raw", type=Path, help="copy the raw dumps here after a cargo capture")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="recapture and diff against --output; write nothing")
    mode.add_argument("--print-container-entries", action="store_true",
                      help="print the .dockerignore, Dockerfile and container-inputs.json entries and exit")
    args = parser.parse_args(argv)
    groups = [group.strip() for group in args.groups.split(",") if group.strip()]
    unknown = sorted(set(groups) - set(GROUP_ORDER))
    if unknown or not groups:
        parser.error("unknown or empty --groups: " + ", ".join(unknown))
    args.groups = [group for group in GROUP_ORDER if group in groups]
    return args


def labels(groups):
    return [(label, how) for group in groups for label, how in GROUPS[group]]


def normalize_sql(sql):
    if sql is None:
        return None
    if not isinstance(sql, str):
        raise CaptureError("sqlite_master.sql must be text or null")
    return sql.replace("\r\n", "\n")


def objects_document(rows):
    """Sort raw sqlite_master rows by (type, name, tbl_name); CRLF -> LF in sql only."""
    objects = []
    seen = set()
    for row in rows:
        if set(row) != {"type", "name", "tbl_name", "sql"}:
            raise CaptureError("unexpected sqlite_master row fields: " + ", ".join(sorted(row)))
        for field in ("type", "name", "tbl_name"):
            if not isinstance(row[field], str):
                raise CaptureError("sqlite_master.%s must be text" % field)
        if row["name"] in seen:
            raise CaptureError("duplicate sqlite_master name: " + row["name"])
        seen.add(row["name"])
        objects.append({"type": row["type"], "name": row["name"], "tbl_name": row["tbl_name"],
                        "sql": normalize_sql(row["sql"])})
    # Python str order is code-point order, which equals UTF-8 byte order and
    # Rust String order; names are unique, so sql never decides.
    return sorted(objects, key=lambda item: (item["type"], item["name"], item["tbl_name"]))


def meta_document(raw):
    meta = {}
    for field in ("user_version", "application_id"):
        value = raw.get(field)
        if not isinstance(value, int) or isinstance(value, bool):
            raise CaptureError(field + " must be an integer")
        meta[field] = value
    return meta


def render_json(value):
    return (json.dumps(value, indent=2, ensure_ascii=False, sort_keys=True) + "\n").encode("utf-8")


def render(raws, groups, revision, version):
    """Return {file name: bytes} for the selected groups from {label: raw dump}."""
    expected = labels(groups)
    names = [label for label, _ in expected]
    missing = sorted(set(names) - set(raws))
    extra = sorted(set(raws) - set(names))
    if missing or extra:
        raise CaptureError("raw dump set differs: missing %s; unexpected %s" % (missing or "none", extra or "none"))
    versions = sorted({raws[label].get("sqlite_version") for label in names})
    if len(versions) != 1 or not isinstance(versions[0], str):
        raise CaptureError("raw dumps disagree on the producer SQLite version: %s" % versions)
    files = {}
    databases = {}
    for label, how in expected:
        raw = raws[label]
        if raw.get("label") != label:
            raise CaptureError("raw dump %s names label %r" % (label, raw.get("label")))
        if not isinstance(raw.get("objects"), list) or not raw["objects"]:
            raise CaptureError("raw dump %s has no sqlite_master rows" % label)
        files[label + ".objects.json"] = render_json(objects_document(raw["objects"]))
        files[label + ".meta.json"] = render_json(meta_document(raw))
        databases[label] = {"objects": label + ".objects.json", "meta": label + ".meta.json", "created_by": how}
    files["manifest.json"] = render_json({
        "schema": MANIFEST_SCHEMA,
        "state_contract": "airk-state/1",
        "source_revision": revision,
        "source_version": version,
        "producer_sqlite_version": versions[0],
        "groups": list(groups),
        "normalization": "sqlite_master rows sorted by type, name, tbl_name; CRLF -> LF in sql; rootpage omitted",
        "command": "python scripts/dev/s3_capture_schema.py --revision " + revision,
        "databases": databases,
    })
    return files


def load_raw(directory):
    raws = {}
    for path in sorted(Path(directory).glob("*.raw.json")):
        raws[path.name[:-len(".raw.json")]] = json.loads(path.read_text(encoding="utf-8"))
    return raws


def diff_files(expected_files, directory, compare_manifest=True):
    """Compare rendered bytes with a directory; return human-readable differences."""
    directory = Path(directory)
    differences = []
    for name, data in sorted(expected_files.items()):
        if name == "manifest.json" and not compare_manifest:
            continue
        path = directory / name
        if not path.is_file():
            differences.append("missing " + name)
            continue
        actual = path.read_bytes()
        if actual != data:
            differences.append("".join(difflib.unified_diff(
                actual.decode("utf-8", "replace").splitlines(True), data.decode("utf-8").splitlines(True),
                "fixture/" + name, "recaptured/" + name)))
    if compare_manifest and directory.is_dir():
        for path in sorted(directory.glob("*.json")):
            if path.name not in expected_files:
                differences.append("unexpected " + path.name)
    return differences


def selected_test_count(output):
    return sum(int(match) for match in re.findall(r"^running (\d+) tests?$", output, re.MULTILINE))


def driver_plan(worktree, groups):
    """Return [(relative path, mode, text)] for the selected groups in this source layout."""
    plan = []
    for group in groups:
        candidates = TARGETS[group]
        if group == "collection":
            path = candidates[0]
            if (Path(worktree) / path).exists():
                raise CaptureError(path + " already exists in the worktree")
            plan.append((path, "create", DRIVERS[group]))
            continue
        existing = [path for path in candidates if (Path(worktree) / path).is_file()]
        if len(existing) != 1:
            raise CaptureError("cannot place the %s driver: expected exactly one of %s" % (group, ", ".join(candidates)))
        plan.append((existing[0], "append", MARKER + DUMP + DRIVERS[group]))
    return plan


def apply_plan(worktree, plan):
    for relative, mode, text in plan:
        path = Path(worktree) / relative
        if mode == "create":
            path.write_text(text, encoding="utf-8", newline="\n")
        else:
            with path.open("a", encoding="utf-8", newline="\n") as handle:
                handle.write(text)


def cargo_command(args, groups, target_dir):
    command = [args.cargo, "test", "--locked", "-p", "airk-store", "--target-dir", str(target_dir)]
    if args.offline:
        command.append("--offline")
    if not args.no_timings:
        command.append("--timings")
    if any(group != "collection" for group in groups):
        command.append("--lib")
    if "collection" in groups:
        command += ["--test", COLLECTION_TEST_TARGET]
    return command + [TEST_PREFIX, "--", "--test-threads=1"]


def container_entries(groups):
    names = [FIXTURE_DIRECTORY + "/" + label + suffix
             for label, _ in labels(groups) for suffix in (".objects.json", ".meta.json")]
    names.sort()
    dockerignore = ["!fixtures/schema/", "fixtures/schema/**",
                    "!" + FIXTURE_DIRECTORY + "/", FIXTURE_DIRECTORY + "/**"]
    return {
        ".dockerignore (parent block, after fixtures/wire/**)": dockerignore,
        ".dockerignore (exact files, before !fixtures/wire/catalog_receipt.json)": ["!" + name for name in names],
        "Dockerfile (before COPY fixtures/wire/catalog_receipt.json)": ["COPY %s ./%s" % (name, name) for name in names],
        "scripts/container-inputs.json files (before fixtures/wire/catalog_receipt.json)":
            ['    "%s",' % name for name in names],
    }


def git(repository, *arguments, capture=True):
    result = subprocess.run(["git", "-C", str(repository), *arguments], check=False,
                            stdout=subprocess.PIPE if capture else None, stderr=subprocess.PIPE if capture else None,
                            text=True)
    if result.returncode != 0:
        raise CaptureError("git %s failed: %s" % (" ".join(arguments), (result.stderr or "").strip()))
    return (result.stdout or "").strip()


def source_version(repository, revision):
    manifest = git(repository, "show", revision + ":Cargo.toml")
    match = re.search(r'^version\s*=\s*"([^"]+)"', manifest, re.MULTILINE)
    if not match:
        raise CaptureError("workspace version not found in Cargo.toml at " + revision)
    return match[1]


def capture_with_cargo(args, repository, revision, groups, raw_directory):
    target_dir = (args.target_dir or Path(tempfile.gettempdir()) / "airk-s3-target").resolve()
    main_target = (repository / "target").resolve()
    if target_dir == main_target or main_target in target_dir.parents:
        raise CaptureError("refusing the main checkout's target directory: " + str(target_dir))
    worktree = Path(tempfile.mkdtemp(prefix="airk-s3-"))
    worktree.rmdir()
    git(repository, "worktree", "add", "--detach", str(worktree), revision)
    try:
        plan = driver_plan(worktree, groups)
        apply_plan(worktree, plan)
        command = cargo_command(args, groups, target_dir)
        print("worktree:", worktree)
        print("command:", " ".join(command))
        environment = dict(os.environ, **{ENVIRONMENT: str(raw_directory)})
        result = subprocess.run(command, cwd=worktree, env=environment, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, text=True, check=False)
        sys.stdout.write(result.stdout)
        if result.returncode != 0:
            raise CaptureError("cargo test failed with exit code %d" % result.returncode)
        selected = selected_test_count(result.stdout)
        if selected != len(groups):
            raise CaptureError("expected %d selected capture tests, observed %d" % (len(groups), selected))
        if not args.no_timings:
            print("cargo timings:", target_dir / "cargo-timings")
    finally:
        if args.keep_worktree:
            print("kept worktree:", worktree)
        else:
            git(repository, "worktree", "remove", "--force", str(worktree))
    if args.save_raw:
        args.save_raw.mkdir(parents=True, exist_ok=True)
        for path in Path(raw_directory).glob("*.raw.json"):
            shutil.copy2(path, args.save_raw / path.name)


def main(argv=None):
    args = parse_args(argv)
    if args.print_container_entries:
        for heading, lines in container_entries(args.groups).items():
            print("# " + heading)
            print("\n".join(lines))
        return 0
    repository = Path(git(Path(__file__).resolve().parent, "rev-parse", "--show-toplevel"))
    revision = git(repository, "rev-parse", "--verify", args.revision + "^{commit}")
    version = source_version(repository, revision)
    output = args.output or repository / FIXTURE_DIRECTORY
    with tempfile.TemporaryDirectory(prefix="airk-s3-raw-") as temporary:
        if args.raw_dir:
            raw_directory = args.raw_dir
        else:
            raw_directory = Path(temporary)
            capture_with_cargo(args, repository, revision, args.groups, raw_directory)
        files = render(load_raw(raw_directory), args.groups, revision, version)
    if args.check:
        differences = diff_files(files, output, compare_manifest=args.groups == list(GROUP_ORDER))
        for difference in differences:
            print(difference)
        print("%d difference(s) against %s" % (len(differences), output))
        return 1 if differences else 0
    output.mkdir(parents=True, exist_ok=True)
    for name, data in files.items():
        (output / name).write_bytes(data)
    stale = sorted(path.name for path in output.glob("*.json") if path.name not in files)
    print("wrote %d files to %s" % (len(files), output))
    if stale:
        print("files not produced by this capture remain: " + ", ".join(stale))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except CaptureError as error:
        print("error: %s" % error, file=sys.stderr)
        sys.exit(2)
