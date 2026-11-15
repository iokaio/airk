# Native service control runtime

Owner: `airk-service::service_runtime`. The runtime admits same-user I1 control
connections and dispatches them to their native owning components. The I1
request/response formats remain owned by `airk-ipc` and Core; this contract
records the service lifetime obligations at the connection delivery boundary.

## Setup exclusion at startup

Every foreground, detached, login and OS-manager entry converges on the same
startup gate before opening control state. For an installed or active setup
control root, normal startup holds the native installation lock through the
ready control listener. An admitted/prepared owner refuses startup regardless
of expiry. The exact fully published candidate may start under its completing
setup owner's lock only when the bound operation requested start, the live
I4 pair/root IDs and native image/signatures match. Other development control
roots retain their independent lifecycle. This adds no lifecycle record fields.

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


## Explicit narrowing on one-index read routes

W4/A2 selection and Open reads retain their ordinary singleton native handler
when no Project/index subset controls are supplied. Presence of existing
`project_ids`, `index_ids` or `member_availability` instead chooses the existing
combined frontend even if the original admitted selection currently resolves
one index. Malformed/empty values reach that owner's validator rather than
being dropped or silently treated as absent. `limit_per_index` alone retains
the existing singleton conversion.

The original W4/A2 grant, Open lease, exact catalog revision, source subset,
read-attempt deadline/cancellation and delivery fences remain the authority.
The combined owner may only narrow its original member roster and validates
all selectors before evidence work. A frozen one-index Catalog request uses
an exact existing CombinedConnectRequest projection with its same context key,
namespace/revision, grant/revision, Project IDs and exactly its original index.
No-selector Catalog reads stay singleton. A legacy Project binding does not
acquire reviewed combined authority and continues to refuse subset selectors.
No request, receipt, digest or persisted shape changes.

Regression selectors:
- service_runtime::tools_call::tests::explicit_subset_controls_use_native_combined_owner_even_for_one_index
- one_index_window_catalog_and_open_explicit_source_selectors_use_original_authorized_combined_owner

The latter owns two real Project aliases for the same source/index, performs
current canonical publication/hash source checks, and covers exact file-info
narrowing and foreign Project/index refusal under W4, Catalog and Open. Open
retains its actual empty Project attribution. Native qualification still needs
the matched-pair real Workbench rerun after this source is built.
