// SPDX-License-Identifier: Apache-2.0
//! Qualified first-wave file-aware retention; no source I/O, adapter execution or policy mutation.
use airk_core::{
    extraction::{ExtractionPayload, ExtractionProfile},
    retention::{AppliedRetention, RetainedFamily, RetainedProfile},
};
use anyhow::{Result, ensure};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackReason {
    UnqualifiedAdapterOrEffectiveVersion,
}
#[derive(Clone, Debug)]
pub enum Selection {
    Qualified {
        producer: ExtractionProfile,
        retained: ExtractionProfile,
    },
    LegacyFallback {
        retained: ExtractionProfile,
        reason: FallbackReason,
    },
}
/// Compare complete current producer identity, not only an adapter name/version.
/// Header contexts/composites/other formats are unchanged legacy fallbacks in this first wave.
pub fn select_profile(
    path: &str,
    base: &ExtractionProfile,
    target: RetainedProfile,
) -> Result<Selection> {
    super::adapters::profile::validate_relative_path(path)?;
    base.identity()?;
    ensure!(
        base.contract_version <= 2,
        "new selection requires original producer identity"
    );
    let family = match base.adapter_id.as_str() {
        "rust" => RetainedFamily::Code,
        "project-manifest" => RetainedFamily::Manifest,
        "markdown" | "instruction-markdown" => RetainedFamily::Markdown,
        "docx" => RetainedFamily::Docx,
        _ => {
            return Ok(Selection::LegacyFallback {
                retained: base.clone(),
                reason: FallbackReason::UnqualifiedAdapterOrEffectiveVersion,
            });
        }
    };
    let registry = super::adapters::registry::AdapterRegistry::baseline();
    let expected = registry.effective_profile(path, &base.adapter_id)?;
    let mut producer = base.clone();
    producer.options.remove("index_detail");
    if producer != expected {
        return Ok(Selection::LegacyFallback {
            retained: base.clone(),
            reason: FallbackReason::UnqualifiedAdapterOrEffectiveVersion,
        });
    }
    let retained = AppliedRetention::bind(&producer, target, family)?;
    Ok(Selection::Qualified { producer, retained })
}
/// Unwrap only an exact qualified retained profile for the full raw producer.
pub fn producer_profile(path: &str, retained: &ExtractionProfile) -> Result<ExtractionProfile> {
    retained.identity()?;
    if retained.contract_version != airk_core::retention::PROFILE_CONTRACT {
        return Ok(retained.clone());
    }
    let AppliedRetention::Selected {
        profile: target, ..
    } = AppliedRetention::from_profile(retained)?
    else {
        unreachable!()
    };
    let mut base = retained.clone();
    // The latest payload contract is not every adapter's producer contract.
    // Selection accepts only the exact registered producer; reconstruct that
    // contract before comparing every remaining identity field below.
    base.contract_version = super::adapters::registry::AdapterRegistry::baseline()
        .effective_profile(path, &retained.adapter_id)?
        .contract_version;
    for key in ["retained_profile", "retained_family", "retention_algorithm"] {
        base.options.remove(key);
    }
    let Selection::Qualified {
        producer,
        retained: expected,
    } = select_profile(path, &base, target)?
    else {
        anyhow::bail!("retained producer is no longer qualified");
    };
    ensure!(
        expected == *retained,
        "retained family or producer identity differs"
    );
    Ok(producer)
}
/// Called after the existing semantic projection capture and before continuity/chunk construction.
/// Keeps the same text, locations, provenance, parser diagnostics and semantic projection.
/// The producer still constructs full raw facts in this foundation; early construction is separate.
pub fn select_payload(payload: &mut ExtractionPayload, profile: &ExtractionProfile) -> Result<()> {
    profile.identity()?;
    let AppliedRetention::Selected {
        profile: selection,
        family,
        ..
    } = AppliedRetention::from_profile(profile)?
    else {
        anyhow::bail!("expected explicit new retained profile");
    };
    super::detail::select_retained(payload, selection, family)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_effective_version_qualifies_and_unrelated_or_ocr_stays_legacy() -> Result<()> {
        let registry = crate::adapters::registry::AdapterRegistry::baseline();
        let base = registry.effective_profile("src/lib.rs", "rust")?;
        let Selection::Qualified { producer, retained } =
            select_profile("src/lib.rs", &base, RetainedProfile::Lean)?
        else {
            panic!("sampled rust not qualified");
        };
        assert_eq!(producer, base);
        assert_eq!(producer_profile("src/lib.rs", &retained)?, base);
        assert_ne!(retained.identity()?, base.identity()?);
        for (path, adapter) in [
            ("Cargo.toml", "project-manifest"),
            ("notes.md", "markdown"),
            ("AGENTS.md", "instruction-markdown"),
            ("notes.docx", "docx"),
        ] {
            let original = registry.effective_profile(path, adapter)?;
            for target in [RetainedProfile::Lean, RetainedProfile::Rich] {
                let Selection::Qualified { retained, .. } =
                    select_profile(path, &original, target)?
                else {
                    anyhow::bail!("first-wave producer did not qualify: {adapter}");
                };
                assert_eq!(producer_profile(path, &retained)?, original);
                let mut changed = retained;
                changed.chunk_version += 1;
                assert!(producer_profile(path, &changed).is_err());
            }
        }
        let mut changed = base;
        changed.mapping_version += 1;
        assert!(matches!(
            select_profile("src/lib.rs", &changed, RetainedProfile::Lean)?,
            Selection::LegacyFallback { .. }
        ));
        let unsupported = registry.effective_profile("sheet.xlsx", "xlsx")?;
        let Selection::LegacyFallback { retained, .. } =
            select_profile("sheet.xlsx", &unsupported, RetainedProfile::Rich)?
        else {
            panic!("unqualified spreadsheet");
        };
        assert_eq!(retained, unsupported);
        let mut ocr = unsupported;
        ocr.adapter_id = "ocr".into();
        assert!(
            matches!(select_profile("scan.pdf",&ocr,RetainedProfile::Lean)?,Selection::LegacyFallback{retained,..} if retained==ocr)
        );
        Ok(())
    }
    #[test]
    fn equivalent_effective_file_reuses_without_policy_or_path_salt() -> Result<()> {
        let registry = crate::adapters::registry::AdapterRegistry::baseline();
        let base = registry.effective_profile("src/a.rs", "rust")?;
        let Selection::Qualified {
            retained: first, ..
        } = select_profile("src/a.rs", &base, RetainedProfile::Lean)?
        else {
            unreachable!()
        };
        let Selection::Qualified {
            retained: second, ..
        } = select_profile("src/b.rs", &base, RetainedProfile::Lean)?
        else {
            unreachable!()
        };
        assert_eq!(first.identity()?, second.identity()?);
        Ok(())
    }
}
