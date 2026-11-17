// SPDX-License-Identifier: Apache-2.0
//! Synthetic map decisions only; no protected files or successful build exists here.
use super::*;
fn row() -> MetadataCustodyCopy {
    let root = std::env::temp_dir();
    let sha = Sha256Digest::from_bytes([1; 32]);
    MetadataCustodyCopy {
        original: ResourceFilePin {
            path: root.join("original.json").to_str().unwrap().into(),
            sha256: sha,
        },
        copy: ResourceFilePin {
            path: root.join("protected-copy.json").to_str().unwrap().into(),
            sha256: sha,
        },
        bytes: 3,
    }
}
#[test]
fn custody_never_redirects_code_authentication_or_mismatched_original_pin() -> Result<()> {
    let row = row();
    let book = Custody::new(std::slice::from_ref(&row))?;
    assert!(book.mapped(&row.original.path)?);
    assert!(!book.mapped(&row.copy.path)?);
    assert!(book.resolve(&row.original, false).is_err());
    let mut wrong = row.original.clone();
    wrong.sha256 = Sha256Digest::from_bytes([2; 32]);
    assert!(book.resolve(&wrong, true).is_err());
    assert_eq!(
        book.resolve(&row.original, true)?,
        (row.copy.clone(), Some(3))
    );
    assert!(book.recheck().is_err());
    book.consumed(&row.original)?;
    book.recheck()?;
    Ok(())
}
#[test]
fn custody_refuses_duplicate_unknown_unused_changed_and_chained_mappings() -> Result<()> {
    let row = row();
    assert!(Custody::new(&[row.clone(), row.clone()]).is_err());
    let mut other = row.clone();
    other.original.path.push_str(".other");
    assert!(Custody::new(&[row.clone(), other]).is_err());
    let mut other = row.clone();
    other.original = row.copy.clone();
    other.copy.path.push_str(".other");
    assert!(Custody::new(&[row.clone(), other]).is_err());
    let mut wrong = row.clone();
    wrong.bytes = 0;
    assert!(Custody::new(&[wrong]).is_err());
    let mut wrong = row.clone();
    wrong.copy.sha256 = Sha256Digest::from_bytes([2; 32]);
    assert!(Custody::new(&[wrong]).is_err());
    let book = Custody::new(&[row])?;
    assert!(
        book.consumed(&ResourceFilePin {
            path: std::env::temp_dir()
                .join("unknown.json")
                .to_str()
                .unwrap()
                .into(),
            sha256: Sha256Digest::from_bytes([1; 32])
        })
        .is_err()
    );
    assert!(book.recheck().is_err());
    Ok(())
}
#[test]
fn custody_decoder_rejects_unknown_schema_fields_and_relative_copy() {
    assert!(decode_metadata_custody(br#"{"schema":"other","copies":[]}"#).is_err());
    assert!(
        decode_metadata_custody(
            br#"{"schema":"airk.native.cuda-metadata-custody/1","copies":[],"approved":true}"#
        )
        .is_err()
    );
    let mut row = row();
    row.copy.path = "relative.json".into();
    assert!(Custody::new(&[row]).is_err());
}

#[cfg(windows)]
#[test]
fn custody_windows_case_aliases_do_not_create_distinct_originals() -> Result<()> {
    let row = row();
    let mut other = row.clone();
    other.original.path = other.original.path.to_uppercase();
    other.copy.path.push_str(".other");
    assert!(Custody::new(&[row, other]).is_err());
    Ok(())
}

#[cfg(windows)]
fn tool_row() -> HistoricalToolCustodyCopy {
    HistoricalToolCustodyCopy{tool:"ninja".into(),original:ResourceFilePin{path:r"D:\code\Github\airk\artifacts\advanced-split\ort130-x64-owned-r3\tools\ninja-build\ninja.exe".into(),sha256:"edccb4c55fe69db51ed8ae286594660cf8e646abead40ff3c39f28c636489653".parse().unwrap()},copy:ResourceFilePin{path:std::env::temp_dir().join("ninja-historical-evidence.bin").to_str().unwrap().into(),sha256:"edccb4c55fe69db51ed8ae286594660cf8e646abead40ff3c39f28c636489653".parse().unwrap()},bytes:600576}
}
#[cfg(windows)]
#[test]
fn historical_tool_mapping_requires_exact_sealed_role_and_consumed_evidence() -> Result<()> {
    let row = tool_row();
    let tools = HistoricalTools::new(std::slice::from_ref(&row))?;
    assert!(tools.mapped(&row.original.path)?);
    assert!(
        tools
            .resolve("runtime_dll", &row.original, row.bytes, 1 << 30)
            .is_err()
    );
    assert!(
        tools
            .resolve("cmake", &row.original, row.bytes, 1 << 30)
            .is_err()
    );
    assert!(
        tools
            .resolve("ninja", &row.original, row.bytes - 1, 1 << 30)
            .is_err()
    );
    assert!(
        tools
            .resolve("ninja", &row.original, row.bytes, row.bytes - 1)
            .is_err()
    );
    assert_eq!(
        tools.resolve("ninja", &row.original, row.bytes, 1 << 30)?,
        (row.copy.clone(), Some(600576))
    );
    assert!(tools.recheck().is_err());
    tools.consumed(&row.original)?;
    tools.recheck()?;
    let metadata = MetadataCustodyCopy {
        original: row.original.clone(),
        copy: row.copy.clone(),
        bytes: row.bytes,
    };
    assert!(validate_custody_compatibility(&[metadata], std::slice::from_ref(&row)).is_err());
    let mut wrong = row.clone();
    wrong.tool = "compiler".into();
    assert!(validate_historical_tool_custody(&[wrong]).is_err());
    let mut wrong = row.clone();
    wrong.original.sha256 = Sha256Digest::from_bytes([1; 32]);
    assert!(validate_historical_tool_custody(&[wrong]).is_err());
    let mut wrong = row;
    wrong.bytes += 1;
    assert!(validate_historical_tool_custody(&[wrong]).is_err());
    Ok(())
}
#[test]
fn unknown_historical_tool_class_never_grants_an_alias() {
    let m = row();
    let h = HistoricalToolCustodyCopy {
        tool: "cuda_provider".into(),
        original: m.original,
        copy: m.copy,
        bytes: m.bytes,
    };
    assert!(validate_historical_tool_custody(&[h]).is_err());
    let tools = HistoricalTools::new(&[]).unwrap();
    assert!(tools.resolve("unknown", &row().original, 3, 3).is_err());
    assert!(!tools.mapped(&row().original.path).unwrap());
    // This exact direct nonaliased path remains the original strict authenticator's duty.
    assert_eq!(
        tools.resolve("python", &row().original, 3, 3).unwrap(),
        (row().original, None)
    );
    tools.recheck().unwrap();
    assert!(tools.consumed(&row().original).is_err());
}
