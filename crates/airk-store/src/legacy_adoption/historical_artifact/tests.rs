// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    ArtifactRef,
    extraction::{ExtractionProfile, LocalRegion, LocalSpan},
    location::{ByteSpan, MapSegment, Mapping, RegionLocation, SourceMap},
};
use std::{collections::BTreeMap, fs};
// Serialized historical shape: deliberately has no semantic_projection field.
const OLD: &str = r#"{"contract_version":1,"extraction_method":"utf8","parse_status":"plain","provenance":[],"text":"hello","facts":[],"syntax_identities":[],"chunks":[{"span":{"start":0,"end":5},"start_line":1,"end_line":1,"location":{"Lines":{"start":1,"end":1}},"region":null}],"regions":[],"relationships":[],"continuity":[],"diagnostics":[],"partial":false}"#;
fn profile() -> ExtractionProfile {
    ExtractionProfile {
        contract_version: 1,
        adapter_id: "text".into(),
        adapter_version: "native-facts/7".into(),
        language: "text".into(),
        dialect: None,
        grammar_identity: None,
        mapping_version: 1,
        chunk_version: 1,
        options: BTreeMap::new(),
        configuration_inputs: BTreeMap::new(),
    }
}
fn entry(bytes: &[u8], profile: ExtractionProfile) -> Artifact {
    Artifact {
        path: "original.txt".into(),
        content_hash: format!("{:x}", Sha256::digest(b"hello")),
        bytes: 5,
        chunks: vec![],
        language: "text".into(),
        extraction_method: "utf8".into(),
        parse_status: "plain".into(),
        facts: vec![],
        retained: Some(ArtifactRef {
            legacy_origin: None,
            source_encoding: None,
            payload_encoding: None,
            object: format!("{:x}", Sha256::digest(bytes)),
            object_bytes: bytes.len(),
            raw_source: true,
            chunks: 1,
            extracted_bytes: 5,
            facts: 0,
            profile: Some(profile),
        }),
    }
}
#[test]
fn legacy_adoption_old_payload_preserves_evidence_without_semantic_projection() -> Result<()> {
    let bytes = OLD.as_bytes();
    assert!(serde_json::from_slice::<airk_core::extraction::ExtractionPayload>(bytes).is_err());
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    fs::create_dir_all(root.join("objects/artifacts"))?;
    fs::create_dir_all(root.join("objects/sources"))?;
    let entry = entry(bytes, profile());
    let object = root
        .join("objects/artifacts")
        .join(&entry.retained.as_ref().unwrap().object);
    fs::write(&object, bytes)?;
    fs::write(
        root.join("objects/sources").join(&entry.content_hash),
        b"hello",
    )?;
    let pins = PinnedRoot::open(&root)?;
    let saved = load(&FileStore::new(&root), &pins, &entry, 65536)?;
    assert_eq!(saved.artifact.chunks[0].text, "hello");
    assert_eq!(saved.artifact.path, "original.txt");
    assert_eq!(saved.raw_source.as_deref(), Some(b"hello".as_slice()));
    assert_eq!(
        saved.profile.as_ref().unwrap().identity()?,
        profile().identity()?
    );
    assert!(saved.payload.is_none());
    assert!(load(&FileStore::new(&root), &pins, &entry, 1).is_err());
    let mut crossed = entry.clone();
    crossed.retained.as_mut().unwrap().chunks = 2;
    assert!(load(&FileStore::new(&root), &pins, &crossed, 65536).is_err());
    fs::write(&object, b"changed")?;
    assert!(load(&FileStore::new(&root), &pins, &entry, 65536).is_err());
    Ok(())
}
#[test]
fn legacy_adoption_old_mapped_payload_preserves_original_host_coordinates() -> Result<()> {
    let mut payload = payload::LegacyPayload::from_bytes(OLD.as_bytes())?;
    payload.contract_version = 2;
    let mut profile = profile();
    profile.contract_version = 2;
    payload.regions = vec![LocalRegion {
        id: 1,
        parent: None,
        profile: profile.clone(),
        text: "hello".into(),
        source_map: SourceMap {
            version: 1,
            segments: vec![MapSegment {
                local: ByteSpan { start: 0, end: 5 },
                mapping: Mapping::Verbatim {
                    host: ByteSpan { start: 0, end: 5 },
                },
            }],
        },
        location: RegionLocation::Embedded {
            region_id: "original-region".into(),
            local_span: ByteSpan { start: 0, end: 5 },
            original_spans: vec![ByteSpan { start: 0, end: 5 }],
        },
    }];
    payload.chunks[0].region = Some(1);
    payload.chunks[0].span = LocalSpan { start: 0, end: 5 };
    let bytes = serde_json::to_vec(&payload)?;
    let entry = entry(&bytes, profile);
    let artifact = decode(&bytes, &entry)?;
    assert_eq!(artifact.chunks[0].text, "hello");
    assert!(matches!(
        artifact.chunks[0].location,
        airk_core::DocumentLocation::Mapped { .. }
    ));
    payload.chunks[0].span.end = 6;
    assert!(decode(&serde_json::to_vec(&payload)?, &entry).is_err());
    payload.chunks[0].span.end = 5;
    payload.regions[0].source_map.segments[0].mapping = Mapping::Verbatim {
        host: ByteSpan { start: 0, end: 6 },
    };
    assert!(decode(&serde_json::to_vec(&payload)?, &entry).is_err());
    Ok(())
}
