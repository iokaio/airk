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
