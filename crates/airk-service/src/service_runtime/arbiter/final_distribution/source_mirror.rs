// SPDX-License-Identifier: Apache-2.0
//! Prepare typed transport maps from privately pinned baseline and verified remote pins.
use super::{PinnedFile, pinned};
use airk_core::{
    arbiter::{AssetDownload, DownloadOrigin, download_origin},
    identity::Sha256Digest,
};
use airk_retrieval::arbiter::publication::github_part_filenames;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::PathBuf,
};

const SCHEMA: &str = "airk.private-arbiter-source-mirror-input/1";
const HUB_REPO: &str = "iokaio/airk-arbiter-models";
const HUB_PREFIX: &str = "https://huggingface.co/iokaio/airk-arbiter-models/";
const GITHUB_PREFIX: &str = "https://github.com/iokaio/airk/releases/download/";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MirrorInput {
    schema: String,
    github_tag: String,
    baseline_maps: Vec<PinnedFile>,
    hf_report: PinnedFile,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HubReport {
    repo: String,
    commit: String,
    files: Vec<RemoteFile>,
    #[serde(rename = "changed")]
    _changed: Option<bool>,
    requested_head: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoteFile {
    path: String,
    bytes: u64,
    sha256: Sha256Digest,
}

fn commit_id(commit: &str) -> bool {
    commit.len() == 40
        && commit
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn flat_name(name: &str) -> bool {
    name.len() <= 192
        && !name.contains("..")
        && name
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
}

fn prepare(
    mut maps: Vec<Vec<AssetDownload>>,
    report: HubReport,
    github_tag: &str,
) -> Result<Vec<Vec<AssetDownload>>> {
    ensure!(
        (1..=3).contains(&maps.len()),
        "one to three baseline download maps required"
    );
    ensure!(
        report.repo == HUB_REPO
            && commit_id(&report.commit)
            && report.requested_head.as_deref().is_none_or(commit_id),
        "unreviewed Hub repository or immutable commit"
    );
    let github_release = format!("{GITHUB_PREFIX}{github_tag}/");
    ensure!(
        download_origin(&format!("{github_release}part"))? == DownloadOrigin::GitHubRelease,
        "invalid GitHub release tag"
    );
    let mut verified = BTreeMap::new();
    for file in report.files {
        ensure!(
            flat_name(&file.path)
                && (1..=1 << 30).contains(&file.bytes)
                && verified
                    .insert(file.path, (file.bytes, file.sha256))
                    .is_none(),
            "invalid or duplicate verified Hub filename"
        );
    }

    let mut covered = BTreeSet::new();
    for map in &mut maps {
        ensure!(
            (1..=128).contains(&map.len()),
            "invalid Arbiter asset download census"
        );
        let mut names = BTreeSet::new();
        for asset in map {
            ensure!(
                !asset.asset_name.is_empty() && names.insert(asset.asset_name.clone()),
                "missing or duplicate Arbiter logical asset name"
            );
            asset.source_count()?;
            let origin = download_origin(&asset.parts[0].url)?;
            for part in &mut asset.parts {
                ensure!(
                    download_origin(&part.url)? == origin,
                    "mixed-primary Arbiter asset cannot be mirrored"
                );
                github_part_filenames(part, github_tag)?;
                if origin == DownloadOrigin::HuggingFace {
                    ensure!(
                        !part.url.starts_with(HUB_PREFIX)
                            && !part
                                .backup_urls
                                .iter()
                                .any(|url| url.starts_with(HUB_PREFIX)),
                        "baseline already contains the destination mirror"
                    );
                    continue;
                }
                ensure!(
                    part.backup_urls.is_empty(),
                    "baseline GitHub part already has an alternate source"
                );
                let filename = part
                    .url
                    .strip_prefix(&github_release)
                    .context("baseline part differs from the requested GitHub tag")?;
                ensure!(
                    verified.get(filename) == Some(&(part.bytes, part.sha256)),
                    "unverified or changed Hub part {filename}"
                );
                let url = format!("{HUB_PREFIX}resolve/{}/{filename}", report.commit);
                covered.insert(filename.to_owned());
                part.backup_urls.push(std::mem::replace(&mut part.url, url));
            }
            asset.source_count()?;
        }
    }
    ensure!(
        !covered.is_empty() && covered.len() == verified.len(),
        "verified Hub roster exceeds the selected GitHub-primary part union"
    );
    Ok(maps)
}

fn load_maps(input: &MirrorInput) -> Result<Vec<Vec<AssetDownload>>> {
    ensure!(
        input.schema == SCHEMA && (1..=3).contains(&input.baseline_maps.len()),
        "invalid private source mirror input"
    );
    let mut paths = BTreeSet::new();
    let maps = input
        .baseline_maps
        .iter()
        .map(|pin| {
            ensure!(paths.insert(&pin.path), "duplicate baseline map input");
            Ok(serde_json::from_slice(&pinned(pin)?)?)
        })
        .collect::<Result<Vec<_>>>()?;
    let report = serde_json::from_slice(&pinned(&input.hf_report)?)?;
    prepare(maps, report, &input.github_tag)
}

#[test]
#[ignore = "explicit private input pins required; produces review metadata only"]
fn emit_reviewed_download_source_maps() -> Result<()> {
    let input = PathBuf::from(
        std::env::var_os("AIRK_ARBITER_SOURCE_MIRROR_INPUT")
            .context("private mirror input path")?,
    );
    let pin = PinnedFile {
        bytes: fs::symlink_metadata(&input)?.len(),
        path: input,
        sha256: std::env::var("AIRK_ARBITER_SOURCE_MIRROR_INPUT_SHA256")?.parse()?,
    };
    let input = serde_json::from_slice(&pinned(&pin)?)?;
    let output = PathBuf::from(
        std::env::var_os("AIRK_ARBITER_SOURCE_MIRROR_OUTPUT").context("fresh mirror map output")?,
    );
    ensure!(
        output.is_absolute() && !output.exists(),
        "fresh absolute mirror map output required"
    );
    let maps = load_maps(&input)?
        .iter()
        .map(serde_json::to_vec_pretty)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    fs::create_dir(&output)?;
    for (index, map) in maps.iter().enumerate() {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join(format!("download-sources-{index:03}.json")))?;
        file.write_all(map)?;
        file.sync_all()?;
    }
    Ok(())
}

#[test]
fn mirrored_maps_preserve_part_order_pins_and_upstream_sources() -> Result<()> {
    let primary = "https://github.com/iokaio/airk/releases/download/v1.9.0/";
    let hf = "https://huggingface.co/iokaio/airk-arbiter-models/resolve/";
    let commit = "0123456789abcdef0123456789abcdef01234567";
    let model = AssetDownload {
        asset_name: "model.onnx.data".into(),
        parts: vec![
            airk_core::arbiter::AssetPart {
                url: format!("{primary}model.onnx.data.part001"),
                backup_urls: vec![],
                bytes: 2,
                sha256: Sha256Digest::from_bytes([1; 32]),
            },
            airk_core::arbiter::AssetPart {
                url: format!("{primary}model.onnx.data.part002"),
                backup_urls: vec![],
                bytes: 3,
                sha256: Sha256Digest::from_bytes([2; 32]),
            },
        ],
    };
    let tokenizer = AssetDownload {
        asset_name: "tokenizer.json".into(),
        parts: vec![airk_core::arbiter::AssetPart {
            url: format!("https://huggingface.co/Qwen/Qwen3-1.7B/resolve/{commit}/tokenizer.json"),
            backup_urls: vec![],
            bytes: 4,
            sha256: Sha256Digest::from_bytes([3; 32]),
        }],
    };
    let report = HubReport {
        repo: HUB_REPO.into(),
        commit: commit.into(),
        files: vec![
            RemoteFile {
                path: "model.onnx.data.part002".into(),
                bytes: 3,
                sha256: Sha256Digest::from_bytes([2; 32]),
            },
            RemoteFile {
                path: "model.onnx.data.part001".into(),
                bytes: 2,
                sha256: Sha256Digest::from_bytes([1; 32]),
            },
        ],
        _changed: Some(false),
        requested_head: Some(commit.into()),
    };
    let actual = prepare(
        vec![vec![model.clone(), tokenizer.clone()], vec![model.clone()]],
        report,
        "v1.9.0",
    )?;
    assert_eq!(actual[0][1], tokenizer);
    assert_eq!(actual[1][0], actual[0][0]);
    let parts = &actual[0][0].parts;
    assert_eq!(
        parts[0].url,
        format!("{hf}{commit}/model.onnx.data.part001")
    );
    assert_eq!(
        parts[1].url,
        format!("{hf}{commit}/model.onnx.data.part002")
    );
    for (actual, before) in parts.iter().zip(model.parts) {
        assert_eq!(actual.backup_urls, [before.url]);
        assert_eq!((actual.bytes, actual.sha256), (before.bytes, before.sha256));
    }
    Ok(())
}

#[test]
fn pinned_reports_and_baseline_maps_refuse_in_place_mutation() -> Result<()> {
    fn write_pin(path: PathBuf, raw: &[u8]) -> Result<PinnedFile> {
        fs::write(&path, raw)?;
        Ok(PinnedFile {
            path,
            bytes: raw.len() as u64,
            sha256: super::sha(raw),
        })
    }
    let dir = tempfile::tempdir()?;
    let map = serde_json::to_vec(&serde_json::json!([{
        "asset_name": "model",
        "parts": [{
            "url": "https://github.com/iokaio/airk/releases/download/v1.9.0/model.onnx",
            "bytes": 1,
            "sha256": "12".repeat(32)
        }]
    }]))?;
    let report = serde_json::to_vec(&serde_json::json!({
        "repo": "iokaio/airk-arbiter-models",
        "commit": "a".repeat(40),
        "files": [{"path":"model.onnx","bytes":1,"sha256":"12".repeat(32)}]
    }))?;
    let baseline = write_pin(dir.path().join("baseline.json"), &map)?;
    let readback = write_pin(dir.path().join("report.json"), &report)?;
    let input = MirrorInput {
        schema: SCHEMA.into(),
        github_tag: "v1.9.0".into(),
        baseline_maps: vec![baseline],
        hf_report: readback,
    };
    assert_eq!(load_maps(&input)?.len(), 1);
    for (pin, bytes) in [(&input.baseline_maps[0], &map), (&input.hf_report, &report)] {
        let mut changed = bytes.to_vec();
        changed[0] = b' ';
        fs::write(&pin.path, changed)?;
        assert!(
            load_maps(&input)
                .unwrap_err()
                .to_string()
                .contains("bytes differ")
        );
        fs::write(&pin.path, bytes)?;
    }
    Ok(())
}

#[test]
fn partial_conflicting_or_unreviewed_mirror_inputs_refuse() -> Result<()> {
    let baseline = serde_json::json!([{
        "asset_name": "model.onnx",
        "parts": [{
            "url": "https://github.com/iokaio/airk/releases/download/v1.9.0/model.onnx",
            "bytes": 13,
            "sha256": "11".repeat(32)
        }]
    }]);
    let report = serde_json::json!({
        "repo": "iokaio/airk-arbiter-models",
        "commit": "a".repeat(40),
        "files": [{"path":"model.onnx", "bytes":13, "sha256":"11".repeat(32)}]
    });
    for field in [
        "repo",
        "revision",
        "missing",
        "extra",
        "duplicate",
        "size",
        "digest",
        "tag",
        "backup",
    ] {
        let mut body = report.clone();
        let mut map = baseline.clone();
        let mut tag = "v1.9.0";
        match field {
            "repo" => body["repo"] = "someone/airk-arbiter-models".into(),
            "revision" => body["commit"] = "main".into(),
            "missing" => body["files"] = serde_json::json!([]),
            "extra" | "duplicate" => {
                let mut row = body["files"][0].clone();
                if field == "extra" {
                    row["path"] = "foreign.onnx".into();
                }
                body["files"].as_array_mut().unwrap().push(row);
            }
            "size" => body["files"][0]["bytes"] = 12.into(),
            "digest" => body["files"][0]["sha256"] = "22".repeat(32).into(),
            "tag" => tag = "v1.9.1",
            "backup" => {
                map[0]["parts"][0]["backup_urls"] = serde_json::json!([
                    "https://github.com/iokaio/airk/releases/download/v1.9.0/old.onnx"
                ]);
            }
            _ => unreachable!(),
        }
        assert!(
            prepare(
                vec![serde_json::from_value(map)?],
                serde_json::from_value(body)?,
                tag
            )
            .is_err(),
            "{field}"
        );
    }
    Ok(())
}
