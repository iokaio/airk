// SPDX-License-Identifier: Apache-2.0
//! Admin mutations through the canonical handlers (contracts §2.4, §4.1,
//! §4.4; split Phase 5b).
//!
//! The browser never holds a request key. A row's reviewed `request_key`
//! rule (contracts §4.3b, U98) says where the identity lives. In `generate`
//! mode the handler reads `(namespace, epoch)` with `RequestEpoch`, draws the
//! caller and the nonce from the OS random source and fills them in; a body
//! that names a rule path itself is refused. In `review` mode the handler
//! fills the identity a session review token holds. A keyed `Enqueue` or
//! `ConfigureProject` is journaled the way `airkctl` journals it (M5
//! prepare, M7 submit, M8 forget after a job is the answer); the other keyed
//! methods are key-idempotent in their own ledger and are sent once with
//! their key. A lost reply is resent with the same key or nonce, never a
//! new one; a second loss is an unknown outcome.
use super::{Dispatch, Random, secrets::Review};
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse, RequestKey},
};
use airk_endpoint::admin_contract::{AdminCode, RequestKeyRule, RequestProgress, RequestState};
use serde_json::{Value, json};

/// The keyed variants the request journal (M5) accepts
/// (`request_journal::prepare_keyed`).
pub(super) const JOURNALED: &[&str] = &["enqueue", "configure_project"];

/// The serde name of the `ControlMethod` a body encodes.
pub(super) fn variant(body: &Value) -> Option<&str> {
    match body {
        Value::String(name) => Some(name),
        Value::Object(object) if object.len() == 1 => object.keys().next().map(String::as_str),
        _ => None,
    }
}

/// The server-generated identity of a keyed request.
#[derive(Clone)]
pub(super) struct Key {
    pub(super) caller: Incarnation,
    pub(super) key: RequestKey,
}

/// What a mutation ended as.
pub(super) enum Executed {
    /// The native answer, and the request it belongs to.
    Answered(Box<ControlResponse>, Option<RequestProgress>),
    /// No answer after one resend with the same key or nonce: it may have
    /// been accepted. A journaled request stays listed (M6) by its nonce.
    Unknown,
}

/// 128 bits from the OS random source.
pub(super) fn identity(random: &Random) -> Result<Incarnation, AdminCode> {
    let bytes = random().ok_or(AdminCode::AdminInternal)?;
    let mut half = [0u8; 16];
    half.copy_from_slice(&bytes[..16]);
    Ok(Incarnation::from_bytes(half))
}

/// The generate-mode rule of the reviewed row whose method `body` encodes.
#[cfg(test)]
fn generate_rule(body: &Value) -> Option<&'static RequestKeyRule> {
    let name = variant(body)?;
    let table = super::operations::table().ok()?;
    table
        .operations
        .iter()
        .filter(|row| {
            let variant = row.method.split('.').next().unwrap_or_default();
            super::operations::snake(variant) == name
        })
        .find_map(|row| {
            row.request_key
                .as_ref()
                .filter(|rule| rule.mode == airk_endpoint::admin_contract::KeyMode::Generate)
        })
}

/// For a body whose method has a generate-mode rule: `generate`. Other
/// bodies are returned unchanged with no key (the unit tests' entry; `call`
/// passes its row's rule to `generate` directly).
#[cfg(test)]
pub(super) async fn fill_key(
    dispatch: &Dispatch,
    random: &Random,
    body: &mut Value,
) -> Result<Result<Option<Key>, ControlResponse>, AdminCode> {
    let Some(rule) = generate_rule(body) else {
        return Ok(Ok(None));
    };
    Ok(generate(dispatch, random, rule, body).await?.map(Some))
}

/// The method's own object (`{"<variant>": {..}}`) and its variant name.
fn inner_mut(body: &mut Value) -> Result<&mut Value, AdminCode> {
    let name = variant(body)
        .ok_or(AdminCode::AdminMalformedRequest)?
        .to_owned();
    body.get_mut(&name)
        .filter(|inner| inner.is_object())
        .ok_or(AdminCode::AdminMalformedRequest)
}

fn present(inner: &Value, path: &str) -> bool {
    let mut value = inner;
    for segment in path.split('.') {
        match value.get(segment) {
            Some(next) => value = next,
            None => return false,
        }
    }
    true
}

/// Set `path` inside `inner`; every parent must already be an object.
fn place(inner: &mut Value, path: &str, value: Value) -> Result<(), AdminCode> {
    let (parents, last) = match path.rsplit_once('.') {
        Some((parents, last)) => (Some(parents), last),
        None => (None, path),
    };
    let mut target = inner;
    for segment in parents.into_iter().flat_map(|parents| parents.split('.')) {
        target = target
            .get_mut(segment)
            .ok_or(AdminCode::AdminMalformedRequest)?;
    }
    target
        .as_object_mut()
        .ok_or(AdminCode::AdminMalformedRequest)?
        .insert(last.to_owned(), value);
    Ok(())
}

fn named(inner: &Value, rule: &RequestKeyRule) -> bool {
    present(inner, &rule.key)
        || rule
            .caller
            .as_deref()
            .is_some_and(|path| present(inner, path))
}

/// Generate mode: refuse a browser-supplied identity, read the epoch and
/// fill a fresh caller (where the rule names one) and key in.
/// `Ok(Err(response))` is the epoch's own refusal (nothing was sent but
/// the read).
pub(super) async fn generate(
    dispatch: &Dispatch,
    random: &Random,
    rule: &RequestKeyRule,
    body: &mut Value,
) -> Result<Result<Key, ControlResponse>, AdminCode> {
    if named(inner_mut(body)?, rule) {
        return Err(AdminCode::AdminMalformedRequest);
    }
    let (namespace, epoch) = match dispatch(ControlMethod::RequestEpoch).await {
        Ok(ControlResponse::RequestEpoch { namespace, epoch }) => (namespace, epoch),
        Ok(other) => return Ok(Err(other)),
        Err(_) => return Err(AdminCode::AdminInternal),
    };
    let key = Key {
        caller: identity(random)?,
        key: RequestKey {
            namespace,
            epoch,
            nonce: identity(random)?,
        },
    };
    let inner = inner_mut(body)?;
    if let Some(path) = &rule.caller {
        place(inner, path, json!(key.caller))?;
    }
    place(inner, &rule.key, json!(key.key))?;
    Ok(Ok(key))
}

/// Review mode: refuse a browser-supplied identity and fill the one the
/// session's review holds.
pub(super) fn restore(
    rule: &RequestKeyRule,
    body: &mut Value,
    review: &Review,
) -> Result<(), AdminCode> {
    let inner = inner_mut(body)?;
    if named(inner, rule) {
        return Err(AdminCode::AdminMalformedRequest);
    }
    if let Some(path) = &rule.caller {
        let caller = review.caller.clone().ok_or(AdminCode::AdminReviewUnknown)?;
        place(inner, path, caller)?;
    }
    place(inner, &rule.key, review.key.clone())
}

/// Remove every object's `key` equal to `key` from a native result, and the
/// `caller` beside it; the first such caller (§4.3b: request identities are
/// the only fields removed).
pub(super) fn redact(value: &mut Value, key: &Value) -> Option<Value> {
    fn walk(value: &mut Value, key: &Value, caller: &mut Option<Value>) {
        match value {
            Value::Object(map) => {
                if map.get("key") == Some(key) {
                    map.remove("key");
                    if let Some(found) = map.remove("caller") {
                        caller.get_or_insert(found);
                    }
                }
                for child in map.values_mut() {
                    walk(child, key, caller);
                }
            }
            Value::Array(items) => {
                for child in items {
                    walk(child, key, caller);
                }
            }
            _ => {}
        }
    }
    let mut caller = None;
    walk(value, key, &mut caller);
    caller
}

/// The request state a successful native answer shows (§2.4 UI states).
pub(super) fn state(response: &ControlResponse) -> RequestState {
    match response {
        ControlResponse::Job { job } if job.outcome.is_none() => RequestState::Accepted,
        ControlResponse::LifecycleAccepted { .. } => RequestState::Accepted,
        _ => RequestState::Completed,
    }
}

fn progress(nonce: Incarnation, response: &ControlResponse) -> Option<RequestProgress> {
    Some(RequestProgress {
        nonce: nonce.to_string(),
        state: state(response),
    })
}

/// Send `method`, resending the identical method once if the reply is lost.
async fn send_twice(dispatch: &Dispatch, method: ControlMethod) -> Option<ControlResponse> {
    match dispatch(method.clone()).await {
        Ok(response) => Some(response),
        Err(_) => dispatch(method).await.ok(),
    }
}

/// Run one admitted mutation. `key` is what `fill_key` filled in.
pub(super) async fn execute(
    dispatch: &Dispatch,
    random: &Random,
    method: ControlMethod,
    key: Option<Key>,
) -> Result<Executed, AdminCode> {
    let Some(Key { caller, key }) = key else {
        // A natural or digest-idempotent mutation: one request. Submit and
        // forget name an existing journaled request; the others get a
        // server-generated nonce that names this admin request only.
        let nonce = match &method {
            ControlMethod::ClientRequestSubmit { nonce }
            | ControlMethod::ClientRequestForget { nonce } => *nonce,
            _ => identity(random)?,
        };
        return Ok(match dispatch(method).await {
            Ok(response) => {
                let request = progress(nonce, &response);
                Executed::Answered(Box::new(response), request)
            }
            Err(_) => Executed::Unknown,
        });
    };
    let journaled = serde_json::to_value(&method)
        .ok()
        .as_ref()
        .and_then(variant)
        .is_some_and(|name| JOURNALED.contains(&name));
    if !journaled {
        return Ok(match send_twice(dispatch, method).await {
            Some(response) => {
                let request = progress(key.nonce, &response);
                Executed::Answered(Box::new(response), request)
            }
            None => Executed::Unknown,
        });
    }
    // M5: journal the exact request; a resend of the same M5 answers
    // `duplicate: true` with the same nonce.
    let prepare = ControlMethod::ClientRequestPrepare {
        caller,
        key: key.clone(),
        request: Box::new(method),
    };
    let nonce = match send_twice(dispatch, prepare).await {
        Some(ControlResponse::ClientRequestPrepared { nonce, .. }) => nonce,
        // Refused before anything was journaled.
        Some(other) => return Ok(Executed::Answered(Box::new(other), None)),
        None => return Ok(Executed::Unknown),
    };
    // M7 by nonce, resent with the same nonce if its reply is lost.
    match send_twice(dispatch, ControlMethod::ClientRequestSubmit { nonce }).await {
        Some(response @ ControlResponse::Job { .. }) => {
            // M8 as `airkctl` does after a job is the answer; the job
            // receipt stands whether or not the forget is answered.
            let _ = dispatch(ControlMethod::ClientRequestForget { nonce }).await;
            let request = progress(nonce, &response);
            Ok(Executed::Answered(Box::new(response), request))
        }
        // A refusal keeps the journaled request for a later submit or forget.
        Some(other) => Ok(Executed::Answered(Box::new(other), None)),
        None => Ok(Executed::Unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        identity::Sha256Digest,
        service::{JobKind, JobRecord, JobState},
    };
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    /// A dispatch that records every method and answers from a script;
    /// `None` is a lost reply.
    fn scripted(script: Vec<Option<ControlResponse>>) -> (Dispatch, Arc<Mutex<Vec<Value>>>) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let script = Arc::new(Mutex::new(VecDeque::from(script)));
        let log = sent.clone();
        let dispatch: Dispatch = Arc::new(move |method: ControlMethod| {
            log.lock()
                .unwrap()
                .push(serde_json::to_value(&method).unwrap());
            let answer = script.lock().unwrap().pop_front().flatten();
            Box::pin(async move { answer.ok_or_else(|| anyhow::anyhow!("reply lost")) })
        });
        (dispatch, sent)
    }

    /// `[n; 32]` for n = 0xc1, 0xc2, …
    fn random() -> Random {
        let next = Arc::new(Mutex::new(0xc0u8));
        Arc::new(move || {
            let mut next = next.lock().unwrap();
            *next += 1;
            Some([*next; 32])
        })
    }

    fn job(project: Sha256Digest) -> ControlResponse {
        ControlResponse::Job {
            job: JobRecord {
                id: Sha256Digest::from_bytes([0x3b; 32]),
                project,
                kind: JobKind::Inspect,
                state: JobState::Pending,
                fencing_generation: 0,
                attempts: 0,
                outcome: None,
            },
        }
    }

    fn namespace() -> Incarnation {
        Incarnation::from_bytes([0x0a; 16])
    }

    #[test]
    fn journaled_requests_get_server_keys_and_replay_with_the_same_nonce() {
        let project = Sha256Digest::from_bytes([0xab; 32]);
        let nonce = Incarnation::from_bytes([0xc2; 16]);
        let (dispatch, sent) = scripted(vec![
            Some(ControlResponse::RequestEpoch {
                namespace: namespace(),
                epoch: 7,
            }),
            // M5's reply is lost; the resend of the same M5 is a duplicate.
            None,
            Some(ControlResponse::ClientRequestPrepared {
                nonce,
                project,
                kind: JobKind::Inspect,
                duplicate: true,
            }),
            // M7's reply is lost; the resend with the same nonce answers.
            None,
            Some(job(project)),
            Some(ControlResponse::ClientRequestForgotten {
                nonce,
                existed: true,
            }),
        ]);
        let random = random();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut body = json!({"enqueue": {"project": project, "kind": "inspect"}});
        let executed = runtime.block_on(async {
            let Ok(Ok(Some(key))) = fill_key(&dispatch, &random, &mut body).await else {
                panic!("a keyed body gets a server key");
            };
            assert_eq!(key.caller, Incarnation::from_bytes([0xc1; 16]));
            assert_eq!(
                key.key,
                RequestKey {
                    namespace: namespace(),
                    epoch: 7,
                    nonce,
                }
            );
            let method: ControlMethod = serde_json::from_value(body.clone()).unwrap();
            execute(&dispatch, &random, method, Some(key))
                .await
                .unwrap()
        });
        let Executed::Answered(response, Some(request)) = executed else {
            panic!("the job is the answer");
        };
        assert!(matches!(*response, ControlResponse::Job { .. }));
        assert_eq!(request.nonce, nonce.to_string());
        assert_eq!(request.state, RequestState::Accepted);
        let sent = sent.lock().unwrap();
        let names: Vec<&str> = sent.iter().filter_map(variant).collect();
        assert_eq!(
            names,
            [
                "request_epoch",
                "client_request_prepare",
                "client_request_prepare",
                "client_request_submit",
                "client_request_submit",
                "client_request_forget",
            ]
        );
        // Both M5 sends are the identical request, whose inner caller and
        // key are the server's; both M7 sends and the M8 name that nonce.
        assert_eq!(sent[1], sent[2]);
        let prepare = &sent[1]["client_request_prepare"];
        assert_eq!(prepare["caller"], prepare["request"]["enqueue"]["caller"]);
        assert_eq!(prepare["key"], prepare["request"]["enqueue"]["key"]);
        assert_eq!(prepare["key"]["nonce"], json!(nonce));
        for index in [3, 4] {
            assert_eq!(sent[index]["client_request_submit"]["nonce"], json!(nonce));
        }
        assert_eq!(sent[5]["client_request_forget"]["nonce"], json!(nonce));
    }

    #[test]
    fn a_browser_supplied_caller_or_key_is_refused_before_any_dispatch() {
        let (dispatch, sent) = scripted(Vec::new());
        let random = random();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        for mut body in [
            json!({"retry": {"id": "3b".repeat(32), "caller": "c1".repeat(16)}}),
            json!({"lifecycle": {"action": "stop", "key": {"namespace": "0a".repeat(16), "epoch": 1, "nonce": "c2".repeat(16)}}}),
            json!({"enqueue": "inspect"}),
        ] {
            let refused = runtime.block_on(fill_key(&dispatch, &random, &mut body));
            assert!(matches!(refused, Err(AdminCode::AdminMalformedRequest)));
        }
        assert!(sent.lock().unwrap().is_empty());
    }

    #[test]
    fn keyed_ledger_methods_resend_their_key_and_lost_twice_is_unknown() {
        let (dispatch, sent) = scripted(vec![
            Some(ControlResponse::RequestEpoch {
                namespace: namespace(),
                epoch: 3,
            }),
            None,
            None,
        ]);
        let random = random();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut body = json!({"lifecycle": {"action": "stop"}});
        let executed = runtime.block_on(async {
            let Ok(Ok(key)) = fill_key(&dispatch, &random, &mut body).await else {
                panic!("the epoch is read");
            };
            assert!(key.is_some());
            let method: ControlMethod = serde_json::from_value(body.clone()).unwrap();
            execute(&dispatch, &random, method, key).await.unwrap()
        });
        assert!(matches!(executed, Executed::Unknown));
        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 3);
        // Never a new key for the same intent.
        assert_eq!(sent[1], sent[2]);
        assert_eq!(
            sent[1]["lifecycle"]["key"]["nonce"],
            json!(Incarnation::from_bytes([0xc2; 16]))
        );
    }

    #[test]
    fn natural_mutations_get_a_request_nonce_and_their_state() {
        let project = Sha256Digest::from_bytes([0xab; 32]);
        let (dispatch, sent) = scripted(vec![Some(job(project))]);
        let random = random();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut body = json!({"pause": {"project": project, "paused": true}});
        let executed = runtime.block_on(async {
            assert!(matches!(
                fill_key(&dispatch, &random, &mut body).await,
                Ok(Ok(None))
            ));
            let method: ControlMethod = serde_json::from_value(body.clone()).unwrap();
            execute(&dispatch, &random, method, None).await.unwrap()
        });
        let Executed::Answered(_, Some(request)) = executed else {
            panic!("answered");
        };
        assert_eq!(
            request.nonce,
            Incarnation::from_bytes([0xc1; 16]).to_string()
        );
        assert_eq!(sent.lock().unwrap().len(), 1);
        assert_eq!(
            state(&ControlResponse::ClientErrorRecorded {}),
            RequestState::Completed
        );
    }
}
