# Native service control runtime

Owner: `airk-service::service_runtime`. The runtime admits same-user I1 control
connections and dispatches them to their native owning components. The I1
request/response formats remain owned by `airk-ipc` and Core; this contract
records the service lifetime obligations at the connection delivery boundary.

## Accepted stop and restart

- A matching `Lifecycle` request is durably accepted through the existing
  `ServiceRegistry` lifecycle ledger before the instance remembers its original
  receipt. A replay keeps that receipt and never creates another request.
- Only a `Draining` record naming this instance requests its drain. A stale
  record replay, incompatible request or refused lifecycle does not request
  shutdown. The existing matching `Shutdown` control request also drains.
- The final delivery boundary signals the service's shutdown notification
  before polling its response writer. Failed, cancelled or blocked delivery
  cannot withdraw an accepted shutdown. Response errors remain observable as
  connection failures; no success response is manufactured.
- When the accept loop observes notification, it stops admitting connections. Existing
  handlers still get their response attempt under their own finite deadlines;
  the existing bounded connection drain waits for them before ending scopes
  and workers. The notification does not abort its own reply immediately.
- Once owners have drained, the same original lifecycle receipt advances:
  Stop to `Complete`, Restart to `Starting`. The admitted next start completes
  a pending predecessor through the existing lifecycle rules. Delivery loss
  does not require a second lifecycle request or a manufactured restart.
- Lifecycle ledger bytes, request keys, phase identities and locks are
  unchanged. No lock is held across response delivery.

## Focused checks

```console
cargo nextest run --profile ci -p airk-service --no-default-features --lib --locked --timings -E 'test(service_runtime::response_delivery_tests::) | test(service_runtime::lifecycle::tests::)'
```

The response-delivery tests use the actual acceptance and settlement ledger
with an injected failing writer and notification observation. They establish
the delivery boundary without starting or stopping an installed service.
Installed process/transport qualification remains a separate required boundary.
Each filtered run must select nonzero tests and report its selected count.
