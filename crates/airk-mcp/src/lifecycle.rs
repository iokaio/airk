// SPDX-License-Identifier: Apache-2.0
//! Connection-local stdio negotiation around rmcp's request dispatcher.
//!
//! rmcp 3.2 permanently requires inline metadata when discovery is the first
//! request, even if the client subsequently falls back to a successful legacy
//! initialize. Own that small lifecycle boundary here; retain SDK framing,
//! cancellation, request validation and tool dispatch.
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        ClientNotification, ClientRequest, InitializeRequestParams, PingRequestMethod,
        ProtocolVersion, ServerInfo, ServerResult,
    },
    service::{NotificationContext, RequestContext, Service},
};
use std::{borrow::Cow, sync::Mutex};

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Awaiting,
    Inline,
    Initialized,
}

pub(crate) struct StdioLifecycle<H> {
    handler: H,
    phase: Mutex<Phase>,
}

impl<H: ServerHandler> StdioLifecycle<H> {
    pub(crate) fn new(handler: H) -> Self {
        Self {
            handler,
            phase: Mutex::new(Phase::Awaiting),
        }
    }

    fn initialize(
        &self,
        mut request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ServerResult, ErrorData> {
        let mut phase = self.phase.lock().expect("stdio lifecycle lock");
        if *phase == Phase::Initialized {
            return Err(ErrorData::invalid_request("Already initialized", None));
        }
        let supported = self.handler.supported_protocol_versions();
        let version = if request.protocol_version < ProtocolVersion::V_2026_07_28
            && supported.contains(&request.protocol_version)
        {
            Some(request.protocol_version.clone())
        } else {
            supported
                .iter()
                .filter(|v| **v < ProtocolVersion::V_2026_07_28)
                .max_by(|a, b| a.as_str().cmp(b.as_str()))
                .cloned()
        };
        let Some(version) = version else {
            return Err(ErrorData::unsupported_protocol_version(
                request.protocol_version,
                &supported,
            ));
        };
        let mut info = self.handler.get_info();
        info.protocol_version = version.clone();
        request.protocol_version = version;
        context.peer.set_peer_info(request);
        *phase = Phase::Initialized;
        Ok(ServerResult::InitializeResult(info))
    }
}

impl<H: ServerHandler> Service<RoleServer> for StdioLifecycle<H> {
    async fn handle_request(
        &self,
        request: ClientRequest,
        context: RequestContext<RoleServer>,
    ) -> Result<ServerResult, ErrorData> {
        if let ClientRequest::InitializeRequest(request) = request {
            return self.initialize(request.params, context);
        }
        {
            let mut phase = self.phase.lock().expect("stdio lifecycle lock");
            let declares_protocol = [
                "io.modelcontextprotocol/protocolVersion",
                "io.modelcontextprotocol/clientCapabilities",
                "io.modelcontextprotocol/clientInfo",
            ]
            .iter()
            .any(|key| context.meta.contains_key(*key));
            let pre_init_ping = *phase == Phase::Awaiting
                && !declares_protocol
                && matches!(request, ClientRequest::PingRequest(_));
            if !pre_init_ping
                && (*phase != Phase::Initialized
                    || declares_protocol
                    || matches!(request, ClientRequest::DiscoverRequest(_)))
            {
                let missing = context
                    .meta
                    .missing_required_keys(&ProtocolVersion::V_2026_07_28);
                if !missing.is_empty() {
                    return Err(ErrorData::invalid_params(
                        format!(
                            "request _meta is missing or has malformed required fields: {}",
                            missing.join(", ")
                        ),
                        None,
                    ));
                }
                if *phase == Phase::Awaiting {
                    *phase = Phase::Inline;
                }
                if *phase == Phase::Inline && matches!(request, ClientRequest::PingRequest(_)) {
                    return Err(ErrorData::method_not_found::<PingRequestMethod>());
                }
            }
        }
        // Discovery never overwrites the legacy negotiation. Explicit inline
        // requests still undergo the SDK's version/capability/schema checks.
        Service::handle_request(&self.handler, request, context).await
    }

    async fn handle_notification(
        &self,
        notification: ClientNotification,
        context: NotificationContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        Service::handle_notification(&self.handler, notification, context).await
    }

    fn get_info(&self) -> ServerInfo {
        self.handler.get_info()
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        self.handler.supported_protocol_versions()
    }
}
