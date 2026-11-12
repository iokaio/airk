// SPDX-License-Identifier: Apache-2.0
//! Pure signature decisions on fixture observations; every platform's rules
//! run on every host. The identity used here is a fictional fixture, not C10.
use super::*;

fn fixture_identity() -> Identity {
    Identity {
        windows_publisher: "CN=Example, O=Example, C=US".into(),
        windows_identity_eku: "1.3.6.1.4.1.311.97.1.2.3".into(),
        macos_team_id: "ABCDE12345".into(),
    }
}

fn windows_claim(identity: &Identity) -> Claim<'_> {
    Claim::Windows {
        publisher: &identity.windows_publisher,
        identity_eku: &identity.windows_identity_eku,
    }
}

fn trusted(subject: &str, ekus: &[&str], timestamped: bool) -> AuthenticodeObservation {
    AuthenticodeObservation {
        status: 0,
        signer: Some(Signer {
            subject: subject.into(),
            ekus: ekus.iter().map(|eku| (*eku).into()).collect(),
            timestamped,
        }),
    }
}

#[test]
fn the_embedded_c10_identity_parses() {
    let identity = identity().expect("scripts/signing-identity.json parses");
    assert!(identity.windows_publisher.starts_with("CN="));
    assert!(is_oid(&identity.windows_identity_eku));
    assert!(is_team_id(&identity.macos_team_id));
}

#[test]
fn an_identity_file_of_another_schema_or_shape_is_refused() {
    let file = |schema: &str, team: &str| {
        format!(
            r#"{{"schema":"{schema}","windows":{{"publisher":"CN=X","identity_eku":"1.2.3"}},"macos":{{"team_id":"{team}"}}}}"#
        )
    };
    assert!(parse_identity(&file("airk-signing-identity/1", "ABCDE12345")).is_ok());
    for text in [
        file("airk-signing-identity/2", "ABCDE12345"),
        file("airk-signing-identity/1", "abc"),
        "{}".to_owned(),
    ] {
        assert_eq!(
            parse_identity(&text).unwrap_err().code,
            IDENTITY_UNAVAILABLE
        );
    }
}

#[test]
fn each_claim_requires_its_platform_check_and_only_the_c10_identity() {
    let identity = fixture_identity();
    assert_eq!(
        required_check(windows_claim(&identity), Host::Windows, &identity),
        Ok(Check::Authenticode {
            publisher: identity.windows_publisher.clone(),
            identity_eku: identity.windows_identity_eku.clone(),
        })
    );
    assert_eq!(
        required_check(
            Claim::Macos {
                team_id: "ABCDE12345"
            },
            Host::Macos,
            &identity
        ),
        Ok(Check::Codesign {
            team_id: "ABCDE12345".into()
        })
    );
    let linux = Claim::Linux {
        signed: false,
        integrity: "record-sha256",
    };
    assert_eq!(
        required_check(linux, Host::Linux, &identity),
        Ok(Check::DigestOnly)
    );
    let refused = |claim, host| required_check(claim, host, &identity).unwrap_err().code;
    // Another identity on the right platform.
    assert_eq!(
        refused(
            Claim::Windows {
                publisher: "CN=Someone Else",
                identity_eku: &identity.windows_identity_eku,
            },
            Host::Windows
        ),
        IDENTITY_MISMATCH
    );
    assert_eq!(
        refused(
            Claim::Windows {
                publisher: &identity.windows_publisher,
                identity_eku: "1.2.3",
            },
            Host::Windows
        ),
        IDENTITY_MISMATCH
    );
    assert_eq!(
        refused(
            Claim::Macos {
                team_id: "ZZZZZ99999"
            },
            Host::Macos
        ),
        IDENTITY_MISMATCH
    );
    // A record of another platform never degrades to digest only.
    for (claim, host) in [
        (windows_claim(&identity), Host::Macos),
        (windows_claim(&identity), Host::Linux),
        (
            Claim::Macos {
                team_id: "ABCDE12345",
            },
            Host::Windows,
        ),
        (linux, Host::Windows),
        (linux, Host::Macos),
        (linux, Host::Other),
        (
            Claim::Linux {
                signed: true,
                integrity: "record-sha256",
            },
            Host::Linux,
        ),
        (
            Claim::Linux {
                signed: false,
                integrity: "none",
            },
            Host::Linux,
        ),
    ] {
        assert_eq!(
            refused(claim, host),
            PLATFORM_MISMATCH,
            "{claim:?} on {host:?}"
        );
    }
}

#[test]
fn authenticode_accepts_only_a_trusted_timestamped_c10_signer() {
    let identity = fixture_identity();
    let publisher = &identity.windows_publisher;
    let eku = &identity.windows_identity_eku;
    let verdict = |observation: &AuthenticodeObservation| {
        authenticode_verdict("airk.exe", observation, publisher, eku)
    };
    let code_signing = "1.3.6.1.5.5.7.3.3";
    assert_eq!(
        verdict(&trusted(publisher, &[code_signing, eku], true)),
        Ok(())
    );
    let refused = [
        // TRUST_E_NOSIGNATURE, TRUST_E_BAD_DIGEST (tampered), CERT_E_UNTRUSTEDROOT.
        AuthenticodeObservation {
            status: 0x800B0100_u32 as i32,
            signer: None,
        },
        AuthenticodeObservation {
            status: 0x80096010_u32 as i32,
            signer: None,
        },
        AuthenticodeObservation {
            status: 0x800B0109_u32 as i32,
            signer: None,
        },
        // Trusted, but the signer could not be read.
        AuthenticodeObservation {
            status: 0,
            signer: None,
        },
        // Another publisher (case differs: the comparison is exact).
        trusted("CN=Other, O=Other, C=US", &[eku], true),
        trusted(&publisher.to_lowercase(), &[eku], true),
        // Missing the identity EKU.
        trusted(publisher, &[code_signing], true),
        trusted(publisher, &[], true),
        // Untimestamped.
        trusted(publisher, &[eku], false),
    ];
    for observation in refused {
        assert_eq!(
            verdict(&observation).unwrap_err().code,
            SIGNATURE_UNVERIFIED,
            "{observation:?}"
        );
    }
}

#[test]
fn codesign_uses_the_team_requirement_and_its_exit_status() {
    assert_eq!(
        codesign_requirement("ABCDE12345").unwrap(),
        "=anchor apple generic and certificate leaf[subject.OU] = \"ABCDE12345\" and certificate leaf[field.1.2.840.113635.100.6.1.13] exists"
    );
    for malformed in ["", "abcde12345", "ABCDE1234", "ABCDE\" or 1", "ABCDE123456"] {
        assert_eq!(
            codesign_requirement(malformed).unwrap_err().code,
            IDENTITY_UNAVAILABLE
        );
    }
    assert_eq!(codesign_verdict("airk", true, ""), Ok(()));
    let refusal = codesign_verdict("airk", false, "airk: invalid signature\n").unwrap_err();
    assert_eq!(refusal.code, SIGNATURE_UNVERIFIED);
    assert!(refusal.message.contains("invalid signature"));
}
