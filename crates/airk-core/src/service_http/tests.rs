// SPDX-License-Identifier: Apache-2.0
use super::*;

fn endpoint() -> EndpointDescriptor {
    EndpointDescriptor {
        schema: HTTP_CONTRACT_VERSION,
        installation: Sha256Digest::from_bytes([1; 32]),
        user: "501".into(),
        namespace: Incarnation::from_bytes([2; 16]),
        generation: 1,
        service_instance: Incarnation::from_bytes([3; 16]),
        executable_sha256: Sha256Digest::from_bytes([4; 32]),
        url: LoopbackMcpUrl::new(PREFERRED_HTTP_PORT).unwrap(),
        ca_sha256: Sha256Digest::from_bytes([5; 32]),
        certificate_sha256: Sha256Digest::from_bytes([6; 32]),
        state: EndpointState::Ready,
        predecessor_digest: None,
    }
}

#[test]
fn loopback_urls_reject_redirect_and_authority_ambiguities() {
    for invalid in [
        "http://127.0.0.1:57421/mcp",
        "https://localhost:57421/mcp",
        "https://127.0.0.2:57421/mcp",
        "https://[::1]:57421/mcp",
        "https://127.0.0.1:0/mcp",
        "https://127.0.0.1:057421/mcp",
        "https://127.0.0.1:65536/mcp",
        "https://127.0.0.1:+443/mcp",
        "https://127.0.0.1:443/mcp?token=secret",
        "https://127.0.0.1:443/mcp#fragment",
        "https://user@127.0.0.1:443/mcp",
        "https://127.0.0.1:443/mcp/../api",
    ] {
        assert!(invalid.parse::<LoopbackMcpUrl>().is_err(), "{invalid}");
    }
    for port in [1, 443, PREFERRED_HTTP_PORT, u16::MAX] {
        let url = LoopbackMcpUrl::new(port).unwrap();
        assert_eq!(url.to_string().parse::<LoopbackMcpUrl>().unwrap(), url);
        assert_eq!(url.port(), port);
    }
}

#[test]
fn descriptor_rejects_unknown_versions_fields_and_oversize_before_use() -> Result<()> {
    let original = endpoint();
    assert_eq!(
        EndpointDescriptor::decode(&serde_json::to_vec(&original)?)?,
        original
    );
    let mut invalid = serde_json::to_value(&original)?;
    invalid["schema"] = 2.into();
    assert!(EndpointDescriptor::decode(&serde_json::to_vec(&invalid)?).is_err());
    invalid["schema"] = 1.into();
    invalid["token"] = "must-not-be-in-a-descriptor".into();
    assert!(EndpointDescriptor::decode(&serde_json::to_vec(&invalid)?).is_err());
    assert!(EndpointDescriptor::decode(&vec![b' '; MAX_ENDPOINT_BYTES + 1]).is_err());
    let mut relocated = original.clone();
    relocated.url = LoopbackMcpUrl::new(57422)?;
    assert_ne!(original.digest()?, relocated.digest()?);
    relocated.generation = 0;
    assert!(relocated.validate().is_err());
    Ok(())
}

/// Local trust plan C: an endpoint record an HTTPS build wrote (fixed bytes,
/// with the CA and leaf digests it carried) decodes as legacy: its port,
/// generation and binding carry forward, its digest is that of its bytes.
#[test]
fn an_old_https_endpoint_record_decodes_as_legacy() -> Result<()> {
    let old = format!(
        concat!(
            r#"{{"schema":1,"installation":"{a}","user":"501","namespace":"{n}","#,
            r#""generation":7,"service_instance":"{n}","executable_sha256":"{a}","#,
            r#""url":"https://127.0.0.1:57423/mcp","ca_sha256":"{c}","#,
            r#""certificate_sha256":"{c}","state":"ready","predecessor_digest":null}}"#
        ),
        a = "01".repeat(32),
        c = "05".repeat(32),
        n = Incarnation::from_bytes([2; 16]),
    );
    let legacy = LegacyEndpoint::decode(old.as_bytes())?;
    assert_eq!(legacy.port, 57423);
    assert_eq!(legacy.generation, 7);
    assert_eq!(legacy.user, "501");
    assert_eq!(legacy.namespace, Incarnation::from_bytes([2; 16]));
    assert_eq!(
        legacy.digest,
        crate::identity::framed_digest(b"airk-http-endpoint/1", &[old.as_bytes()])
    );
    let saved = SavedEndpoint::decode(old.as_bytes())?;
    assert_eq!(saved.port(), 57423);
    assert_eq!(saved.generation(), 7);
    assert_eq!(saved.digest()?, legacy.digest);
    // Not an HTTPS URL, a zero generation or another schema: not legacy.
    for (from, to) in [
        ("https://127.0.0.1:57423/mcp", "https://localhost:57423/mcp"),
        (r#""generation":7"#, r#""generation":0"#),
        (r#"{"schema":1"#, r#"{"schema":2"#),
    ] {
        assert!(
            LegacyEndpoint::decode(old.replace(from, to).as_bytes()).is_err(),
            "{to}"
        );
    }
    assert_eq!(
        LoopbackMcpUrl::legacy_https_port("https://127.0.0.1:443/mcp"),
        Some(443)
    );
    for invalid in [
        "https://127.0.0.1:0/mcp",
        "https://127.0.0.1:0443/mcp",
        "https://127.0.0.1:443/mcp?x",
        "http://127.0.0.1:443/mcp",
    ] {
        assert_eq!(
            LoopbackMcpUrl::legacy_https_port(invalid),
            None,
            "{invalid}"
        );
    }
    Ok(())
}
