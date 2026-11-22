// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};
const HF: &str = "https://huggingface.co/Qwen/Qwen3-0.6B-Base/resolve/da87bfb608c14b7cf20ba1ce41287e8de496c0cd/LICENSE";
const HF_BACKUP: &str = "https://huggingface.co/Qwen/Qwen3-1.7B/resolve/70d244cc86ccca08cf5af4e1e306ecf908b1ad5e/LICENSE";
const AIRK_HF: &str = "https://huggingface.co/iokaio/airk-arbiter-models/resolve/0123456789abcdef0123456789abcdef01234567/airk-arbiter-focus.fixture.bin";
const AIRK_GITHUB: &str =
    "https://github.com/iokaio/airk/releases/download/v1.9.0/airk-arbiter-focus.fixture.bin";
fn part() -> AssetPart {
    AssetPart {
        url: HF.into(),
        backup_urls: vec![],
        bytes: 11_343,
        sha256: "832dd9e00a68dd83b3c3fb9f5588dad7dcf337a0db50f7d9483f310cd292e92e"
            .parse()
            .unwrap(),
    }
}
/// SHA-256 of the retained fixture in its authenticated CRLF form. The
/// pinned digest was taken from a CRLF working copy; the committed blob is
/// LF (`.gitattributes` `eol=lf`), so the bytes are canonicalized to CRLF
/// before hashing and the check no longer depends on how the file was
/// checked out. The pin itself is unchanged.
fn retained_crlf_sha256(raw: &[u8]) -> String {
    let mut crlf = Vec::with_capacity(raw.len() + raw.len() / 16);
    for (index, byte) in raw.iter().enumerate() {
        if *byte == b'\r' && raw.get(index + 1) == Some(&b'\n') {
            continue;
        }
        if *byte == b'\n' {
            crlf.push(b'\r');
        }
        crlf.push(*byte);
    }
    format!("{:x}", Sha256::digest(crlf))
}
fn profile() -> Result<Profile> {
    Ok(serde_json::from_slice(include_bytes!(
        "../../../../../fixtures/arbiter/profile-downloads.json"
    ))?)
}
#[test]
fn empty_backups_preserve_exact_retained_part_and_profile_identity() -> Result<()> {
    let original = br#"{"url":"https://github.com/iokaio/airk/releases/download/v1.9.0/model.bin","bytes":17,"sha256":"1111111111111111111111111111111111111111111111111111111111111111"}"#;
    let value: AssetPart = serde_json::from_slice(original)?;
    assert!(value.backup_urls.is_empty());
    assert_eq!(serde_json::to_vec(&value)?, original);
    let mut explicit: serde_json::Value = serde_json::from_slice(original)?;
    explicit["backup_urls"] = json!([]);
    assert_eq!(
        serde_json::to_vec(&serde_json::from_value::<AssetPart>(explicit)?)?,
        original
    );
    let raw = include_bytes!("../../../../../fixtures/arbiter/profile-downloads.json");
    assert_eq!(
        retained_crlf_sha256(raw),
        "d0064974be46e7cb56099a71c248587d81cd93eccd1405ec9e736664c199d593"
    );
    let original = profile()?;
    original.validate_release_downloads()?;
    let mut explicit = serde_json::to_value(&original)?;
    for download in explicit["downloads"].as_array_mut().unwrap() {
        for part in download["parts"].as_array_mut().unwrap() {
            part["backup_urls"] = json!([]);
        }
    }
    let decoded: Profile = serde_json::from_value(explicit)?;
    assert_eq!(
        serde_json::to_vec(&decoded)?,
        serde_json::to_vec(&original)?
    );
    assert_eq!(decoded.manifest, original.manifest);
    Ok(())
}
#[test]
fn immutable_reviewed_download_origins_are_closed() -> Result<()> {
    for (repo, revision, file) in [
        (
            "Qwen/Qwen3-Reranker-0.6B",
            "e61197ed45024b0ed8a2d74b80b4d909f1255473",
            "tokenizer.json",
        ),
        (
            "Qwen/Qwen3-0.6B-Base",
            "da87bfb608c14b7cf20ba1ce41287e8de496c0cd",
            "LICENSE",
        ),
        (
            "Qwen/Qwen3-1.7B",
            "70d244cc86ccca08cf5af4e1e306ecf908b1ad5e",
            "tokenizer.json",
        ),
        (
            "Qwen/Qwen3-4B",
            "1cfa9a7208912126459214e8b04321603b3df60c",
            "tokenizer.json",
        ),
        (
            "onnx-community/Qwen3-Reranker-0.6B-ONNX",
            "9995c50e2310679108a55f5ccd16ba8be9f17c20",
            "tokenizer.json",
        ),
    ] {
        assert_eq!(
            download_origin(&format!(
                "https://huggingface.co/{repo}/resolve/{revision}/{file}"
            ))?,
            DownloadOrigin::HuggingFace
        );
    }
    assert_eq!(
        download_origin("https://github.com/iokaio/airk/releases/download/v1.9.0/part01")?,
        DownloadOrigin::GitHubRelease
    );
    for url in [
        HF.replace("https:", "http:"),
        HF.replace("huggingface.co/", "user@huggingface.co/"),
        HF.replace("huggingface.co/", "huggingface.co:443/"),
        HF.replace("huggingface.co/", "huggingface.co.evil/"),
        HF.replace("Qwen/Qwen3-0.6B-Base", "unknown/repo"),
        HF.replace("Qwen/Qwen3-0.6B-Base", "iokaio/airk-models"),
        HF.replace("da87bfb608c14b7cf20ba1ce41287e8de496c0cd", "main"),
        HF.replace(
            "da87bfb608c14b7cf20ba1ce41287e8de496c0cd",
            "DA87BFB608C14B7CF20BA1CE41287E8DE496C0CD",
        ),
        HF.replace("/LICENSE", "/../LICENSE"),
        HF.replace("/LICENSE", "/%2e%2e"),
        HF.replace("/LICENSE", "/."),
        HF.replace("/LICENSE", "/nested/LICENSE"),
        format!("{HF}?token=secret"),
        format!("{HF}#fragment"),
    ] {
        assert!(download_origin(&url).is_err(), "{url}");
    }
    Ok(())
}
#[test]
fn airk_hub_origin_requires_exact_repository_and_a_flat_commit_pinned_file() -> Result<()> {
    assert_eq!(download_origin(AIRK_HF)?, DownloadOrigin::HuggingFace);
    let (base, _) = AIRK_HF.rsplit_once('/').unwrap();
    assert_eq!(
        download_origin(&format!("{base}/{}", "a".repeat(192)))?,
        DownloadOrigin::HuggingFace
    );
    const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
    for url in [
        AIRK_HF.replace("/iokaio/", "/someone-else/"),
        AIRK_HF.replace("/iokaio/", "/Iokaio/"),
        AIRK_HF.replace("/iokaio/", "/datasets/iokaio/"),
        AIRK_HF.replace("/airk-arbiter-models/", "/airk-models/"),
        AIRK_HF.replace("/airk-arbiter-models/", "/airk-arbiter-models.git/"),
        AIRK_HF.replace("/resolve/", "/blob/"),
        AIRK_HF.replace(REVISION, "main"),
        AIRK_HF.replace(REVISION, &REVISION[..39]),
        AIRK_HF.replace(REVISION, &format!("{REVISION}0")),
        AIRK_HF.replace(REVISION, &REVISION.to_ascii_uppercase()),
        format!("{base}/nested/part-01.bin"),
        format!("{base}/%2e%2e"),
        format!("{base}/{}", "a".repeat(193)),
        format!("{AIRK_HF}?download=true"),
        format!("{AIRK_HF}#file"),
    ] {
        assert!(download_origin(&url).is_err(), "{url}");
    }
    Ok(())
}

#[test]
fn complete_backup_ordinals_share_exact_part_identity_and_asset_bounds() -> Result<()> {
    let mut value = part();
    value.backup_urls = vec![HF_BACKUP.into()];
    value.validate()?;
    assert_eq!(value.source_url(0), Some(HF));
    assert_eq!(value.source_url(1), Some(HF_BACKUP));
    assert_eq!(value.source_url(2), None);
    let mut duplicate = value.clone();
    duplicate.backup_urls.push(HF.into());
    assert!(duplicate.validate().is_err());
    duplicate.backup_urls = vec![HF_BACKUP.into(), HF_BACKUP.into()];
    assert!(duplicate.validate().is_err());
    duplicate.backup_urls = vec![HF_BACKUP.into(); 3];
    assert!(duplicate.validate().is_err());
    duplicate = value.clone();
    duplicate.bytes = 0;
    assert!(duplicate.validate().is_err());
    duplicate.bytes = (1_u64 << 30) + 1;
    assert!(duplicate.validate().is_err());
    let mut profile = profile()?;
    for download in &mut profile.downloads {
        for part in &mut download.parts {
            let filename = part.url.rsplit('/').next().unwrap();
            part.backup_urls = vec![format!(
                "https://huggingface.co/Qwen/Qwen3-1.7B/resolve/70d244cc86ccca08cf5af4e1e306ecf908b1ad5e/{filename}"
            )];
        }
    }
    profile.validate_release_downloads()?;
    let mut three = profile.clone();
    for download in &mut three.downloads {
        for part in &mut download.parts {
            let filename = part.url.rsplit('/').next().unwrap();
            part.backup_urls.push(format!("https://huggingface.co/Qwen/Qwen3-4B/resolve/1cfa9a7208912126459214e8b04321603b3df60c/{filename}"));
        }
        assert_eq!(download.source_count()?, 3);
    }
    three.validate_release_downloads()?;
    let last = profile.downloads.len() - 1;
    assert_eq!(profile.downloads[last].source_count()?, 2);
    let mut incomplete = profile.clone();
    incomplete.downloads[last].parts[1].backup_urls.clear();
    assert!(incomplete.validate_release_downloads().is_err());
    let mut repeated = profile.clone();
    repeated.downloads[last].parts[1].backup_urls =
        repeated.downloads[last].parts[0].backup_urls.clone();
    assert!(repeated.validate_release_downloads().is_err());
    let mut short = profile.clone();
    short.downloads[last].parts[1].bytes += 1;
    assert!(short.validate_release_downloads().is_err());
    let mut singleton = profile.clone();
    singleton.downloads[0].parts[0].sha256 = Sha256Digest::from_bytes([9; 32]);
    assert!(singleton.validate_release_downloads().is_err());
    let mut raw = serde_json::to_value(value)?;
    raw["backup_bytes"] = json!(1);
    assert!(serde_json::from_value::<AssetPart>(raw).is_err());
    Ok(())
}
#[test]
fn native_download_origin_semantic_fixture_emitter() -> Result<()> {
    let original = profile()?;
    original.validate_release_downloads()?;
    let hf = part();
    hf.validate()?;
    let mut actual_hf_backup = hf.clone();
    actual_hf_backup.backup_urls = vec![HF_BACKUP.into()];
    actual_hf_backup.validate()?;
    // Transport shape only. No working GitHub backup or publication is invented.
    let mut configured_gh_backup = hf.clone();
    configured_gh_backup.backup_urls = vec![original.downloads[0].parts[0].url.clone()];
    configured_gh_backup.validate()?;
    let airk_mirrored = AssetDownload {
        asset_name: "structural-airk-mirror".into(),
        parts: vec![AssetPart {
            url: AIRK_HF.into(),
            backup_urls: vec![AIRK_GITHUB.into()],
            bytes: 17,
            sha256: Sha256Digest::from_bytes([1; 32]),
        }],
    };
    let airk_sources = airk_mirrored.source_count()?;
    assert_eq!(airk_sources, 2);
    let airk_part = &airk_mirrored.parts[0];
    assert_eq!(
        download_origin(airk_part.source_url(0).unwrap())?,
        DownloadOrigin::HuggingFace
    );
    assert_eq!(
        download_origin(airk_part.source_url(1).unwrap())?,
        DownloadOrigin::GitHubRelease
    );
    let incomplete = AssetDownload {
        asset_name: "structural-two-part".into(),
        parts: vec![actual_hf_backup.clone(), hf.clone()],
    };
    assert!(incomplete.source_count().is_err());
    let mut empty = incomplete.clone();
    empty.parts.clear();
    assert!(empty.source_count().is_err());
    let document = json!({"schema":"airk.arbiter.download-origin-cases/1","qualification":false,"network_executed":false,
        "hf_primary_no_backup":{"part":hf,"origin":"hugging_face","source_count":1,"public_metadata_identity_only":true},
        "hf_primary_actual_same_byte_hf_backup":{"part":actual_hf_backup,"source_count":2,"public_metadata_identity_only":true},
        "hf_primary_configured_gh_backup_structural_only":{"part":configured_gh_backup,"source_count":2,"working_gh_backup_asserted":false},
        "hf_airk_primary_configured_gh_backup_structural_only":{"part":airk_part,"source_count":airk_sources,"working_gh_backup_asserted":false},
        "retained_profile":{"profile":original,"empty_backups_omit":true,"old_profile_fixture_sha256":"d0064974be46e7cb56099a71c248587d81cd93eccd1405ec9e736664c199d593"},
        "incomplete_backup_roster":{"download":incomplete,"valid":false},"empty_parts":{"download":empty,"valid":false},
        "schema_document":crate::arbiter::schema::document()});
    if let Some(path) = std::env::var_os("AIRK_WRITE_ARBITER_DOWNLOAD_ORIGIN_CASES") {
        std::fs::write(path, serde_json::to_vec_pretty(&document)?)?;
    }
    if let Some(path) = std::env::var_os("AIRK_WRITE_ARBITER_SCHEMA") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&crate::arbiter::schema::document())?,
        )?;
    }
    Ok(())
}
