// SPDX-License-Identifier: Apache-2.0
//! Official release HTTP transport. Only compiled Core part declarations enter.
use super::*;
use airk_store::service::arbiter_assets::AssetWriter;
use http_body_util::{BodyExt, Empty};
use sha2::{Digest, Sha256};
use std::io::Write;
use tokio_util::sync::CancellationToken;

fn approved(url: &str, initial: bool) -> Result<::http::Uri> {
    let uri: ::http::Uri = url.parse()?;
    ensure!(
        uri.scheme_str() == Some("https"),
        "Arbiter download requires HTTPS"
    );
    let authority = uri.authority().context("Arbiter download host missing")?;
    ensure!(
        !authority.as_str().contains('@') && uri.port_u16().is_none_or(|port| port == 443),
        "Arbiter download authority refused"
    );
    let host = uri.host().context("Arbiter download host missing")?;
    let path = uri.path();
    let github = host == "github.com"
        && path.starts_with("/iokaio/airk/releases/download/")
        && uri.query().is_none();
    let cdn = !initial
        && ((host == "release-assets.githubusercontent.com"
            && path.starts_with("/github-production-release-asset/"))
            || (host == "objects.githubusercontent.com"
                && path.starts_with("/github-production-release-asset-2e65be/")));
    ensure!(
        github || cdn,
        "Arbiter release download redirected outside approved hosts"
    );
    Ok(uri)
}
struct Connection(tokio::task::JoinHandle<()>);
impl Drop for Connection {
    fn drop(&mut self) {
        self.0.abort();
    }
}
async fn response(url: &str) -> Result<(hyper::Response<hyper::body::Incoming>, Connection)> {
    let uri = approved(url, false)?;
    let host = uri.host().context("download host missing")?.to_owned();
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let socket = tokio::net::TcpStream::connect((host.as_str(), 443)).await?;
    let tls = tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(
            rustls::pki_types::ServerName::try_from(host.clone())?,
            socket,
        )
        .await?;
    let (mut sender, connection) =
        hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(tls)).await?;
    let connection = Connection(tokio::spawn(async move {
        let _ = connection.await;
    }));
    let target = uri
        .path_and_query()
        .context("download path missing")?
        .as_str();
    let request = hyper::Request::builder()
        .method("GET")
        .uri(target)
        .header("Host", host)
        .header("User-Agent", "AIRK-Arbiter-native")
        .header("Accept-Encoding", "identity")
        .body(Empty::<bytes::Bytes>::new())?;
    Ok((sender.send_request(request).await?, connection))
}

async fn consume<B>(mut body: B, part: &AssetPart, writer: &mut AssetWriter) -> Result<()>
where
    B: http_body::Body<Data = bytes::Bytes> + Unpin,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let mut bytes = 0_u64;
    let mut hash = Sha256::new();
    while let Some(frame) = tokio::time::timeout(Duration::from_secs(30), body.frame())
        .await
        .context("Arbiter asset idle deadline")?
    {
        let frame = frame?;
        let data = frame
            .into_data()
            .map_err(|_| anyhow::anyhow!("Arbiter asset trailers refused"))?;
        bytes = bytes
            .checked_add(u64::try_from(data.len())?)
            .context("Arbiter asset byte overflow")?;
        ensure!(bytes <= part.bytes, "Arbiter part exceeds approved bytes");
        hash.update(&data);
        writer.write_all(&data)?;
    }
    ensure!(
        bytes == part.bytes
            && airk_core::identity::Sha256Digest::from_bytes(hash.finalize().into()) == part.sha256,
        "Arbiter part digest differs"
    );
    Ok(())
}
pub(super) async fn part(
    part: &AssetPart,
    writer: &mut AssetWriter,
    cancel: &CancellationToken,
) -> Result<()> {
    approved(&part.url, true)?;
    ensure!(
        part.bytes > 0 && part.bytes <= 1_u64 << 30,
        "Arbiter download part bound"
    );
    let work = async {
        let mut url = part.url.clone();
        for redirects in 0..=4 {
            let (response, connection) =
                tokio::time::timeout(Duration::from_secs(30), response(&url))
                    .await
                    .context("Arbiter response deadline")??;
            if response.status().is_redirection() {
                ensure!(redirects < 4, "Arbiter redirect limit");
                let next = response
                    .headers()
                    .get(::http::header::LOCATION)
                    .context("Arbiter redirect missing location")?
                    .to_str()?
                    .to_owned();
                approved(&next, false)?;
                drop(response);
                drop(connection);
                url = next;
                continue;
            }
            ensure!(
                response.status() == ::http::StatusCode::OK,
                "Arbiter asset HTTP status {}",
                response.status()
            );
            ensure!(
                response
                    .headers()
                    .get(::http::header::CONTENT_ENCODING)
                    .is_none_or(|v| v == "identity"),
                "Arbiter compressed transport refused"
            );
            let declared = response
                .headers()
                .get(::http::header::CONTENT_LENGTH)
                .context("Arbiter asset length missing")?
                .to_str()?
                .parse::<u64>()?;
            ensure!(
                declared == part.bytes,
                "Arbiter part Content-Length differs"
            );
            consume(response.into_body(), part, writer).await?;
            drop(connection);
            return Ok(());
        }
        anyhow::bail!("Arbiter redirect limit")
    };
    tokio::select! {biased;_ = cancel.cancelled()=>anyhow::bail!("Arbiter provisioning canceled"),result=tokio::time::timeout(Duration::from_secs(900),work)=>result.context("Arbiter part deadline")?}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn approved_release_redirects_are_exact_https_and_never_workspace_or_arbitrary_hosts()
    -> Result<()> {
        approved(
            "https://github.com/iokaio/airk/releases/download/v1.9.0/model.part01",
            true,
        )?;
        approved(
            "https://release-assets.githubusercontent.com/github-production-release-asset/1/2?sig=opaque",
            false,
        )?;
        for url in [
            "http://github.com/iokaio/airk/releases/download/v1.9.0/m",
            "https://github.com/other/airk/releases/download/v1/m",
            "https://github.com@evil.invalid/iokaio/airk/releases/download/v1/m",
            "https://release-assets.githubusercontent.com/other/1",
            "https://github.com:444/iokaio/airk/releases/download/v1/m",
            "https://example.invalid/m",
        ] {
            assert!(approved(url, false).is_err(), "{url}");
        }
        assert!(
            approved(
                "https://release-assets.githubusercontent.com/github-production-release-asset/1/2",
                true
            )
            .is_err()
        );
        Ok(())
    }
    #[tokio::test]
    async fn streamed_part_hash_and_length_failures_never_publish_logical_asset() -> Result<()> {
        use airk_store::service::{
            ServiceLease,
            arbiter_assets::{AssetStore, Phase},
        };
        let root = tempfile::tempdir()?;
        let lease = Arc::new(ServiceLease::acquire(root.path())?);
        let store = AssetStore::open(
            lease,
            airk_core::identity::Sha256Digest::from_bytes([1; 32]),
            airk_core::identity::Incarnation::from_bytes([2; 16]),
        )?;
        let mut profile: Profile = serde_json::from_slice(include_bytes!(
            "../../../../../fixtures/arbiter/profile-downloads.json"
        ))?;
        let digest = airk_core::identity::Sha256Digest::from_bytes(Sha256::digest(b"hello").into());
        profile.assets.truncate(6);
        profile.peak_disk_bytes = 1 << 20;
        for asset in &mut profile.assets {
            asset.bytes = 10;
            asset.sha256 =
                airk_core::identity::Sha256Digest::from_bytes(Sha256::digest(b"helloworld").into());
        }
        // Descriptor pins name the exact raw fixture bytes, just like the
        // logical assets; the injected part failures below remain unchanged.
        profile.compiler =
            airk_core::identity::Sha256Digest::from_bytes(Sha256::digest(b"helloworld").into());
        profile.calibration =
            airk_core::identity::Sha256Digest::from_bytes(Sha256::digest(b"helloworld").into());
        let transaction = store.begin(&profile, 1)?;
        let mut writer = transaction
            .asset(&profile.assets[0])?
            .context("missing stage")?;
        let first = AssetPart {
            url: "https://github.com/iokaio/airk/releases/download/v1.9.0/model.part01".into(),
            bytes: 5,
            sha256: digest,
        };
        consume(
            http_body_util::Full::new(bytes::Bytes::from_static(b"hello")),
            &first,
            &mut writer,
        )
        .await?;
        let mut second = first.clone();
        second.sha256 =
            airk_core::identity::Sha256Digest::from_bytes(Sha256::digest(b"world").into());
        consume(
            http_body_util::Full::new(bytes::Bytes::from_static(b"world")),
            &second,
            &mut writer,
        )
        .await?;
        transaction.publish(writer)?;
        let mut bad = transaction
            .asset(&profile.assets[1])?
            .context("missing bad stage")?;
        assert!(
            consume(
                http_body_util::Full::new(bytes::Bytes::from_static(b"wrong")),
                &first,
                &mut bad
            )
            .await
            .is_err()
        );
        assert!(transaction.publish(bad).is_err());
        let mut oversized = transaction
            .asset(&profile.assets[2])?
            .context("missing size stage")?;
        assert!(
            consume(
                http_body_util::Full::new(bytes::Bytes::from_static(b"hello!")),
                &first,
                &mut oversized
            )
            .await
            .is_err()
        );
        drop(transaction);
        assert_eq!(store.phase(&profile)?, Some(Phase::Failed));
        assert!(store.pin(&profile).is_err());
        let path = root
            .path()
            .join("arbiter-v2/assets")
            .join(profile.manifest.to_string());
        assert_eq!(
            std::fs::read(path.join(&profile.assets[0].name))?,
            b"helloworld"
        );
        assert!(!path.join(&profile.assets[1].name).exists());
        assert!(!path.join(&profile.assets[2].name).exists());
        Ok(())
    }
}
