// SPDX-License-Identifier: Apache-2.0
//! Audit committed unit citations from pinned, detached index databases.
use super::{DATABASE_FILE, FileRow, IndexReader, MAX_PAGE, UnitRow};
use crate::current::CapturedInput;
use airk_core::{
    Citation, RetrievalArtifact,
    binding::{ArtifactBinding, bind_payload},
    identity::Sha256Digest,
    vector_evidence::{VectorEvidenceReader, VectorMemberSpan},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_INPUT_BYTES: u64 = 1 << 20;
const MAX_PATHS: usize = 512;
const MAX_OCCURRENCES: usize = 100_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditInput {
    collections: Vec<AuditCollection>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditCollection {
    repository: String,
    database: PathBuf,
    publication_sequence: u64,
    pipeline: String,
    committed_rows: u64,
    db_sha256: String,
    paths: Vec<String>,
}

#[derive(Serialize)]
struct AuditPath<'a> {
    repository: &'a str,
    path: &'a str,
    source_present: bool,
    units: Vec<AuditUnit>,
}

#[derive(Serialize)]
struct AuditUnit {
    row: u64,
    ordinal: u32,
    members: Vec<VectorMemberSpan>,
    citation: Citation,
}

fn sha256_file(path: &Path) -> Result<Sha256Digest> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let bytes = file.read(&mut buffer)?;
        if bytes == 0 {
            return Ok(Sha256Digest::from_bytes(hash.finalize().into()));
        }
        hash.update(&buffer[..bytes]);
    }
}

fn open_snapshot(root: &Path, collection: &AuditCollection) -> Result<IndexReader> {
    let directory = collection.database.canonicalize()?;
    ensure!(
        directory == root.join("db").join(&collection.repository),
        "database must be the detached audit copy"
    );
    let database = directory.join(DATABASE_FILE);
    ensure!(
        database.symlink_metadata()?.file_type().is_file(),
        "detached database must be a regular file"
    );
    for suffix in ["-wal", "-journal"] {
        let sidecar = directory.join(format!("{DATABASE_FILE}{suffix}"));
        ensure!(
            !sidecar.try_exists()? || sidecar.symlink_metadata()?.len() == 0,
            "detached database has an uncommitted sidecar"
        );
    }
    ensure!(
        sha256_file(&database)? == collection.db_sha256.parse::<Sha256Digest>()?,
        "detached database SHA-256 differs"
    );
    let index = IndexReader::open(&directory)?;
    ensure!(
        index.sequence() == collection.publication_sequence,
        "detached publication sequence differs"
    );
    let (setup, _, committed_rows) = index.vector_setup()?.context("vector setup missing")?;
    ensure!(
        committed_rows == collection.committed_rows
            && setup.pipeline == collection.pipeline.parse::<Sha256Digest>()?,
        "detached vector publication differs"
    );
    Ok(index)
}

fn retrieval_artifact(index: &IndexReader, file: &FileRow) -> Result<RetrievalArtifact> {
    let captured: CapturedInput = serde_json::from_slice(&file.record)?;
    ensure!(
        captured.postings_version <= 1 && serde_json::to_vec(&captured)? == file.record,
        "current artifact record is not canonical"
    );
    let compact = captured.artifact;
    let reference = compact.retained.context("retained reference missing")?;
    let profile = reference.profile.context("extraction profile missing")?;
    ensure!(
        compact.path == file.path
            && compact.content_hash.parse::<Sha256Digest>()? == file.source_hash
            && reference.object.parse::<Sha256Digest>()? == file.payload
            && profile.chunk_version == 3,
        "retained artifact identity differs from indexed file"
    );
    let bytes = index.payload(&file.payload)?;
    ensure!(
        bytes.len() == reference.object_bytes,
        "retained extraction length differs"
    );
    let payload = serde_json::from_slice(&bytes)?;
    let artifact = bind_payload(
        &payload,
        &profile,
        ArtifactBinding {
            path: &file.path,
            source_hash: file.source_hash,
            source_bytes: compact.bytes,
        },
    )?;
    ensure!(
        artifact.language == compact.language
            && artifact.extraction_method == compact.extraction_method
            && artifact.parse_status == compact.parse_status
            && artifact.chunks.len() == reference.chunks
            && artifact.extracted_bytes() == reference.extracted_bytes
            && artifact.facts.len() == reference.facts,
        "bound retained extraction differs from indexed file"
    );
    Ok(RetrievalArtifact {
        artifact,
        profile,
        payload,
        payload_digest: file.payload,
    })
}

fn audit_path(
    index: &IndexReader,
    repository: &str,
    path: &str,
    occurrences: Vec<(u64, UnitRow)>,
) -> Result<()> {
    let file = index.file(path)?;
    let mut units = Vec::with_capacity(occurrences.len());
    if !occurrences.is_empty() {
        let file = file.as_ref().context("committed unit file missing")?;
        let retained = retrieval_artifact(index, file)?;
        let mut evidence = VectorEvidenceReader::new(&retained)?;
        for (row, unit) in occurrences {
            ensure!(
                unit.path == file.path
                    && unit.source_hash == file.source_hash
                    && unit.payload == file.payload,
                "committed unit binding differs from indexed file"
            );
            let members: Vec<VectorMemberSpan> = serde_json::from_slice(&unit.members)?;
            let citation = evidence.materialize(&members)?.citation;
            units.push(AuditUnit {
                row,
                ordinal: unit.ordinal,
                members,
                citation,
            });
        }
    }
    let receipt = AuditPath {
        repository,
        path,
        source_present: file.is_some(),
        units,
    };
    println!(
        "AIRK_COMMITTED_VECTOR_CITATIONS {}",
        serde_json::to_string(&receipt)?
    );
    Ok(())
}

fn audit_collection(root: &Path, collection: &AuditCollection) -> Result<()> {
    let index = open_snapshot(root, collection)?;
    let mut by_path: BTreeMap<&str, Vec<_>> = collection
        .paths
        .iter()
        .map(|path| (path.as_str(), Vec::new()))
        .collect();
    ensure!(
        !collection.paths.is_empty()
            && by_path.len() == collection.paths.len()
            && by_path.len() <= MAX_PATHS,
        "invalid audit path roster size or duplicate path"
    );
    for path in by_path.keys() {
        super::validate_path(path)?;
    }
    let paths = serde_json::to_string(&collection.paths)?;
    let mut occurrences = 0;
    for row in index.live_rows_for_paths(Some(&paths), None, collection.committed_rows)? {
        let page = index.scoped_units_for_row(row, Some(&paths), None, MAX_PAGE)?;
        occurrences += page.len();
        ensure!(
            page.len() < MAX_PAGE && occurrences <= MAX_OCCURRENCES,
            "audit occurrence bound exceeded"
        );
        for unit in page {
            by_path
                .get_mut(unit.path.as_str())
                .context("committed unit outside requested paths")?
                .push((row, unit));
        }
    }
    for (path, units) in by_path {
        audit_path(&index, &collection.repository, path, units)
            .with_context(|| format!("{}: {path}", collection.repository))?;
    }
    Ok(())
}

#[test]
#[ignore = "requires pinned detached Search evaluation databases"]
fn committed_vector_coverage_audit() -> Result<()> {
    let input_path = PathBuf::from(
        env::var_os("AIRK_SEARCH_VECTOR_AUDIT_INPUT").context("missing audit input path")?,
    )
    .canonicalize()?;
    let expected = env::var("AIRK_SEARCH_VECTOR_AUDIT_SHA256")?.parse::<Sha256Digest>()?;
    let mut bytes = Vec::new();
    File::open(&input_path)?
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_INPUT_BYTES
            && Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == expected,
        "audit input length or SHA-256 differs"
    );
    let input: AuditInput = serde_json::from_slice(&bytes)?;
    let repositories: BTreeSet<_> = input
        .collections
        .iter()
        .map(|collection| collection.repository.as_str())
        .collect();
    ensure!(
        input.collections.len() == 2
            && repositories == BTreeSet::from(["bookbuilder-v2", "munarium"]),
        "audit requires each of the two pinned repositories once"
    );
    let root = input_path.parent().context("audit input has no parent")?;
    for collection in &input.collections {
        audit_collection(root, collection).with_context(|| collection.repository.clone())?;
    }
    Ok(())
}
