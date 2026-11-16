// SPDX-License-Identifier: Apache-2.0
//! Native original publisher checkpoint roster; no model or reference execution.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OriginalSourceInput {
    schema: String,
    model: ModelChoice,
    files: Vec<ResourceFilePin>,
}
#[derive(Clone)]
pub(super) struct SourceSpec {
    name: &'static str,
    sha256: &'static str,
    role: Option<AssetRole>,
}
fn source(name: &'static str, sha256: &'static str, role: Option<AssetRole>) -> SourceSpec {
    SourceSpec { name, sha256, role }
}
fn specifications(model: ModelChoice) -> Result<Vec<SourceSpec>> {
    let mut sources = match model {
        ModelChoice::FocusQwenReranker06B => vec![source(
            "model.safetensors",
            "27cd75a405b9c1b46b59abfd88aaa209e6fed2a1972cde9b70e7659537c5e65b",
            Some(AssetRole::Model),
        )],
        ModelChoice::ContextQwenCompact => vec![
            source(
                "model-00001-of-00002.safetensors",
                "169ad53ec313c3a34b06c0809216e4fc072cce444a5d4ff2b59690d064130ed5",
                Some(AssetRole::Model),
            ),
            source(
                "model-00002-of-00002.safetensors",
                "912becff8d60672aa8628ef08c05898d9adf17c2ad4ae3caf99b065622fdeff9",
                Some(AssetRole::ExternalWeights),
            ),
        ],
        ModelChoice::ContextQwenLarge => vec![
            source(
                "model-00001-of-00003.safetensors",
                "328a91d3122359d5547f9d79521205bc0a46e1f79a792dfe650e99fc2d651223",
                Some(AssetRole::Model),
            ),
            source(
                "model-00002-of-00003.safetensors",
                "6cd087b316306a68c562436b5492edbcf6e16c6dba3a1308279caa5a58e21ca5",
                Some(AssetRole::ExternalWeights),
            ),
            source(
                "model-00003-of-00003.safetensors",
                "e4bf436957184f4eeb86a80e9db394503f1f56446b2e6b7edeac5b81470f4ca1",
                Some(AssetRole::ExternalWeights),
            ),
        ],
        _ => anyhow::bail!("original numerical source is not a selected publisher model"),
    };
    sources.push(source(
        "tokenizer.json",
        "aeb13307a71acd8fe81861d94ad54ab689df773318809eed3cbe794b4492dae4",
        Some(AssetRole::Tokenizer),
    ));
    let config = match model {
        ModelChoice::FocusQwenReranker06B => {
            "d479c427a9ca5295218063d4f9aca4f297ab4ac27487cca7af42c84643d51ef0"
        }
        ModelChoice::ContextQwenCompact => {
            "1ddb5b89ebc90dcb417a45c213d818577e65976454d29385c8f6140771d95197"
        }
        ModelChoice::ContextQwenLarge => {
            "8ba006f74fecfaaeb392872a60f4a480e7ec9860153d2e1b769ec81f9a147f8a"
        }
        _ => unreachable!(),
    };
    sources.push(source("config.json", config, None));
    if model != ModelChoice::FocusQwenReranker06B {
        let index = if model == ModelChoice::ContextQwenCompact {
            "0d660e94b165eb912669a5249dff44b83188c4777a07ddb9611fb78d91b0578d"
        } else {
            "6dc0981b8829fead746441f68f38f24c5ca4a3a66351f652c26c6df0efc43ab2"
        };
        sources.push(source("model.safetensors.index.json", index, None));
    }
    Ok(sources)
}
pub(super) fn select_sources(
    model: ModelChoice,
    pins: &[ResourceFilePin],
) -> Result<Vec<(SourceSpec, ResourceFilePin)>> {
    let expected = specifications(model)?;
    ensure!(
        pins.len() == expected.len(),
        "complete original publisher checkpoint/config/tokenizer required"
    );
    let mut names = BTreeMap::new();
    for pin in pins {
        pin.validate()?;
        let name = Path::new(&pin.path)
            .file_name()
            .and_then(|name| name.to_str())
            .context("original source filename absent")?;
        ensure!(
            names.insert(name, pin).is_none(),
            "duplicate original publisher source"
        );
    }
    expected
        .into_iter()
        .map(|source| {
            let pin = names
                .get(source.name)
                .context("missing/foreign original publisher source")?;
            ensure!(
                pin.sha256 == source.sha256.parse::<Sha256Digest>()?,
                "cross-model or drifted original publisher identity"
            );
            Ok((source, (**pin).clone()))
        })
        .collect()
}
pub(super) fn measure_source(pin: &ResourceFilePin) -> Result<Value> {
    pin.validate()?;
    let path = Path::new(&pin.path);
    let before = std::fs::symlink_metadata(path)?;
    ensure!(
        before.is_file()
            && !before.file_type().is_symlink()
            && before.len() > 0
            && before.len() <= 16u64 << 30,
        "original source is not a bounded regular file"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            before.file_attributes() & 0x400 == 0,
            "original publisher reparse source refused"
        );
    }
    let digest = file_hash(path)?;
    let after = std::fs::symlink_metadata(path)?;
    ensure!(
        after.is_file() && !after.file_type().is_symlink(),
        "original source type changed while hashing"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            after.file_attributes() & 0x400 == 0,
            "original source became a reparse point"
        );
    }
    ensure!(
        digest == pin.sha256
            && before.len() == after.len()
            && before.modified()? == after.modified()?,
        "original publisher file changed while hashing"
    );
    Ok(json!({"path":pin.path,"bytes":after.len(),"sha256":digest}))
}

#[test]
#[ignore = "exact independently pinned original publisher files and current private native request required; NN0"]
fn emit_software_v2_original_reference_assets() -> Result<()> {
    let (input, profile, _, native_input_sha) = inputs()?;
    let request_pin = ResourceFilePin {
        path: std::env::var("AIRK_SOFTWARE_V2_ORIGINAL_INPUT")?,
        sha256: std::env::var("AIRK_SOFTWARE_V2_ORIGINAL_INPUT_SHA256")?.parse()?,
    };
    let request: OriginalSourceInput = serde_json::from_slice(&pinned(&request_pin, 1 << 20)?)?;
    ensure!(
        request.schema == "airk.private.original-numeric-source-input/1"
            && request.model == profile.model,
        "original reference source model/request differs"
    );
    let sources = select_sources(request.model, &request.files)?;
    let mut assets = Vec::new();
    let mut reference_files = BTreeMap::new();
    let mut source_files = BTreeMap::new();
    for (source, pin) in &sources {
        let measured = measure_source(pin)?;
        if let Some(role) = &source.role {
            assets.push(Asset {
                name: source.name.into(),
                role: role.clone(),
                bytes: measured["bytes"]
                    .as_u64()
                    .context("actual source length absent")?,
                sha256: pin.sha256,
            });
            reference_files.insert(source.name, measured.clone());
        }
        source_files.insert(source.name, measured);
    }
    for (_, pin) in &sources {
        measure_source(pin)?;
    }
    pinned(&request_pin, 1 << 20)?;
    let (publisher_model, publisher_revision, publisher_manifest_sha256) = match profile.model {
        ModelChoice::FocusQwenReranker06B => (
            "Qwen/Qwen3-Reranker-0.6B",
            "e61197ed45024b0ed8a2d74b80b4d909f1255473",
            "412bba9ad1c10197ef0d2b67db20690b415194fe3473e3031a35e056509b9710",
        ),
        ModelChoice::ContextQwenCompact => (
            "Qwen/Qwen3-1.7B",
            "70d244cc86ccca08cf5af4e1e306ecf908b1ad5e",
            "4cac4fc751d996e987bb4701fb90ce8cbd600af06152151b4ea99bed8d6c664c",
        ),
        ModelChoice::ContextQwenLarge => (
            "Qwen/Qwen3-4B",
            "1cfa9a7208912126459214e8b04321603b3df60c",
            "4cac4fc751d996e987bb4701fb90ce8cbd600af06152151b4ea99bed8d6c664c",
        ),
        _ => unreachable!(),
    };
    let report = json!({"schema":"airk.arbiter.original-numeric-source-assets/1","model":profile.model,
        "publisher_model":publisher_model,"publisher_revision":publisher_revision,"publisher_manifest_sha256":publisher_manifest_sha256,
        "compiler":profile.compiler,"resource_policy_digest":input.policy_digest,"native_input_sha256":native_input_sha,
        "original_source_input":request_pin,"original_reference_assets":assets,"original_reference_files":reference_files,
        "original_source_files":source_files,"native_airk_nn_invocations":0,"qualification":false,
        "scope":"Native serialization/authentication only; original independent Torch scores and actual custody remain required."});
    let output = PathBuf::from(std::env::var("AIRK_SOFTWARE_V2_NATIVE_OUTPUT")?);
    ensure!(
        output.is_absolute(),
        "absolute fresh native original-source output required"
    );
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.sync_all()?;
    Ok(())
}
