// SPDX-License-Identifier: Apache-2.0
//! The native service owns the retained loopback HTTP socket (local trust
//! plan C: plain TCP, no TLS identity). Every request is the built-in local
//! agent session (U110); the service never opens or creates a credential
//! ledger, and deletes a retired one on upgrade (local trust plan B). An
//! upgraded installation's first start reads the HTTPS build's endpoint
//! record as a legacy predecessor: it keeps the port and publishes the next
//! generation over HTTP. The old CA and TLS files are retired by setup, in
//! the user's session (`http_endpoint::legacy_trust`), never here.
use super::*;
use airk_core::service_http::{
    EndpointDescriptor, EndpointState, HTTP_CONTRACT_VERSION, LoopbackMcpUrl,
};
use airk_endpoint::http_endpoint::{self, BoundEndpoint, InstallationBinding};
use std::{path::PathBuf, sync::Weak};
use tokio_util::sync::CancellationToken;

pub(super) struct Owner {
    root: PathBuf,
    binding: InstallationBinding,
    descriptor: Mutex<EndpointDescriptor>,
    shutdown: CancellationToken,
    checkpoint: Arc<checkpoint::Coordinator>,
}
type StartedOwner = (Arc<Owner>, tokio::task::JoinHandle<Result<()>>);

/// U105 (§7): the local agent session's hook: the service's open-read path
/// (the one M3 `ToolSelection::Open` reaches).
fn open_hooks(state: Weak<State>) -> airk_https::http_backend::OpenHooks {
    airk_https::http_backend::OpenHooks {
        reader: Arc::new(move |read: airk_https::http_backend::OpenRead| {
            let state = state.clone();
            Box::pin(async move {
                let Some(state) = state.upgrade() else {
                    return airk_core::service::ControlResponse::Failure {
                        code: "service_draining".into(),
                    };
                };
                super::open_reads::response(
                    &state,
                    super::open_reads::OpenCall {
                        index_names: read.index_names,
                        set_names: read.set_names,
                        workspace: read.workspace,
                        tool: read.tool,
                        arguments: read.arguments,
                        mode: airk_core::service::ToolResponseMode::Terse,
                        client: read.client,
                    },
                )
                .await
            })
        }),
    }
}

impl Owner {
    pub(super) fn start(
        state: &Arc<State>,
        ownership: Arc<airk_store::service::ServiceLease>,
    ) -> Result<Option<StartedOwner>> {
        let root = state.control_root.join("http-1");
        // Upgrade (local trust plan B): delete the credential ledger and token
        // files a pre-B installation left, exact names only, before the
        // folder is prepared for the endpoint records.
        http_endpoint::retired_credentials::remove(&root)?;
        let Some(gate) = state.catalog_gate.clone() else {
            return Ok(None);
        };
        let clock = gate
            .catalog_clock()
            .ok_or_else(|| anyhow::anyhow!("HTTP catalog clock unavailable"))?;
        local_ipc::prepare_root(&root)?;
        // This is the stable protected installation/control root, never a versioned payload path.
        let binding = http_endpoint::installation_binding(&state.control_root, clock.namespace)?;
        // Current or legacy (an HTTPS build's record): the port and the
        // generation carry forward either way.
        let previous = http_endpoint::load_saved(&root, &binding)?;
        let held = BoundEndpoint::bind(previous.as_ref().map(|value| value.port()))?;
        let descriptor = EndpointDescriptor {
            schema: HTTP_CONTRACT_VERSION,
            installation: binding.installation,
            user: binding.user.clone(),
            namespace: binding.namespace,
            generation: previous.as_ref().map_or(Ok(1), |value| {
                value
                    .generation()
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("HTTP endpoint generation exhausted"))
            })?,
            service_instance: state.instance,
            executable_sha256: state.executable_sha256.parse()?,
            url: LoopbackMcpUrl::new(held.port()?)?,
            state: if state.checkpoint.admission_closed()? {
                EndpointState::Recovering
            } else {
                EndpointState::Ready
            },
            predecessor_digest: previous
                .as_ref()
                .map(airk_core::service_http::SavedEndpoint::digest)
                .transpose()?,
        };
        let control = state.control_root.clone();
        let weak: Weak<State> = Arc::downgrade(state);
        let open_hooks = open_hooks(weak.clone());
        // U110: every request is the built-in local agent session, an open
        // session of this endpoint's namespace.
        let namespace = descriptor.namespace;
        let validate: airk_https::http_gateway::ValidateRequest = Arc::new(move |context| {
            let control = control.clone();
            let weak = weak.clone();
            let open_hooks = open_hooks.clone();
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
                // U105: an open session needs only a ready catalog in its
                // namespace; each call reads through the open path.
                airk_https::http_backend::validate_open(&control, namespace, &context).await?;
                let factory = airk_https::http_backend::open_factory(open_hooks);
                Ok(airk_https::http_backend::with_query_capacity(
                    factory,
                    state.supervisor.query_capacity_reader(),
                ))
            })
        });
        let checkpoint = state.checkpoint.clone();
        let closed = Arc::new(move || checkpoint.admission_closed().unwrap_or(true));
        let checkpoint = state.checkpoint.clone();
        let admit: airk_https::http_gateway::AdmitRequest =
            Arc::new(move || Ok(Arc::new(checkpoint.admit()?)));
        let gateway = airk_https::http_gateway::Gateway::new(
            &descriptor,
            validate,
            airk_https::http_gateway::NativeAdmission {
                closed,
                gate,
                admit,
                settle: state.supervisor.read_settlement_requester(),
                delivery_guard_factory: super::arbiter::policy_delivery::factory(state),
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
            shutdown: CancellationToken::new(),
            checkpoint: state.checkpoint.clone(),
        });
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

    /// The caller serializes this periodic maintenance and shutdown
    /// publication. With no leaf to renew it republishes only a changed state.
    pub(super) fn refresh(&self) -> Result<()> {
        self.update_state()
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
