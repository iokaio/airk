// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn native_refusal_provenance_binds_all_diagnostics_and_direction_valid_entrypoints() -> Result<()> {
    let digest = |value| Sha256Digest::from_bytes([value; 32]);
    for read_hidden in [false, true] {
        for write_hidden in [false, true] {
            let provenance = NativeRefusalProvenance {
                native_entrypoints: NativeRefusalEntrypoints {
                    read: if read_hidden {
                        NativeReadRefusalEntrypoint::HiddenUpgradeProbe
                    } else {
                        NativeReadRefusalEntrypoint::ServiceRequestsList
                    },
                    write: if write_hidden {
                        NativeWriteRefusalEntrypoint::HiddenUpgradeMigrate
                    } else {
                        NativeWriteRefusalEntrypoint::ServiceRequestsForget
                    },
                },
                read: NativeRefusalDiagnostic {
                    initial_admission_pending_digest: read_hidden.then(|| digest(2)),
                    final_classified_diagnostic_digest: digest(3),
                },
                write: NativeRefusalDiagnostic {
                    initial_admission_pending_digest: write_hidden.then(|| digest(4)),
                    final_classified_diagnostic_digest: digest(5),
                },
            };
            let proof = RuntimeRefusal::from_native(digest(1), provenance.clone())?;
            proof.validate()?;
            assert_ne!(proof.read_refusal_digest, proof.write_refusal_digest);
            assert_eq!(
                serde_json::from_slice::<RuntimeRefusal>(&serde_json::to_vec(&proof)?)?,
                proof
            );
            let mut changed = proof.clone();
            changed.payload_identity = digest(6);
            assert!(changed.validate().is_err());
            if read_hidden {
                let mut changed = proof.clone();
                changed
                    .native_provenance
                    .as_mut()
                    .unwrap()
                    .read
                    .initial_admission_pending_digest = Some(digest(10));
                assert!(changed.validate().is_err());
            }
            let mut changed = proof.clone();
            changed
                .native_provenance
                .as_mut()
                .unwrap()
                .read
                .final_classified_diagnostic_digest = digest(7);
            assert!(changed.validate().is_err());
            let mut changed = proof.clone();
            changed
                .native_provenance
                .as_mut()
                .unwrap()
                .write
                .final_classified_diagnostic_digest = digest(8);
            assert!(changed.validate().is_err());
            let mut changed = provenance.clone();
            changed.read.initial_admission_pending_digest =
                if read_hidden { None } else { Some(digest(9)) };
            assert!(RuntimeRefusal::from_native(digest(1), changed).is_err());
            let mut changed = provenance;
            changed.write.initial_admission_pending_digest =
                if write_hidden { None } else { Some(digest(9)) };
            assert!(RuntimeRefusal::from_native(digest(1), changed).is_err());
        }
    }
    Ok(())
}

#[test]
fn legacy_refusal_without_provenance_retains_exact_original_bytes_and_digests() -> Result<()> {
    let bytes = format!(
        "{{\"payload_identity\":\"{}\",\"read_refusal_digest\":\"{}\",\"write_refusal_digest\":\"{}\"}}",
        Sha256Digest::from_bytes([1; 32]),
        Sha256Digest::from_bytes([2; 32]),
        Sha256Digest::from_bytes([3; 32])
    );
    let proof: RuntimeRefusal = serde_json::from_str(&bytes)?;
    assert!(proof.native_provenance.is_none());
    proof.validate()?;
    assert_eq!(serde_json::to_vec(&proof)?, bytes.as_bytes());
    Ok(())
}
