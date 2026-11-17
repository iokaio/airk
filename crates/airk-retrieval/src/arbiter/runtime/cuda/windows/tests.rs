// SPDX-License-Identifier: Apache-2.0
//! State/closure ordering only; no protected file or actual build proof is created.
use super::*;
#[test]
fn intermediate_source_recheck_preserves_later_unread_mapping_until_outer_completion() -> Result<()>
{
    use super::super::provenance::projection::Source;
    let root = std::env::temp_dir();
    let sha = Sha256Digest::from_bytes([1; 32]);
    let later = super::super::MetadataCustodyCopy {
        original: airk_core::arbiter::resource_policy::ResourceFilePin {
            path: root
                .join("later-legal-original.txt")
                .to_str()
                .unwrap()
                .into(),
            sha256: sha,
        },
        copy: airk_core::arbiter::resource_policy::ResourceFilePin {
            path: root
                .join("later-legal-protected-copy.txt")
                .to_str()
                .unwrap()
                .into(),
            sha256: sha,
        },
        bytes: 3,
    };
    let source = ProjectionSource {
        deadline: Instant::now() + std::time::Duration::from_secs(120),
        files: std::cell::RefCell::new(BTreeMap::new()),
        custody: super::super::custody::Custody::new(std::slice::from_ref(&later))?,
        historical: super::super::custody::HistoricalTools::new(&[])?,
    };
    // Shared checkpoint helper may rehash its currently held files; later legal
    // mappings have not been consumed. This failed in the original R1 source.
    source.recheck()?;
    assert!(source.custody.recheck().is_err());
    // Pure consumption bookkeeping here proves order only, never file custody.
    source.custody.consumed(&later.original)?;
    source.recheck()?;
    source.custody.recheck()?;
    source.historical.recheck()?;
    Ok(())
}
