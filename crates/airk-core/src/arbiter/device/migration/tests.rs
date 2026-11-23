// SPDX-License-Identifier: Apache-2.0
use super::*;

fn installation() -> Sha256Digest {
    Sha256Digest::from_bytes([1; 32])
}
fn source_catalog() -> Incarnation {
    Incarnation::from_bytes([2; 16])
}
fn binding(byte: u8) -> DeviceCatalogBinding {
    DeviceCatalogBinding {
        catalog: Sha256Digest::from_bytes([byte; 32]),
        source: Sha256Digest::from_bytes([byte + 1; 32]),
    }
}
fn transition() -> Configuration4Transition {
    Configuration4Transition {
        old: binding(0x10),
        new: binding(0x20),
    }
}

/// A configuration/4 document written out by hand in that schema's field
/// order, so the expectations below do not come from the code under test.
fn v4(revision: u64, catalog: &str, focus: &str, context: &str) -> Vec<u8> {
    format!(
        concat!(
            r#"{{"schema":"airk.arbiter.configuration/4","installation":"{}","#,
            r#""catalog":"02020202020202020202020202020202","revision":{},"#,
            r#""device_catalog":{},"focus":{},"context":{}}}"#,
            "\n"
        ),
        "01".repeat(32),
        revision,
        catalog,
        focus,
        context
    )
    .into_bytes()
}
fn old_catalog() -> String {
    format!(
        r#"{{"catalog":"{}","source":"{}"}}"#,
        "10".repeat(32),
        "11".repeat(32)
    )
}
const OFF: &str = r#"{"enabled":false,"selected_model":null,"device_policy":"auto","gpu_precision":"auto","indexes":[]}"#;
const ENROLLED: &str =
    r#"[{"index_id":"idx_1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f","tasks":["airk.context_action.v1"]}]"#;
fn context(model: &str, enabled: bool, policy: &str, precision: &str) -> String {
    format!(
        r#"{{"enabled":{enabled},"selected_model":"{model}","device_policy":"{policy}","gpu_precision":"{precision}","indexes":{ENROLLED}}}"#
    )
}

fn migrate(bytes: &[u8]) -> Result<DeviceConfiguration> {
    DeviceConfiguration::migrate_v4(bytes, installation(), source_catalog(), &transition())
}

#[test]
fn a_disabled_document_moves_to_the_new_binding_and_the_next_revision() -> Result<()> {
    let doc = migrate(&v4(1, &old_catalog(), OFF, OFF))?;
    assert_eq!(doc.schema, "airk.arbiter.configuration/5");
    assert_eq!(doc.revision, 2);
    assert_eq!(doc.device_catalog, binding(0x20));
    assert_eq!(doc.focus, DeviceStageConfiguration::disabled());
    assert_eq!(doc.context, DeviceStageConfiguration::disabled());
    assert_eq!(doc.context_retired, None);
    Ok(())
}

#[test]
fn a_compact_selection_becomes_arbiter_with_everything_else_kept() -> Result<()> {
    let doc = migrate(&v4(
        4,
        &old_catalog(),
        OFF,
        &context("context_compact", true, "cpu", "fp16"),
    ))?;
    assert_eq!(doc.revision, 5);
    assert!(doc.context.enabled);
    assert_eq!(doc.context.selected_model, Some(SelectedModel::Arbiter));
    assert_eq!(doc.context.device_policy, DevicePolicy::Cpu);
    assert_eq!(doc.context.gpu_precision, GpuPrecisionPreference::Fp16);
    assert_eq!(doc.context.indexes.len(), 1);
    assert_eq!(doc.context_retired, None);
    // The /5 bytes say "arbiter" where /4 said "context_compact".
    let text = String::from_utf8(doc.canonical_bytes()?)?;
    assert!(text.contains(r#""selected_model":"arbiter""#));
    assert!(!text.contains("context_compact") && !text.contains("context_retired"));
    Ok(())
}

#[test]
fn a_large_selection_is_retired_never_replaced_by_arbiter() -> Result<()> {
    let doc = migrate(&v4(
        2,
        &old_catalog(),
        OFF,
        &context("context_large", true, "auto", "fp32"),
    ))?;
    assert_eq!(doc.revision, 3);
    assert!(!doc.context.enabled);
    assert_eq!(doc.context.selected_model, None);
    assert_eq!(doc.context.gpu_precision, GpuPrecisionPreference::Fp32);
    assert_eq!(doc.context.indexes.len(), 1, "enrollments are kept");
    assert_eq!(doc.context_retired, Some(RetiredModel::ContextLarge));
    let text = String::from_utf8(doc.canonical_bytes()?)?;
    assert!(text.contains(r#""context_retired":"context_large""#));
    // The /5 document decodes under the new binding.
    let decoded = DeviceConfiguration::decode(
        text.as_bytes(),
        installation(),
        source_catalog(),
        &binding(0x20),
    )?;
    assert_eq!(decoded, doc);
    Ok(())
}

#[test]
fn a_retired_marker_needs_a_disabled_stage_with_no_model() -> Result<()> {
    let mut doc = migrate(&v4(1, &old_catalog(), OFF, OFF))?;
    doc.context_retired = Some(RetiredModel::ContextLarge);
    doc.validate()?;
    doc.context.selected_model = Some(SelectedModel::Arbiter);
    assert!(doc.validate().is_err());
    Ok(())
}

#[test]
fn migration_refuses_a_foreign_noncanonical_or_unnamed_document() {
    let good = v4(1, &old_catalog(), OFF, OFF);
    assert!(migrate(&good).is_ok());
    // A binding the compiled transition does not name.
    let other = format!(
        r#"{{"catalog":"{}","source":"{}"}}"#,
        "30".repeat(32),
        "31".repeat(32)
    );
    assert!(migrate(&v4(1, &other, OFF, OFF)).is_err());
    // Not canonical: an extra space, or no final newline.
    let spaced = String::from_utf8(good.clone())
        .unwrap()
        .replacen(':', ": ", 1);
    assert!(migrate(spaced.as_bytes()).is_err());
    assert!(migrate(&good[..good.len() - 1]).is_err());
    // Another installation.
    assert!(
        DeviceConfiguration::migrate_v4(
            &good,
            Sha256Digest::from_bytes([9; 32]),
            source_catalog(),
            &transition()
        )
        .is_err()
    );
    // A transition that does not move the binding.
    let still = Configuration4Transition {
        old: binding(0x10),
        new: binding(0x10),
    };
    assert!(
        DeviceConfiguration::migrate_v4(&good, installation(), source_catalog(), &still).is_err()
    );
    // A model in the other stage's slot, and an unknown field.
    let crossed = v4(
        1,
        &old_catalog(),
        &context("context_compact", false, "auto", "auto"),
        OFF,
    );
    assert!(migrate(&crossed).is_err());
    let unknown = String::from_utf8(good.clone()).unwrap().replacen(
        r#""revision":1"#,
        r#""revision":1,"extra":0"#,
        1,
    );
    assert!(migrate(unknown.as_bytes()).is_err());
    // A configuration/5 document is not migrated again.
    let five = migrate(&good).unwrap().canonical_bytes().unwrap();
    assert!(migrate(&five).is_err());
}

#[test]
fn the_binding_of_a_configuration4_document_is_read_without_choosing_a_catalog() -> Result<()> {
    let (found, revision) = configuration4_binding(
        &v4(3, &old_catalog(), OFF, OFF),
        installation(),
        source_catalog(),
    )?;
    assert_eq!((found, revision), (binding(0x10), 3));
    Ok(())
}
