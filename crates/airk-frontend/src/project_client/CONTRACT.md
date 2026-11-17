# Canonical project connection reads

`Connection::exchange` validates an existing original read attempt and Core
PartitionAllowance before its first frame write. A valid `is_ordinary_focus()`
allowance accepts only canonical `airk_search`; every other valid partition,
including a zero-optional-work Context share, accepts only `airk_answer`. Neither
accepts NativeRead requests or public aliases at this internal boundary. An
unsupported peer cannot carry any partition. No partition leaves the ordinary
connection path unchanged.

The selector/Host owns actual selection admission. This frontend predicate is
purpose validation only and grants no authority. Existing Host clock, original
request identity, immutable absolute expiry, native worker/scope proof, transport
submission and delivery fences remain required; nothing creates a new budget
or retries a submitted query.

Cheap regressions in `project_client::read_attempt_tests` use Rust-emitted Core
allowance fixtures and the actual Connection exchange over local IPC. They
compare original allowances and arguments, retain the original attempt ID and
check crossed/native/unsupported/malformed refusal before the first frame.

Direct LocalTools SDK dispatch applies the same Core purpose predicate to every
partition before selecting an operation-specific attempt path. A partition on
any other canonical tool is refused, rather than silently discarded. Existing
Search/Answer aliases retain their previous local-only mapping; the native
connection continues to require canonical wire names.

## D06 accepted read and connection admission are separate clocks

The existing55-second client reuse cutoff and60-second native broker/worker
connection admission expiry remain unchanged. They decide whether a NEW query
may be submitted/admitted; neither rewrites the original accepted read budget.
An admitted read keeps its original monotonic parent, request ID, cancellation,
partition and immutable source/selection binding through native completion and
outer delivery. The caller's actual120-second Answer cap/125-second transport
parent and existing125-second hard-reap boundary still apply. No completed work
or receipt is rebound, no query is replayed, and no authority lifetime is renewed.

Connection reuse remains fresh/healthy and source-ticket checked before the
first frame. A cached connection is not replaced merely because its admission
expiry precedes the parent read deadline: the broker owns actual acceptance.
After acceptance, expiration of the admission window prohibits further queries
on that connection, while the existing owner awaits the accepted native result.
Delivery still refuses caller expiry/cancellation and actual nonce, catalog,
installed source scope, configuration or publication changes. The connection
admission clock never grants source authority or permits expired scope delivery.

Injected-clock regressions distinguish accepted completion at70s under the same
120s parent from refused new submission after55s; parent expiry/cancellation
still refuse. An actual local IPC case preserves the captured120s absolute wire
expiry/request/arguments exactly rather than narrowing it to55s. No model runs.
