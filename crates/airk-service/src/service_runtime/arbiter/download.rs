// SPDX-License-Identifier: Apache-2.0
//! Explicit native Provision transport. Core owns every declared source.
use super::*;
use airk_core::arbiter::{DownloadOrigin, download_origin};
use airk_store::service::arbiter_assets::AssetWriter;
use http_body_util::{BodyExt, Empty};
use sha2::{Digest, Sha256};
use std::io::Write;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub(super) enum Failure {
    Source(anyhow::Error),
    Local(anyhow::Error),
    Canceled,
    Deadline,
}
impl Failure {
    pub(super) fn can_fallback(&self) -> bool {
        matches!(self, Self::Source(_))
    }
    pub(super) fn class(&self) -> &'static str {
        match self {
            Self::Source(_) => "source_failure",
            Self::Local(_) => "local_custody_or_policy_failure",
            Self::Canceled => "canceled",
            Self::Deadline => "original_part_deadline",
        }
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(error) | Self::Local(error) => write!(f, "{}: {error:#}", self.class()),
            _ => f.write_str(self.class()),
        }
    }
}
impl std::error::Error for Failure {}

pub(super) struct PartBudget {
    deadline: tokio::time::Instant,
    redirects: usize,
}
/// Minimum per-part deadline; larger parts get time for this sustained rate.
const PART_DEADLINE_FLOOR: Duration = Duration::from_secs(900);
/// Sustained transfer rate a part's deadline allows (256 KiB/s). Stalls are
/// still bounded separately by the idle timeout.
const PART_MIN_BYTES_PER_SECOND: u64 = 256 * 1024;

impl PartBudget {
    #[cfg(test)]
    pub(super) fn new() -> Self {
        Self::for_part(0)
    }
    /// One budget per catalog part, never reset across sources: 900 s, or
    /// enough time for `bytes` at 256 KiB/s when that is longer.
    pub(super) fn for_part(bytes: u64) -> Self {
        let scaled = Duration::from_secs(bytes.div_ceil(PART_MIN_BYTES_PER_SECOND));
        Self {
            deadline: tokio::time::Instant::now() + PART_DEADLINE_FLOOR.max(scaled),
            redirects: 0,
        }
    }
    pub(super) fn live(&self) -> std::result::Result<(), Failure> {
        if tokio::time::Instant::now() >= self.deadline {
            Err(Failure::Deadline)
        } else {
            Ok(())
        }
    }
    fn admit(&self, cancel: &CancellationToken) -> std::result::Result<(), Failure> {
        if cancel.is_cancelled() {
            return Err(Failure::Canceled);
        }
        self.live()
    }
    fn redirect(&mut self) -> Result<()> {
        ensure!(self.redirects < 4, "Arbiter original part redirect limit");
        self.redirects += 1;
        Ok(())
    }
}

fn hex_component(value: &str, bytes: usize) -> bool {
    value.len() == bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn approved(url: &str, initial: bool) -> Result<::http::Uri> {
    let uri: ::http::Uri = url.parse()?;
    ensure!(
        uri.scheme_str() == Some("https"),
        "Arbiter download requires HTTPS"
    );
    let authority = uri.authority().context("Arbiter download host missing")?;
    ensure!(
        !authority.as_str().contains('@')
            && authority.as_str() == uri.host().context("Arbiter download host missing")?,
        "Arbiter download authority refused"
    );
    if initial || download_origin(url).is_ok() {
        download_origin(url)?;
        return Ok(uri);
    }
    let host = uri.host().context("Arbiter download host missing")?;
    let path = uri.path();
    let github = (host == "release-assets.githubusercontent.com"
        && path.starts_with("/github-production-release-asset/"))
        || (host == "objects.githubusercontent.com"
            && path.starts_with("/github-production-release-asset-2e65be/"));
    let pieces = path.split('/').collect::<Vec<_>>();
    let hf_cdn = host == "us.aws.cdn.hf.co"
        && pieces.len() == 4
        && pieces[1] == "xet-bridge-us"
        && hex_component(pieces[2], 24)
        && hex_component(pieces[3], 64);
    let hf_cache = if host == "huggingface.co" {
        path.strip_prefix("/api/resolve-cache/models/")
            .is_some_and(|suffix| {
                let parts = suffix.split('/').collect::<Vec<_>>();
                parts.len() == 4
                    && download_origin(&format!(
                        "https://huggingface.co/{}/{}/resolve/{}/{}",
                        parts[0], parts[1], parts[2], parts[3]
                    ))
                    .is_ok()
            })
    } else {
        false
    };
    ensure!(
        github || hf_cdn || hf_cache,
        "Arbiter download redirected outside approved hosts/routes"
    );
    Ok(uri)
}
fn redirect_url(original: &str, location: &str) -> Result<String> {
    let origin = download_origin(original)?;
    let next = if location.starts_with('/') && !location.starts_with("//") {
        ensure!(
            origin == DownloadOrigin::HuggingFace,
            "Arbiter relative release redirect refused"
        );
        format!("https://huggingface.co{location}")
    } else {
        location.to_owned()
    };
    let uri = approved(&next, false)?;
    match origin {
        DownloadOrigin::GitHubRelease => ensure!(
            uri.host().is_some_and(|host| matches!(
                host,
                "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
            )),
            "Arbiter release redirect crossed source origin"
        ),
        DownloadOrigin::HuggingFace => {
            ensure!(
                uri.host()
                    .is_some_and(|host| matches!(host, "huggingface.co" | "us.aws.cdn.hf.co")),
                "Arbiter HF redirect crossed source origin"
            );
            if uri.host() == Some("huggingface.co") {
                let original_uri: ::http::Uri = original.parse()?;
                let original_path = original_uri.path();
                let suffix = original_path
                    .strip_prefix('/')
                    .context("HF original path")?
                    .replace("/resolve/", "/");
                ensure!(
                    uri.path() == original_path
                        || uri.path() == format!("/api/resolve-cache/models/{suffix}"),
                    "Arbiter HF redirect changed immutable source"
                );
            }
        }
    }
    Ok(next)
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
    let request = hyper::Request::builder()
        .method("GET")
        .uri(
            uri.path_and_query()
                .context("download path missing")?
                .as_str(),
        )
        .header("Host", host)
        .header("User-Agent", "AIRK-Arbiter-native")
        .header("Accept-Encoding", "identity")
        .body(Empty::<bytes::Bytes>::new())?;
    Ok((sender.send_request(request).await?, connection))
}

#[cfg(test)]
pub(super) async fn consume<B>(
    body: B,
    part: &AssetPart,
    writer: &mut AssetWriter,
) -> std::result::Result<(), Failure>
where
    B: http_body::Body<Data = bytes::Bytes> + Unpin,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    consume_checked(body, part, writer, || Ok(())).await
}
async fn consume_checked<B>(
    mut body: B,
    part: &AssetPart,
    writer: &mut AssetWriter,
    check: impl Fn() -> std::result::Result<(), Failure> + Send + Sync,
) -> std::result::Result<(), Failure>
where
    B: http_body::Body<Data = bytes::Bytes> + Unpin,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let mut bytes = 0_u64;
    let mut hash = Sha256::new();
    loop {
        check()?;
        let Some(frame) = tokio::time::timeout(Duration::from_secs(30), body.frame())
            .await
            .context("Arbiter asset idle deadline")
            .map_err(Failure::Source)?
        else {
            break;
        };
        check()?;
        let frame = frame.map_err(|error| Failure::Source(anyhow::Error::new(error)))?;
        let data = frame
            .into_data()
            .map_err(|_| Failure::Source(anyhow::anyhow!("Arbiter asset trailers refused")))?;
        bytes = bytes
            .checked_add(u64::try_from(data.len()).map_err(|error| Failure::Source(error.into()))?)
            .context("Arbiter asset byte overflow")
            .map_err(Failure::Source)?;
        if bytes > part.bytes {
            return Err(Failure::Source(anyhow::anyhow!(
                "Arbiter part exceeds approved bytes"
            )));
        }
        hash.update(&data);
        check()?;
        writer
            .write_all(&data)
            .map_err(|error| Failure::Local(error.into()))?;
    }
    check()?;
    if bytes != part.bytes
        || airk_core::identity::Sha256Digest::from_bytes(hash.finalize().into()) != part.sha256
    {
        return Err(Failure::Source(anyhow::anyhow!(
            "Arbiter part digest differs"
        )));
    }
    check()?;
    Ok(())
}
pub(super) async fn part(
    part: &AssetPart,
    ordinal: usize,
    budget: &mut PartBudget,
    writer: &mut AssetWriter,
    cancel: &CancellationToken,
) -> std::result::Result<(), Failure> {
    part.validate().map_err(Failure::Local)?;
    let original = part
        .source_url(ordinal)
        .context("Arbiter complete source ordinal absent")
        .map_err(Failure::Local)?;
    approved(original, true).map_err(Failure::Local)?;
    budget.admit(cancel)?;
    let deadline = budget.deadline;
    let work = async {
        let mut url = original.to_owned();
        loop {
            budget.admit(cancel)?;
            let (response, connection) =
                tokio::time::timeout(Duration::from_secs(30), response(&url))
                    .await
                    .context("Arbiter response deadline")
                    .map_err(Failure::Source)?
                    .map_err(Failure::Source)?;
            if response.status().is_redirection() {
                budget.redirect().map_err(Failure::Source)?;
                let next = response
                    .headers()
                    .get(::http::header::LOCATION)
                    .context("Arbiter redirect missing location")
                    .map_err(Failure::Source)?
                    .to_str()
                    .context("Arbiter redirect location encoding")
                    .map_err(Failure::Source)?;
                url = redirect_url(original, next).map_err(Failure::Source)?;
                drop(response);
                drop(connection);
                continue;
            }
            if response.status() != ::http::StatusCode::OK {
                return Err(Failure::Source(anyhow::anyhow!(
                    "Arbiter asset HTTP status {}",
                    response.status()
                )));
            }
            if response
                .headers()
                .get(::http::header::CONTENT_ENCODING)
                .is_some_and(|value| value != "identity")
            {
                return Err(Failure::Source(anyhow::anyhow!(
                    "Arbiter compressed transport refused"
                )));
            }
            let declared = response
                .headers()
                .get(::http::header::CONTENT_LENGTH)
                .context("Arbiter asset length missing")
                .map_err(Failure::Source)?
                .to_str()
                .context("Arbiter asset length encoding")
                .map_err(Failure::Source)?
                .parse::<u64>()
                .context("Arbiter asset length invalid")
                .map_err(Failure::Source)?;
            if declared != part.bytes {
                return Err(Failure::Source(anyhow::anyhow!(
                    "Arbiter part Content-Length differs"
                )));
            }
            consume_checked(response.into_body(), part, writer, || budget.admit(cancel)).await?;
            drop(connection);
            return Ok(());
        }
    };
    tokio::select! {biased;
        _ = cancel.cancelled() => Err(Failure::Canceled),
        result = tokio::time::timeout_at(deadline, work) => result.unwrap_or(Err(Failure::Deadline)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct CancelOnData {
        cancel: CancellationToken,
        sent: bool,
    }
    impl http_body::Body for CancelOnData {
        type Data = bytes::Bytes;
        type Error = std::convert::Infallible;
        fn poll_frame(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<std::result::Result<http_body::Frame<Self::Data>, Self::Error>>>
        {
            let body = self.get_mut();
            if body.sent {
                return std::task::Poll::Ready(None);
            }
            body.sent = true;
            body.cancel.cancel();
            std::task::Poll::Ready(Some(Ok(http_body::Frame::data(bytes::Bytes::from_static(
                b"hello",
            )))))
        }
    }
    /// A real body can transfer a prefix and then stop producing frames. The
    /// virtual clock exercises the existing idle bound without wall-clock sleeps.
    struct StalledAfterData(bool);
    impl http_body::Body for StalledAfterData {
        type Data = bytes::Bytes;
        type Error = std::convert::Infallible;
        fn poll_frame(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<std::result::Result<http_body::Frame<Self::Data>, Self::Error>>>
        {
            if std::mem::replace(&mut self.get_mut().0, true) {
                std::task::Poll::Pending
            } else {
                std::task::Poll::Ready(Some(Ok(http_body::Frame::data(bytes::Bytes::from_static(
                    b"hello",
                )))))
            }
        }
    }
    #[test]
    fn native_emitted_canonical_source_cases_enter_transport_without_invented_backups() -> Result<()>
    {
        let cases: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../../../fixtures/split/arbiter/download_origin_cases.json"
        ))?;
        for name in [
            "hf_primary_no_backup",
            "hf_primary_actual_same_byte_hf_backup",
            "hf_primary_configured_gh_backup_structural_only",
        ] {
            let row = &cases[name];
            let part: AssetPart = serde_json::from_value(row["part"].clone())?;
            let count = usize::try_from(
                row["source_count"]
                    .as_u64()
                    .context("Native source count absent")?,
            )?;
            for ordinal in 0..count {
                approved(
                    part.source_url(ordinal).context("Native source absent")?,
                    true,
                )?;
            }
            assert!(part.source_url(count).is_none());
        }
        assert_eq!(cases["network_executed"], false);
        assert_eq!(cases["qualification"], false);
        assert_eq!(
            cases["hf_primary_configured_gh_backup_structural_only"]["working_gh_backup_asserted"],
            false
        );
        Ok(())
    }
    #[test]
    fn canonical_hf_sources_and_only_observed_origin_bound_redirects_are_accepted() -> Result<()> {
        let source = "https://huggingface.co/Qwen/Qwen3-1.7B/resolve/70d244cc86ccca08cf5af4e1e306ecf908b1ad5e/tokenizer.json";
        approved(source, true)?;
        let cache = "/api/resolve-cache/models/Qwen/Qwen3-1.7B/70d244cc86ccca08cf5af4e1e306ecf908b1ad5e/tokenizer.json?etag=opaque";
        redirect_url(source, cache)?;
        redirect_url(
            source,
            "https://us.aws.cdn.hf.co/xet-bridge-us/680da7517bcb5198014ad394/6aec39639a0a2d1ca966356b8c2b8426a484f80ff80731f44fa8482040713bdf?signature=opaque",
        )?;
        for bad in [
            "https://cdn.huggingface.co/anything",
            "https://us.aws.cdn.hf.co/arbitrary/object?signature=opaque",
            "https://huggingface.co/api/resolve-cache/models/Qwen/Qwen3-4B/1cfa9a7208912126459214e8b04321603b3df60c/tokenizer.json",
            "https://github.com/iokaio/airk/releases/download/v1.9.0/tokenizer.json",
            "//evil.invalid/tokenizer.json",
        ] {
            assert!(redirect_url(source, bad).is_err(), "{bad}");
        }
        assert!(approved(cache, true).is_err());
        assert!(
            redirect_url(
                "https://github.com/iokaio/airk/releases/download/v1.9.0/model.part01",
                source
            )
            .is_err()
        );
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn large_part_deadline_allows_the_minimum_sustained_rate() -> Result<()> {
        // A full 1 GiB catalog part at 256 KiB/s needs 4096 s; small parts keep 900 s.
        let large = PartBudget::for_part(1 << 30);
        let small = PartBudget::for_part(1 << 20);
        tokio::time::advance(Duration::from_secs(4095)).await;
        large.live()?;
        assert!(matches!(small.live(), Err(Failure::Deadline)));
        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(matches!(large.live(), Err(Failure::Deadline)));
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn part_deadline_and_redirect_budget_are_never_reset_for_backup() -> Result<()> {
        let mut budget = PartBudget::new();
        let original = budget.deadline;
        let cancel = CancellationToken::new();
        budget.admit(&cancel)?;
        let already_canceled = CancellationToken::new();
        already_canceled.cancel();
        assert!(matches!(
            budget.admit(&already_canceled),
            Err(Failure::Canceled)
        ));
        assert_eq!(budget.deadline, original);
        for _ in 0..4 {
            budget.redirect()?;
        }
        assert!(budget.redirect().is_err());
        assert_eq!(budget.redirects, 4);
        tokio::time::advance(Duration::from_secs(899)).await;
        budget.live()?;
        assert_eq!(budget.deadline, original);
        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(matches!(budget.live(), Err(Failure::Deadline)));
        assert_eq!(budget.deadline, original);
        assert!(Failure::Source(anyhow::anyhow!("network")).can_fallback());
        assert!(!Failure::Local(anyhow::anyhow!("custody")).can_fallback());
        assert!(!Failure::Canceled.can_fallback());
        assert!(!Failure::Deadline.can_fallback());
        Ok(())
    }
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
    #[tokio::test(start_paused = true)]
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
        // This is a Store/stream boundary fixture, not a qualified model.
        profile.qualification = None;
        profile.stage = Some(airk_core::arbiter::Stage::Context);
        profile.model = airk_core::arbiter::ModelChoice::ContextQwenCompact;
        profile.tasks = std::collections::BTreeSet::from([airk_core::arbiter::Task::ContextAction]);
        let task_policy = airk_retrieval::arbiter::context_action::policy_digest()?;
        profile.task_policies = std::collections::BTreeMap::from([(
            airk_core::arbiter::Task::ContextAction,
            task_policy,
        )]);
        profile.abi = super::super::failure_tests::structural_context_abi();
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
            backup_urls: vec![],
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
        let mut stopped = transaction
            .asset(&profile.assets[3])?
            .context("missing cancellation stage")?;
        let cancel = CancellationToken::new();
        let budget = PartBudget::new();
        let result = consume_checked(
            CancelOnData {
                cancel: cancel.clone(),
                sent: false,
            },
            &first,
            &mut stopped,
            || budget.admit(&cancel),
        )
        .await;
        assert!(matches!(result, Err(Failure::Canceled)));
        drop(stopped);
        let untouched = root
            .path()
            .join("arbiter-v3/assets")
            .join(profile.manifest.to_string())
            .join("stage-1")
            .join(&profile.assets[3].name);
        assert_eq!(std::fs::metadata(untouched)?.len(), 0);
        let mut stalled = transaction
            .asset(&profile.assets[4])?
            .context("missing stalled stage")?;
        let whole = AssetPart {
            bytes: 10,
            sha256: airk_core::identity::Sha256Digest::from_bytes(
                Sha256::digest(b"helloworld").into(),
            ),
            ..first.clone()
        };
        let idle_started = tokio::time::Instant::now();
        let failure = consume(StalledAfterData(false), &whole, &mut stalled)
            .await
            .expect_err("stalled partial body must expire");
        assert!(matches!(failure, Failure::Source(_)));
        assert_eq!(idle_started.elapsed(), Duration::from_secs(30));
        assert!(transaction.publish(stalled).is_err());
        drop(transaction);
        assert_eq!(store.phase(&profile)?, Some(Phase::Failed));
        assert!(store.pin(&profile).is_err());
        let path = root
            .path()
            .join("arbiter-v3/assets")
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
