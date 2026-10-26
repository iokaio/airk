// SPDX-License-Identifier: Apache-2.0
//! Isolated process: the transport connector is intentionally installed only once.
use airk_core::usage::{ContentKind, TokenizerAlgorithm, TokenizerIdentity};
use airk_kernel::{Kernel, usage::CalibrationRequest};
use airk_retrieval::embedding_worker::{
    self, Connector, ModelLocation, RemoteSession, Request, Response,
};
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

struct FakeHelper {
    definition: PathBuf,
    changed: AtomicBool,
}
struct FakeConnector(Arc<FakeHelper>);
impl Connector for FakeConnector {
    fn open(&self, _: ModelLocation, _: &str) -> Result<Arc<dyn RemoteSession>> {
        anyhow::bail!("calibration must not load an embedding model")
    }
    fn tokenizer(&self, path: &Path) -> Result<Arc<dyn RemoteSession>> {
        ensure!(
            path == self.0.definition.canonicalize()?,
            "wrong selected definition"
        );
        Ok(self.0.clone())
    }
}
impl RemoteSession for FakeHelper {
    fn request(&self, request: Request) -> Result<Response> {
        match request {
            Request::TokenizerOpen { path, .. } => {
                let mut bytes = fs::read(path)?;
                // Simulate a different definition captured after the caller's read.
                if self.changed.load(Ordering::Acquire) {
                    bytes.push(b' ');
                }
                let mut hash = Sha256::new();
                hash.update(b"airk-local-calibration/2\0tokenizers/0.23.2\0");
                hash.update(bytes);
                Ok(Response::TokenizerOpened {
                    identity: TokenizerIdentity {
                        algorithm: TokenizerAlgorithm::HuggingFaceWordLevel,
                        definition_sha256: format!("{:x}", hash.finalize()),
                        model_revision_sha256: "a".repeat(64),
                    },
                    description: serde_json::json!({"fixture":"remote transport"}),
                })
            }
            Request::TokenCount { text } => Ok(Response::TokenCount {
                count: if text == "\"baseline\"" { 4 } else { 2 },
            }),
            _ => anyhow::bail!("unexpected calibration operation"),
        }
    }
}
#[test]
fn service_calibration_rejects_changed_helper_definition_before_counting_or_recording() -> Result<()>
{
    let owner = tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    let runtime = owner.path().join("runtime");
    let model = owner.path().join("model");
    for path in [&source, &data, &runtime, &model] {
        fs::create_dir(path)?;
    }
    let vocabulary = br#"{"copper":[1.0,0.0]}"#;
    fs::write(model.join("vocabulary.json"), vocabulary)?;
    fs::write(
        model.join("model.json"),
        serde_json::to_vec(&serde_json::json!({"schema":1,
        "name":"unused original fixture","license":"Apache-2.0","dimensions":2,"sha256":format!("{:x}",Sha256::digest(vocabulary))}))?,
    )?;
    let kernel = Kernel::open_service_collection(&source, Some(data.clone()), "fixture")?
        .with_service_models(
            &runtime,
            airk_retrieval::model::inspect_object_plan(&model)?,
        )?;
    let definition = owner.path().join("definition.json");
    fs::write(
        &definition,
        serde_json::to_vec(&serde_json::json!({"schema":2,
        "model_revision_sha256":"a".repeat(64),"tokenizer_file":"tokenizer.json","tokenizer_sha256":"b".repeat(64),
        "add_special_tokens":false,"framing":{"prefix":"","suffix":""}}))?,
    )?;
    let baseline = owner.path().join("before.json");
    let returned = owner.path().join("after.json");
    fs::write(&baseline, "\"baseline\"")?;
    fs::write(&returned, "\"returned\"")?;
    let helper = Arc::new(FakeHelper {
        definition: definition.clone(),
        changed: AtomicBool::new(true),
    });
    embedding_worker::install_connector(Arc::new(FakeConnector(helper.clone())))?;
    let request = || CalibrationRequest {
        tokenizer: &definition,
        baseline: &baseline,
        returned: &returned,
        generation: 1,
        content_kind: ContentKind::PlainText,
        record: false,
    };
    let error = kernel.usage_calibrate(request()).unwrap_err();
    assert!(format!("{error:#}").contains("definition changed"));
    helper.changed.store(false, Ordering::Release);
    let result = kernel.usage_calibrate(request())?;
    assert_eq!(result["summary"]["baseline_tokens"], "4");
    assert_eq!(result["summary"]["returned_tokens"], "2");
    assert!(result["recording"].is_null());
    assert_eq!(fs::read_to_string(baseline)?, "\"baseline\"");
    assert!(
        !runtime.join("model-files-1.4").exists(),
        "calibration unexpectedly provisioned MiniLM"
    );
    Ok(())
}
