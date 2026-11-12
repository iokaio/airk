// SPDX-License-Identifier: Apache-2.0
//! The native service owns the retained HTTPS socket, identity and enrollment.
use super::*;
use crate::http_endpoint::{
    self, BoundEndpoint, CredentialStore, InstallationBinding, TlsIdentity,
};
use airk_core::{
    identity::Incarnation,
    service_http::{
        EndpointDescriptor, EndpointState, HTTP_CONTRACT_VERSION, HttpConnectionInfo,
        HttpSelection, LoopbackMcpUrl,
    },
};
use std::{
    path::PathBuf,
    sync::Weak,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

pub(super) struct Owner {
    root: PathBuf,
    binding: InstallationBinding,
    descriptor: Mutex<EndpointDescriptor>,
    credentials: Arc<Mutex<CredentialStore>>,
    ca_file: PathBuf,
    tls: watch::Sender<tokio_rustls::TlsAcceptor>,
    gateway: crate::http_gateway::Gateway,
    shutdown: CancellationToken,
    checkpoint: Arc<checkpoint::Coordinator>,
    ownership: Arc<airk_store::service::ServiceLease>,
}
type StartedOwner = (Arc<Owner>, tokio::task::JoinHandle<Result<()>>);

fn now_ms() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}

impl Owner {
    pub(super) fn start(
        state: &Arc<State>,
        ownership: Arc<airk_store::service::ServiceLease>,
    ) -> Result<Option<StartedOwner>> {
        let Some(gate) = state.catalog_gate.clone() else {
            return Ok(None);
        };
        let clock = gate
            .catalog_clock()
            .ok_or_else(|| anyhow::anyhow!("HTTP catalog clock unavailable"))?;
        let root = state.control_root.join("http-1");
        local_ipc::prepare_root(&root)?;
        // This is the stable protected installation/control root, never a versioned payload path.
        let binding = http_endpoint::installation_binding(&state.control_root, clock.namespace)?;
        let previous = http_endpoint::load_descriptor(&root, &binding)?;
        let mut credential_store = CredentialStore::open(&root, binding.clone())?;
        credential_store.initialize_empty_owned(&ownership)?;
        let held = BoundEndpoint::bind(previous.as_ref().map(|value| value.url.port()))?;
        let identity = TlsIdentity::load_or_create(&root, binding.clone(), now_ms()?)?;
        let descriptor = EndpointDescriptor {
            schema: HTTP_CONTRACT_VERSION,
            installation: binding.installation,
            user: binding.user.clone(),
            namespace: binding.namespace,
            generation: previous.as_ref().map_or(Ok(1), |value| {
                value
                    .generation
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("HTTP endpoint generation exhausted"))
            })?,
            service_instance: state.instance,
            executable_sha256: state.executable_sha256.parse()?,
            url: LoopbackMcpUrl::new(held.port()?)?,
            ca_sha256: identity.ca_sha256(),
            certificate_sha256: identity.certificate_sha256(),
            state: if state.checkpoint.admission_closed()? {
                EndpointState::Recovering
            } else {
                EndpointState::Ready
            },
            predecessor_digest: previous
                .as_ref()
                .map(EndpointDescriptor::digest)
                .transpose()?,
        };
        let credentials = Arc::new(Mutex::new(credential_store));
        let (tls, receiver) = watch::channel(identity.acceptor());
        let control = state.control_root.clone();
        let weak: Weak<State> = Arc::downgrade(state);
        let validate: crate::http_gateway::ValidateRequest =
            Arc::new(move |credential, context| {
                let control = control.clone();
                let weak = weak.clone();
                Box::pin(async move {
                    let state = weak
                        .upgrade()
                        .ok_or_else(|| anyhow::anyhow!("service stopped"))?;
                    if let Ok(mut activity) = state.last_client_activity.lock() {
                        *activity = Instant::now();
                    }
                    if let Some(owner) = state.http.get() {
                        owner.update_state()?;
                    }
                    let factory = crate::http_backend::factory_attached(
                        control,
                        credential.selection,
                        credential.explorer_descriptor,
                        context,
                    )
                    .await?;
                    Ok(crate::http_backend::with_query_capacity(
                        factory,
                        state.supervisor.query_capacity_reader(),
                    ))
                })
            });
        let checkpoint = state.checkpoint.clone();
        let closed = Arc::new(move || checkpoint.admission_closed().unwrap_or(true));
        let checkpoint = state.checkpoint.clone();
        let admit: crate::http_gateway::AdmitRequest =
            Arc::new(move || Ok(Arc::new(checkpoint.admit()?)));
        let gateway = crate::http_gateway::Gateway::new(
            &descriptor,
            credentials.clone(),
            receiver,
            validate,
            crate::http_gateway::NativeAdmission {
                closed,
                gate,
                admit,
                settle: state.supervisor.read_settlement_requester(),
            },
        )?;
        gateway.set_admin(state.admin.clone());
        http_endpoint::publish_descriptor(
            &root,
            &binding,
            &descriptor,
            descriptor.predecessor_digest,
        )?;
        let owner = Arc::new(Self {
            root,
            binding,
            descriptor: Mutex::new(descriptor),
            credentials,
            ca_file: identity.ca_certificate_file().to_owned(),
            tls,
            gateway,
            shutdown: CancellationToken::new(),
            checkpoint: state.checkpoint.clone(),
            ownership: ownership.clone(),
        });
        let gateway = owner.gateway.clone();
        let shutdown = owner.shutdown.clone();
        let task = tokio::spawn(async move {
            let _ownership = ownership;
            gateway.run(held, shutdown).await
        });
        Ok(Some((owner, task)))
    }

    pub(super) fn descriptor(&self) -> Result<EndpointDescriptor> {
        self.update_state()?;
        Ok(self
            .descriptor
            .lock()
            .map_err(|_| anyhow::anyhow!("HTTP descriptor unavailable"))?
            .clone())
    }

    pub(super) fn update_state(&self) -> Result<()> {
        let mut descriptor = self
            .descriptor
            .lock()
            .map_err(|_| anyhow::anyhow!("HTTP descriptor unavailable"))?;
        let state = if self.shutdown.is_cancelled() {
            EndpointState::Stopping
        } else if self.checkpoint.admission_closed()? {
            EndpointState::Recovering
        } else {
            EndpointState::Ready
        };
        if descriptor.state != state {
            let expected = descriptor.digest()?;
            let mut next = descriptor.clone();
            next.state = state;
            next.predecessor_digest = Some(expected);
            http_endpoint::publish_descriptor(&self.root, &self.binding, &next, Some(expected))?;
            *descriptor = next;
        }
        Ok(())
    }

    pub(super) async fn enroll(
        &self,
        control: &Path,
        id: Incarnation,
        selection: HttpSelection,
        explorer_descriptor: Option<PathBuf>,
        client: airk_core::usage::ClientFamily,
        reattach_connection_id: Option<Incarnation>,
    ) -> Result<HttpConnectionInfo> {
        let context = airk_mcp::read_attempt::ReadAttemptContext::new(
            Instant::now() + Duration::from_secs(30),
            CancellationToken::new(),
        )
        .with_lifetime_guard(Arc::new(self.checkpoint.admit()?));
        let selection = crate::http_backend::validate(control, &selection, &context).await?;
        let credentials = self.credentials.clone();
        let saved = selection.clone();
        let guard = context.lifetime_guard();
        let ownership = self.ownership.clone();
        let retirement_credentials = credentials.clone();
        let retirement_guard = guard.clone();
        let retirement_ownership = ownership.clone();
        let (retired, retirement_result) = tokio::task::spawn_blocking(move || {
            let _guard = retirement_guard;
            let _ownership = retirement_ownership;
            match retirement_credentials.lock() {
                Ok(mut credentials) => match now_ms() {
                    Ok(now) => credentials.revoke_inactive(now),
                    Err(error) => (Vec::new(), Err(error)),
                },
                Err(_) => (
                    Vec::new(),
                    Err(anyhow::anyhow!("HTTP credential store unavailable")),
                ),
            }
        })
        .await?;
        // Fence even an uncertain durable mutation before permitting another
        // enrollment. Captured authentication and TLS buffers keep their old
        // delivery cells and cannot recover authority through compaction.
        for retired_id in retired {
            self.gateway.revoke(retired_id).await;
        }
        retirement_result?;
        let issued = tokio::task::spawn_blocking(move || {
            let _guard = guard;
            let _ownership = ownership;
            if let Some(descriptor) = explorer_descriptor.as_ref() {
                crate::explorer_assistance::window_plan(descriptor)?;
            }
            let mut credentials = credentials
                .lock()
                .map_err(|_| anyhow::anyhow!("HTTP credential store unavailable"))?;
            if let Some(previous) = reattach_connection_id {
                credentials.validate_reattachment(
                    previous,
                    id,
                    &saved,
                    client,
                    explorer_descriptor.as_deref(),
                    now_ms()?,
                )?;
            }
            credentials.compact_revoked()?;
            credentials.enroll_default_for_client(id, saved, explorer_descriptor, client, now_ms()?)
        })
        .await??;
        let connection = HttpConnectionInfo {
            schema: HTTP_CONTRACT_VERSION,
            connection_id: id,
            endpoint: self.descriptor()?,
            selection,
            ca_certificate_file: self
                .ca_file
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("HTTP CA path not Unicode"))?
                .into(),
            credential_file: issued
                .credential_file
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("HTTP credential path not Unicode"))?
                .into(),
        };
        connection.validate()?;
        Ok(connection)
    }

    pub(super) async fn revoke(&self, id: Incarnation) -> Result<()> {
        let credentials = self.credentials.clone();
        let guard = self.checkpoint.admit()?;
        let ownership = self.ownership.clone();
        tokio::task::spawn_blocking(move || {
            let _guard = guard;
            let _ownership = ownership;
            credentials
                .lock()
                .map_err(|_| anyhow::anyhow!("HTTP credential store unavailable"))?
                .revoke(id)
        })
        .await??;
        self.gateway.revoke(id).await;
        Ok(())
    }

    /// The caller serializes this periodic maintenance and shutdown publication.
    pub(super) fn refresh(&self) -> Result<()> {
        let identity = TlsIdentity::load_or_create(&self.root, self.binding.clone(), now_ms()?)?;
        let mut descriptor = self
            .descriptor
            .lock()
            .map_err(|_| anyhow::anyhow!("HTTP descriptor unavailable"))?;
        let state = if self.shutdown.is_cancelled() {
            EndpointState::Stopping
        } else if self.checkpoint.admission_closed()? {
            EndpointState::Recovering
        } else {
            EndpointState::Ready
        };
        if descriptor.certificate_sha256 != identity.certificate_sha256()
            || descriptor.state != state
        {
            let expected = descriptor.digest()?;
            let mut next = descriptor.clone();
            next.state = state;
            next.certificate_sha256 = identity.certificate_sha256();
            next.predecessor_digest = Some(expected);
            http_endpoint::publish_descriptor(&self.root, &self.binding, &next, Some(expected))?;
            self.tls.send_replace(identity.acceptor());
            *descriptor = next;
        }
        Ok(())
    }

    pub(super) fn stop(&self) -> Result<()> {
        self.shutdown.cancel();
        let mut descriptor = self
            .descriptor
            .lock()
            .map_err(|_| anyhow::anyhow!("HTTP descriptor unavailable"))?;
        let expected = descriptor.digest()?;
        let mut next = descriptor.clone();
        next.state = EndpointState::Stopping;
        next.predecessor_digest = Some(expected);
        http_endpoint::publish_descriptor(&self.root, &self.binding, &next, Some(expected))?;
        *descriptor = next;
        Ok(())
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}
