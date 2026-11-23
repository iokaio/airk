// SPDX-License-Identifier: Apache-2.0
//! Literal receipt vocabulary controls, not native model/device qualification.
use super::*;
const CUDA: &str = "onnxruntime-cuda/1.30.0/device-execution/1111111111111111111111111111111111111111111111111111111111111111";
const DML: &str = "onnxruntime-directml/1.30.0/device-execution/2222222222222222222222222222222222222222222222222222222222222222";
const COREML: &str = "onnxruntime-coreml/1.30.0/device-execution/3333333333333333333333333333333333333333333333333333333333333333";
#[test]
fn receipt_runtime_closed_cpu_and_gpu_digest_vocabulary() -> Result<()> {
    for value in [RUNTIME, CUDA, DML, COREML] {
        assert!(valid_advice_runtime(value), "refused literal {value}");
    }
    for value in [
        "onnxruntime-cuda/1.30.0",
        "onnxruntime-gpu/1.30.0",
        "onnxruntime-cpu/1.30.0/physical1/intra1/inter1/sequential/spinning0",
        "onnxruntime-cuda/1.31.0/device-execution/1111111111111111111111111111111111111111111111111111111111111111",
        "onnxruntime-cpu/1.30.0/device-execution/1111111111111111111111111111111111111111111111111111111111111111",
        "onnxruntime-cuda/1.30.0/device-execution/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "onnxruntime-cuda/1.30.0/device-execution/111111111111111111111111111111111111111111111111111111111111111",
        "onnxruntime-cuda/1.30.0/device-execution/11111111111111111111111111111111111111111111111111111111111111111",
    ] {
        assert!(!valid_advice_runtime(value), "accepted malformed {value}");
    }
    assert!(!valid_advice_runtime(&format!("{CUDA}/suffix")));
    assert!(!valid_advice_runtime(&format!("{CUDA}\n")));
    Ok(())
}
#[test]
fn profile_receipt_runtime_keeps_cpu_bytes_and_binds_gpu_execution() -> Result<()> {
    let cpu = crate::arbiter::device::tests::literal_profile(SelectedModel::Arbiter, 1, false)?;
    assert_eq!(cpu.advice_runtime()?, RUNTIME);
    let before = serde_json::to_vec(&cpu)?;
    cpu.validate()?;
    assert_eq!(serde_json::to_vec(&cpu)?, before);
    let mut gpu = crate::arbiter::device::tests::literal_profile(SelectedModel::Arbiter, 2, true)?;
    let original = gpu.advice_runtime()?;
    assert_eq!(original, gpu.device_execution_identity()?);
    assert!(valid_advice_runtime(&original));
    assert!(!valid_advice_runtime(&gpu.runtime));
    gpu.device_execution.as_mut().unwrap().math_policy = Sha256Digest::from_bytes([99; 32]);
    assert_ne!(gpu.advice_runtime()?, original);
    Ok(())
}
#[test]
fn all_advice_and_focus_runtime_siblings_accept_bound_gpu_without_cpu_projection() -> Result<()> {
    let pin = Sha256Digest::from_bytes([7; 32]);
    let mut advice = Advice {
        schema: ADVICE_SCHEMA.into(),
        task: Task::RequestSupported,
        profile: pin,
        runtime: RUNTIME.into(),
        compiler: pin,
        calibration: pin,
        policy: pin,
        scope: crate::arbiter::stage_tests::scope(),
        input_digest: pin,
        evidence: vec![],
        work: Work::default(),
        outcome: Outcome::Unavailable {
            reason: "Synthetic no model qualification".into(),
        },
    };
    let cpu = serde_json::to_vec(&advice)?;
    advice.validate()?;
    assert_eq!(serde_json::to_vec(&advice)?, cpu);
    let mut joint = context_action::ContextActionAdvice {
        schema: context_action::ADVICE_SCHEMA_V2.into(),
        task: Task::ContextAction,
        profile: pin,
        runtime: RUNTIME.into(),
        compiler: pin,
        calibration: pin,
        policy: pin,
        task_policy: pin,
        scope: crate::arbiter::stage_tests::scope(),
        input_digest: pin,
        evidence: vec![],
        reserved_input_tokens: 1,
        observed_input_tokens: Some(0),
        work: Work::default(),
        outcome: Outcome::Unavailable {
            reason: "Synthetic no task admission".into(),
        },
    };
    let budget = Budget::default();
    let mut ranking = crate::arbiter::focus::tests::ranking()?;
    let mut application = FocusApplication {
        application: IndexApplication {
            scope: crate::arbiter::stage_tests::scope(),
            profile: Some(pin),
            availability: Availability::Ready,
            reason: "Synthetic native view only".into(),
        },
        identity: ranking.identity.clone(),
        limits: Some(FocusProfileLimits {
            max_candidates: 8,
            pair_input_tokens: 512,
            max_input_tokens: 8192,
        }),
    };
    for runtime in [RUNTIME, CUDA, DML, COREML] {
        advice.runtime = runtime.into();
        joint.runtime = runtime.into();
        ranking.identity.as_mut().unwrap().runtime = runtime.into();
        application.identity.as_mut().unwrap().runtime = runtime.into();
        advice.validate()?;
        joint.validate()?;
        ranking.validate(&budget)?;
        application.validate()?;
        assert_eq!(advice.runtime, runtime);
        assert_eq!(ranking.identity.as_ref().unwrap().runtime, runtime);
    }
    for runtime in ["onnxruntime-cuda/1.30.0", "unapproved-device"] {
        advice.runtime = runtime.into();
        joint.runtime = runtime.into();
        ranking.identity.as_mut().unwrap().runtime = runtime.into();
        application.identity.as_mut().unwrap().runtime = runtime.into();
        assert!(advice.validate().is_err());
        assert!(joint.validate().is_err());
        assert!(ranking.validate(&budget).is_err());
        assert!(application.validate().is_err());
    }
    Ok(())
}
