// SPDX-License-Identifier: Apache-2.0
//! Private TRAIN-only actual native semantic-unit projection; zero forwards.
use super::*;
use airk_retrieval::vector_units::visit_vector_units;

const CORPUS_PIN: &str = "6e9d8ade6f799ed2a5e1903de69482db9fcd3cb1731a5fee4a504adede5fca6f";
const REQUESTS_PIN: &str = "916b398de0fc9774041fe174d0e585c862bc68e9aa18fa04028c3126f693a18f";
const PARENTS_PIN: &str = "1c0f106de7372b33a8b355aa5087fa466d830a5c4c6e00e14fedf5781f0659ac";
const NEGATIVES_PIN: &str = "314825dccad822874d3405305df789317c8e90a0a0e4cdcdb590acd27a555c48";

// Each marker is flushed before entering the owning boundary, including hangs.
fn phase<T>(name: &str, work: impl FnOnce() -> Result<T>) -> Result<T> {
    use std::io::Write;
    let started = std::time::Instant::now();
    eprintln!("TRAIN_UNIT_PHASE before {name}");
    std::io::stderr().flush()?;
    let result = work();
    eprintln!(
        "TRAIN_UNIT_PHASE after {name} elapsed_ns={} ok={}",
        started.elapsed().as_nanos(),
        result.is_ok()
    );
    std::io::stderr().flush()?;
    result
}

fn lexical_setup(
    control: &std::path::Path,
    source: &std::path::Path,
    data: PathBuf,
) -> Result<(
    NativeInferenceOwner,
    crate::Kernel,
    airk_ingest::IndexReport,
)> {
    let lease = phase("service_lease", || {
        Ok(Arc::new(airk_store::service::ServiceLease::acquire(
            control,
        )?))
    })?;
    let owner = phase("inference_owner_admit", || {
        NativeInferenceOwner::admit(lease)
    })?;
    let kernel = phase("kernel_open", || crate::Kernel::open(source, Some(data)))?;
    phase("configure_vectors_false", || {
        kernel.configure_vectors(false)
    })?;
    let report = phase("lexical_index", || kernel.index())?;
    Ok((owner, kernel, report))
}

#[test]
fn native_train_tiny_lexical_setup_uses_same_owner_flow() -> Result<()> {
    let started = std::time::Instant::now();
    let fixture = tempfile::tempdir()?;
    let source = fixture.path().join("sources");
    std::fs::create_dir(&source)?;
    std::fs::write(
        source.join("tiny.txt"),
        "A tiny native ownership fixture.\n",
    )?;
    let control = tempfile::tempdir()?;
    let (_owner, kernel, report) =
        lexical_setup(control.path(), &source, fixture.path().join("data"))?;
    assert_eq!(report.embedding_actual_forwards, 0);
    assert_eq!(report.embedding_inferences, 0);
    assert_eq!(
        kernel
            .load_optional()?
            .context("missing tiny publication")?
            .artifacts
            .len(),
        1
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "tiny owner setup exceeded five seconds"
    );
    Ok(())
}

// Exact canonical scanner, without this caller retaining/publishing its output.
fn unpublished_extractions(
    kernel: &crate::Kernel,
) -> Result<(Vec<airk_core::RetrievalArtifact>, airk_ingest::IndexReport)> {
    std::fs::create_dir_all(kernel.data_dir())?;
    let previous = kernel.empty_snapshot();
    let policy = kernel.rebuild_extraction_policy()?.policy;
    let roots = kernel.configured_roots()?;
    let source_policy = kernel.source_policy_for_work()?;
    let mut captured = Vec::new();
    let (_, report, _) = airk_ingest::scan_payloads_with_optional_control_snapshot(
        kernel.root(),
        kernel.data_dir(),
        &previous,
        &roots,
        kernel.scan_limits()?,
        &policy,
        true,
        airk_ingest::ExtractionExecution::default(),
        None,
        source_policy.as_deref(),
        None,
        |_| Ok(None),
        |profile, artifact, bytes, payload| {
            let payload = payload.context("fresh unpublished extraction missing payload")?;
            ensure!(
                artifact.retained.is_none(),
                "unpublished extraction unexpectedly retained"
            );
            ensure!(
                artifact.content_hash == format!("{:x}", Sha256::digest(bytes))
                    && artifact.bytes == bytes.len(),
                "captured source binding differs"
            );
            let digest =
                Sha256Digest::from_bytes(Sha256::digest(serde_json::to_vec(payload)?).into());
            captured.push(airk_core::RetrievalArtifact {
                artifact: artifact.clone(),
                profile: profile.clone(),
                payload: payload.clone(),
                payload_digest: digest,
            });
            Ok(artifact.clone())
        },
    )?;
    Ok((captured, report))
}

// Native ONNX returns no vector for a bounded blank input before tokenization/FFI.
// Planned units are already <=MAX_INPUT_BYTES, so the adapter's prefix is exact.
fn unit_tensors(model: &EmbeddingModel, text: &str) -> Result<Option<Value>> {
    ensure!(
        text.len() <= airk_retrieval::model::MAX_INPUT_BYTES,
        "native unit exceeds adapter input bound"
    );
    if text.trim().is_empty() {
        ensure!(
            model.embed_text(text)?.is_none(),
            "native blank input unexpectedly produced a vector"
        );
        Ok(None)
    } else {
        model.evaluation_inputs(text).map(Some)
    }
}

#[test]
#[ignore = "actual original native ONNX blank/no-forward admission under five seconds"]
fn native_blank_unit_has_no_invented_tensors_or_vector() -> Result<()> {
    let started = std::time::Instant::now();
    let control = tempfile::tempdir()?;
    let owner = NativeInferenceOwner::admit_private_fixture(control.path())?;
    let input = inputs()?
        .into_iter()
        .find(|row| row.family == "original-mixed-u8-ort130-evaluation")
        .context("missing exact blank-control encoder")?;
    owner.run_encoder_evaluation(input.model_json_sha256, || -> Result<()> {
        let model = EmbeddingModel::open(&input.directory)?;
        for text in ["", " \t\r\n", "\u{2003}\r\n"] {
            assert!(model.evaluation_inputs(text).is_err());
            assert!(unit_tensors(&model, text)?.is_none());
        }
        assert!(unit_tensors(&model, "Actual nonblank input")?.is_some());
        Ok(())
    })??;
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    Ok(())
}

fn extraction_publication_parity(name: &str, bytes: &[u8]) -> Result<()> {
    let started = std::time::Instant::now();
    let fixture = tempfile::tempdir()?;
    let source = fixture.path().join("sources");
    std::fs::create_dir(&source)?;
    std::fs::write(source.join(name), bytes)?;
    let control = tempfile::tempdir()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(control.path())?);
    let owner = NativeInferenceOwner::admit(lease)?;
    let kernel = crate::Kernel::open(&source, Some(fixture.path().join("data")))?;
    kernel.configure_vectors(false)?;
    let (extracted, report) = phase("unpublished_extraction", || {
        unpublished_extractions(&kernel)
    })?;
    assert_eq!(report.embedding_actual_forwards, 0);
    assert_eq!(extracted.len(), 1);
    phase("parity_kernel_index", || kernel.index())?;
    let snapshot = kernel
        .load_optional()?
        .context("missing parity publication")?;
    assert_eq!(snapshot.artifacts.len(), 1);
    let retained = kernel.store.load_retrieval_artifact(
        snapshot
            .artifacts
            .get(name)
            .context("missing exact parity source")?,
    )?;
    let unpublished = &extracted[0];
    let mut published_artifact = retained.artifact.clone();
    published_artifact.retained = None;
    assert_eq!(
        serde_json::to_value(&published_artifact)?,
        serde_json::to_value(&unpublished.artifact)?
    );
    assert_eq!(
        serde_json::to_vec(&retained.payload)?,
        serde_json::to_vec(&unpublished.payload)?
    );
    assert_eq!(
        serde_json::to_vec(&retained.profile)?,
        serde_json::to_vec(&unpublished.profile)?
    );
    assert_eq!(retained.payload_digest, unpublished.payload_digest);
    let input = inputs()?
        .into_iter()
        .find(|row| row.family == "original-mixed-u8-ort130-evaluation")
        .context("missing exact parity encoder")?;
    owner.run_encoder_evaluation(input.model_json_sha256,||->Result<()> {
        let model = EmbeddingModel::open(&input.directory)?;
        let project = |payload: &airk_core::extraction::ExtractionPayload| -> Result<Vec<Value>> {
            let mut units=Vec::new();
            visit_vector_units(payload,&model,|unit| {
                units.push(json!({"input":unit.input,"members":unit.members,"tensors":unit_tensors(&model,&unit.input)?}));
                Ok(())
            })?;
            Ok(units)
        };
        let actual=project(&unpublished.payload)?;
        assert!(actual.len()>1,"long fixture must exercise native unit splitting");
        assert_eq!(actual,project(&retained.payload)?);
        Ok(())
    })??;
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "single-file parity exceeded five seconds"
    );
    Ok(())
}

#[test]
#[ignore = "actual original encoder assets; zero-forward published/unpublished parity under five seconds"]
fn native_unpublished_txt_matches_kernel_payload_and_planner() -> Result<()> {
    let text = format!(
        "Heading: original locations\r\n\r\n{}",
        "Unicode caf\u{e9} \u{6771}\u{4eac} sentence preserving original locations.\r\n".repeat(80)
    );
    extraction_publication_parity("tiny.txt", text.as_bytes())
}
#[test]
#[ignore = "actual original encoder assets; zero-forward published/unpublished parity under five seconds"]
fn native_unpublished_python_matches_kernel_payload_and_planner() -> Result<()> {
    let text = format!(
        "def preserved(value):\r\n    \"\"\"{}\"\"\"\r\n    return value\r\n",
        "Unicode caf\u{e9} \u{6771}\u{4eac} original function contract. ".repeat(80)
    );
    extraction_publication_parity("tiny.py", text.as_bytes())
}

fn verify_original_parent_span(bytes: &[u8], start: usize, end: usize, digest: &str) -> Result<()> {
    ensure!(
        start <= end && end <= bytes.len(),
        "original parent source span out of bounds"
    );
    ensure!(
        format!("{:x}", Sha256::digest(&bytes[start..end])) == digest,
        "original parent source span hash differs"
    );
    Ok(())
}

#[test]
fn native_original_empty_parent_requires_exact_hash_and_has_no_coverage() -> Result<()> {
    let empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    verify_original_parent_span(b"abc", 1, 1, empty)?;
    verify_original_parent_span(b"abc", 0, 3, abc)?;
    assert!(verify_original_parent_span(b"abc", 1, 1, abc).is_err());
    assert!(verify_original_parent_span(b"abc", 1, 1, "").is_err());
    assert!(verify_original_parent_span(b"abc", 3, 4, empty).is_err());
    assert!(verify_original_parent_span(b"abc", 2, 1, empty).is_err());
    use airk_core::extraction::LocalSpan;
    assert!(!covered(
        &LocalSpan { start: 1, end: 1 },
        &[LocalSpan { start: 0, end: 3 }]
    ));
    assert!(!covered(
        &LocalSpan { start: 2, end: 1 },
        &[LocalSpan { start: 0, end: 3 }]
    ));
    assert!(!covered(
        &LocalSpan { start: 0, end: 3 },
        &[
            LocalSpan { start: 0, end: 1 },
            LocalSpan { start: 2, end: 3 }
        ]
    ));
    Ok(())
}

fn covered(
    parent: &airk_core::extraction::LocalSpan,
    spans: &[airk_core::extraction::LocalSpan],
) -> bool {
    if parent.start >= parent.end {
        return false;
    }
    let mut intersections: Vec<_> = spans
        .iter()
        .filter_map(|span| {
            let start = parent.start.max(span.start);
            let end = parent.end.min(span.end);
            (start < end).then_some((start, end))
        })
        .collect();
    intersections.sort_unstable();
    let mut cursor = parent.start;
    for (start, end) in intersections {
        if start > cursor {
            return false;
        }
        cursor = cursor.max(end);
    }
    cursor >= parent.end
}

fn source_census(
    base: &std::path::Path,
    path: &std::path::Path,
    output: &mut BTreeSet<String>,
) -> Result<()> {
    ensure!(
        !path.components().any(|part| matches!(
            part,
            std::path::Component::CurDir | std::path::Component::ParentDir
        )),
        "TRAIN source traversal components refused"
    );
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        path.canonicalize()? == path,
        "TRAIN source lexical/ancestor alias refused"
    );
    ensure!(
        !metadata.file_type().is_symlink(),
        "TRAIN source alias refused"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "TRAIN source reparse refused"
        );
    }
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path)? {
            source_census(base, &entry?.path(), output)?;
        }
    } else {
        ensure!(
            metadata.is_file() && output.len() < 10_000,
            "TRAIN source special-file/census bound exceeded"
        );
        output.insert(
            path.strip_prefix(base)?
                .to_str()
                .context("TRAIN source path notUTF8")?
                .replace('\\', "/"),
        );
    }
    Ok(())
}

#[test]
fn native_parent_coverage_requires_exact_union_without_cropping_units() {
    use airk_core::extraction::LocalSpan;
    let parent = LocalSpan { start: 10, end: 30 };
    assert!(covered(
        &parent,
        &[
            LocalSpan { start: 0, end: 20 },
            LocalSpan { start: 20, end: 40 }
        ]
    ));
    assert!(!covered(
        &parent,
        &[
            LocalSpan { start: 0, end: 19 },
            LocalSpan { start: 20, end: 40 }
        ]
    ));
    assert!(!covered(&parent, &[LocalSpan { start: 10, end: 29 }]));
    assert!(!covered(&parent, &[]));
}

#[test]
fn native_train_source_census_detects_extra_missing_and_alias_paths() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let base = temp.path().canonicalize()?;
    let sources = base.join("sources");
    std::fs::create_dir(&sources)?;
    std::fs::write(sources.join("a.txt"), b"source")?;
    let expected = BTreeSet::from(["sources/a.txt".to_string()]);
    let mut before = BTreeSet::new();
    source_census(&base, &sources, &mut before)?;
    assert_eq!(before, expected);
    std::fs::write(sources.join("extra.txt"), b"foreign")?;
    let mut extra = BTreeSet::new();
    source_census(&base, &sources, &mut extra)?;
    assert_ne!(extra, expected);
    std::fs::remove_file(sources.join("extra.txt"))?;
    std::fs::remove_file(sources.join("a.txt"))?;
    let mut missing = BTreeSet::new();
    source_census(&base, &sources, &mut missing)?;
    assert_ne!(missing, expected);
    // PathBuf::push normalizes dot components for Windows verbatim prefixes.
    // Construct the raw incoming spelling without erasing the refusal stimulus.
    let mut spelling = sources.as_os_str().to_os_string();
    spelling.push(std::path::MAIN_SEPARATOR_STR);
    spelling.push("..");
    spelling.push(std::path::MAIN_SEPARATOR_STR);
    spelling.push("sources");
    let alias = PathBuf::from(spelling);
    assert!(
        alias
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir)),
        "test stimulus must retain an actual parent traversal component"
    );
    assert!(source_census(&base, &alias, &mut BTreeSet::new()).is_err());
    Ok(())
}

#[test]
#[ignore = "reviewed TRAIN source roster only; actual native planner/tensors, zero embedding forwards"]
fn emit_actual_native_train_units_and_parent_coverage() -> Result<()> {
    let manifest_path = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_TRAIN_SOURCE_MANIFEST")
            .context("missing reviewed TRAIN source roster")?,
    );
    let manifest_pin: Sha256Digest = std::env::var("AIRK_NATIVE_TRAIN_SOURCE_MANIFEST_SHA256")
        .context("missing TRAIN source roster digest")?
        .parse()?;
    let manifest_bytes = phase("manifest_seal", || sealed(&manifest_path, manifest_pin))?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes)?;
    ensure!(
        manifest["schema"] == "airk.private-native-TRAIN-source-roster/1"
            && manifest["corpus_sha256"] == CORPUS_PIN
            && manifest["requests_sha256"] == REQUESTS_PIN
            && manifest["parent_rows_sha256"] == PARENTS_PIN
            && manifest["fixed_negative_rows_sha256"] == NEGATIVES_PIN,
        "foreign TRAIN source/admission scope"
    );
    let source = manifest_path
        .parent()
        .context("missing source parent")?
        .canonicalize()?;
    let source_root = source.join("sources").canonicalize()?;
    let files: Vec<SourceRow> = serde_json::from_value(manifest["files"].clone())?;
    ensure!(
        !files.is_empty() && files.len() <= 10_000,
        "bounded nonempty TRAIN file census required"
    );
    let mut expected = BTreeSet::new();
    for file in &files {
        airk_core::index_catalog::relative_path(&file.file, false)?;
        ensure!(
            expected.insert(file.file.clone()),
            "duplicate TRAIN source path"
        );
        let path = source.join(&file.file);
        ensure!(
            path.canonicalize()? == path && path.starts_with(&source_root),
            "TRAIN source alias/escape refused"
        );
        let metadata = std::fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() == file.bytes,
            "TRAIN file metadata differs"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "TRAIN reparse file refused"
            );
        }
        sealed(&path, file.sha256)?;
    }
    let mut observed = BTreeSet::new();
    phase("source_census", || {
        source_census(&source, &source_root, &mut observed)
    })?;
    ensure!(
        observed == expected,
        "unexpected/missing source BEFORE native lexical indexing"
    );
    let data = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_TRAIN_UNIT_DATA")
            .context("missing fresh lexical data root")?,
    );
    let output = PathBuf::from(
        std::env::var_os("AIRK_NATIVE_TRAIN_UNIT_OUTPUT").context("missing fresh unit output")?,
    );
    ensure!(
        data.is_absolute() && !data.try_exists()? && !output.try_exists()?,
        "fresh explicit TRAIN data/output required"
    );
    let data_parent = data
        .parent()
        .context("missing data parent")?
        .canonicalize()?;
    let output_parent = output
        .parent()
        .context("missing output parent")?
        .canonicalize()?;
    ensure!(
        !data_parent.starts_with(&source) && !output_parent.starts_with(&source),
        "metadata writes inside TRAIN source refused"
    );
    let encoder_bytes = sealed(
        &PathBuf::from(
            std::env::var_os("AIRK_NATIVE_ENCODER_INPUT")
                .context("missing closed encoder input")?,
        ),
        "3ead240726ebdba10577db6778bffbf5232eab10c50f2ae128d589e0a961ab81".parse()?,
    )?;
    let input: EncoderInput = serde_json::from_slice::<Vec<EncoderInput>>(&encoder_bytes)?
        .into_iter()
        .find(|row| row.family == "original-mixed-u8-ort130-evaluation")
        .context("missing exact original encoder")?;
    let model_manifest: airk_retrieval::model::OnnxManifest = serde_json::from_slice(&sealed(
        &input.directory.join("model.json"),
        input.model_json_sha256,
    )?)?;
    ensure!(
        model_manifest.tokenizer_sha256
            == "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037"
            && model_manifest.max_tokens == 256,
        "original tokenizer/context differs"
    );
    let control = tempfile::tempdir()?;
    let lease = phase("service_lease", || {
        Ok(Arc::new(airk_store::service::ServiceLease::acquire(
            control.path(),
        )?))
    })?;
    let owner = phase("inference_owner_admit", || {
        NativeInferenceOwner::admit(lease)
    })?;
    let kernel = phase("kernel_open", || {
        crate::Kernel::open(&source_root, Some(data))
    })?;
    let (extracted, extraction_report) = phase("unpublished_native_extraction", || {
        unpublished_extractions(&kernel)
    })?;
    ensure!(
        extraction_report.embedding_actual_forwards == 0
            && extraction_report.embedding_inferences == 0,
        "unpublished extraction unexpectedly performed embedding inference"
    );
    let actual: BTreeSet<_> = extracted
        .iter()
        .map(|entry| format!("sources/{}", entry.artifact.path))
        .collect();
    ensure!(
        actual == expected,
        "native unpublished census differs from reviewed TRAIN roster"
    );
    let parents = manifest["paragraphs"]
        .as_array()
        .context("missing original parent span map")?;
    ensure!(
        !parents.is_empty() && parents.len() <= 10_000,
        "bounded original parent map required"
    );
    let mut parent_ids = BTreeSet::new();
    for parent in parents {
        ensure!(
            parent_ids.insert(parent["id"].as_str().context("missing original parentID")?),
            "duplicate original parentID"
        );
        let name = parent["file"]
            .as_str()
            .context("missing parent source path")?;
        ensure!(
            expected.contains(name),
            "parent outside closed source roster"
        );
        let bytes = std::fs::read(source.join(name))?;
        let start = parent["byte_start"]
            .as_u64()
            .context("missing parent start")? as usize;
        let end = parent["byte_end"].as_u64().context("missing parent end")? as usize;
        verify_original_parent_span(
            &bytes,
            start,
            end,
            parent["paragraph_sha256"]
                .as_str()
                .context("original parent hash missing")?,
        )?;
    }
    let (units, planning_descriptor) = phase("encoder_evaluation_owner", || {
        owner.run_encoder_evaluation(input.model_json_sha256,||->Result<_> {
        let model=phase("embedding_model_open", || EmbeddingModel::open(&input.directory))?;let descriptor=model.cache_descriptor()?;let mut result=Vec::new();
        for retained in &extracted {
            let artifact=&retained.artifact;let path=&artifact.path;
            let original=std::fs::read(source_root.join(path))?;
            ensure!(retained.payload.text.as_bytes()==original,"nonidentity extraction mapping requires owning mapped-source support, not guessed offsets");
            ensure!(retained.payload.chunks.iter().all(|chunk|chunk.region.is_none()),"mapped TRAIN payload not admitted by this exact-source projection");
            visit_vector_units(&retained.payload,&model,|unit| {
                let tensors=unit_tensors(&model,&unit.input)?;
                ensure!(model.planning_tokens(&unit.input)?.len()<=256,"native planner emitted an over-context input");
                let body=unit.members.iter().map(|member|retained.payload.text.get(member.span.start..member.span.end).context("native body member source span invalid")).collect::<Result<Vec<_>>>()?.concat();
                let prefix=unit.input.strip_suffix(&body).context("native input does not preserve exact body suffix")?;
                let context_mapping_ambiguous=!prefix.is_empty();
                let file_name=format!("sources/{path}");
                let admitted_spans:Vec<_>=parents.iter().filter(|parent|parent["file"]==file_name).map(|parent|Ok(airk_core::extraction::LocalSpan{start:parent["byte_start"].as_u64().context("missing parent start")? as usize,end:parent["byte_end"].as_u64().context("missing parent end")? as usize})).collect::<Result<_>>()?;
                // QASPER source joins use the predeclared twoLF template. For
                // original modules, unrelated non-parent functions are not a
                // silent extension of the fixed training candidate pool.
                let body_in_pool=path.starts_with("qasper/") || unit.members.iter().all(|member|covered(&member.span,&admitted_spans));
                let member_bytes=serde_json::to_vec(&unit.members)?;
                let input_sha=Sha256Digest::from_bytes(Sha256::digest(unit.input.as_bytes()).into());
                let embedding_key=airk_core::embedding::embedding_key(&descriptor,unit.input.as_bytes())?;
                let unit_id=airk_core::identity::framed_digest(b"private-native-training-unit/1",&[path.as_bytes(),artifact.content_hash.as_bytes(),input_sha.as_bytes(),&member_bytes]);
                result.push(json!({"id":unit_id,"file":file_name,"source_sha256":artifact.content_hash,"input":unit.input,"embedding_input_sha256":input_sha,"embedding_key":embedding_key,"members":unit.members,"embedding_admission":if tensors.is_some(){"embeddable"}else{"not_embeddable_blank"},"native_input_tensors":tensors,"context_prefix_bytes":prefix.len(),"context_parent_mapping_ambiguous":context_mapping_ambiguous,"body_within_admitted_training_source":body_in_pool,"payload_sha256":retained.payload_digest,"extraction_profile_sha256":retained.profile.identity()?,"published":false}));
                Ok(())
            })?;
        }
        Ok((result,descriptor))
    })
    })??;
    let mut coverage = Vec::new();
    for parent in parents {
        ensure!(
            parent["dataset"] == "QASPER" || parent["dataset"] == "requests-original-docstrings",
            "non-TRAIN source family refused"
        );
        let start = parent["byte_start"]
            .as_u64()
            .context("parent start missing")? as usize;
        let end = parent["byte_end"].as_u64().context("parent end missing")? as usize;
        if start == end {
            coverage.push(json!({"parent":parent,"admission":"EMPTY_ORIGINAL_NOT_EVALUABLE","unit_ids":[],"single_unit_complete_annotation":false,"union_complete_annotation":false,"outside_parent_body_bytes":0,"context_prefix_bytes":0,"unambiguous_admitted_unit_ids":[],"unambiguous_union_complete_annotation":false,"not_embeddable_unit_ids":[],"fragment_positive_labels":false}));
            continue;
        }
        ensure!(start < end, "reversed parent mapping");
        let span = airk_core::extraction::LocalSpan { start, end };
        let mut all = Vec::new();
        let mut bag = Vec::new();
        let mut single = false;
        let mut outside_parent = 0usize;
        let mut context_bytes = 0usize;
        let mut unambiguous = Vec::new();
        let mut admitted_bag = Vec::new();
        let mut not_embeddable_bag = Vec::new();
        for unit in &units {
            if unit["file"] != parent["file"] {
                continue;
            }
            let members: Vec<airk_core::vector_evidence::VectorMemberSpan> =
                serde_json::from_value(unit["members"].clone())?;
            let spans: Vec<_> = members.iter().map(|m| m.span.clone()).collect();
            if !spans.iter().any(|s| s.start < end && s.end > start) {
                continue;
            }
            single |= covered(&span, &spans);
            outside_parent += spans
                .iter()
                .map(|s| (s.end - s.start) - (s.end.min(end).saturating_sub(s.start.max(start))))
                .sum::<usize>();
            all.extend(spans);
            bag.push(unit["id"].clone());
            if unit["embedding_admission"] == "not_embeddable_blank" {
                not_embeddable_bag.push(unit["id"].clone());
            }
            context_bytes += unit["context_prefix_bytes"]
                .as_u64()
                .context("missing context byte observation")? as usize;
            if unit["context_parent_mapping_ambiguous"] == false
                && unit["body_within_admitted_training_source"] == true
                && unit["embedding_admission"] == "embeddable"
            {
                unambiguous.extend(members.into_iter().map(|member| member.span));
                admitted_bag.push(unit["id"].clone());
            }
        }
        coverage.push(json!({"parent":parent,"admission":"NONEMPTY_ORIGINAL_PARENT","unit_ids":bag,"single_unit_complete_annotation":single,"union_complete_annotation":covered(&span,&all),"outside_parent_body_bytes":outside_parent,"context_prefix_bytes":context_bytes,"unambiguous_admitted_unit_ids":admitted_bag,"unambiguous_union_complete_annotation":covered(&span,&unambiguous),"not_embeddable_unit_ids":not_embeddable_bag,"fragment_positive_labels":false}));
    }
    for file in &files {
        sealed(&source.join(&file.file), file.sha256)?;
    }
    let mut after = BTreeSet::new();
    source_census(&source, &source_root, &mut after)?;
    ensure!(
        after == expected,
        "TRAIN source census changed after native planning"
    );
    ensure!(
        sealed(&manifest_path, manifest_pin)? == manifest_bytes,
        "TRAIN roster changed"
    );
    std::fs::write(
        output,
        serde_json::to_vec_pretty(
            &json!({"schema":"airk.private-native-training-units/1","source_manifest_sha256":manifest_pin,"encoder_input_sha256":format!("{:x}",Sha256::digest(encoder_bytes)),"model_json_sha256":input.model_json_sha256,"tokenizer_sha256":model_manifest.tokenizer_sha256,"planning_pipeline_digest":planning_descriptor.identity()?,"planning_descriptor":planning_descriptor,"native_binary_sha256":asset_digest()?,"unit_specification":airk_retrieval::vector_units::VECTOR_UNIT_SPECIFICATION,"empty_original_parent_count":coverage.iter().filter(|row|row["admission"]=="EMPTY_ORIGINAL_NOT_EVALUABLE").count(),"not_embeddable_unit_count":units.iter().filter(|unit|unit["embedding_admission"]=="not_embeddable_blank").count(),"units":units,"parents":coverage,"extraction_report":extraction_report,"published":false,"preparation":"canonical-unpublished-native-extraction","embedding_forwards":0,"training_gradients":false,"complete":true}),
        )?,
    )?;
    Ok(())
}
fn asset_digest() -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(std::fs::read(std::env::current_exe()?)?)
    ))
}
