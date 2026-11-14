# Native editor Admin transport

`AdminLaunchSecret` is owned here with its original validation, serialization
and redacted Debug implementation. `service::AdminLaunchSecret` reexports the
same type for compatibility; private custody has no dependency back to service
request/response types. Moving this definition changes no wire or state bytes.

Owner: Core. Recorded primary contract decision for the authorized P6 work:
the editor uses its persistent native `airkctl session` as a private Admin
client. It never manufactures an HTTP peer or exposes a cookie/anti-forgery
secret in I2. Service implementation uses the same AdminPlane/session pool,
operation table, confirmations, drain policy and typed public projection as I3.
Browser HTTP admission remains unchanged. This adds no crate or persisted state.

I1 has private AdminNativeOpen/Call/Read/Cancel/Close methods. Open receives the
original same-host monotonic deadline_ns. Call carries the original request_id,
deadline_ns, operation and exact raw JSON body. Read/cancel name that original
request; Read cannot refresh its deadline. Only the actual owning service
instance may issue or accept the private session. Every metadata relay refuses
these methods and every public response projection refuses private credential
variants, including unexpected replies and error-line native values.

The raw body remains bounded to 64 KiB UTF-8, retaining the existing domain
parser/depth semantics. RawValue avoids double escaping and framing depth.
Existing request/frame limits remain unchanged: AdminNativeCall uses the
existing 128 KiB I1 request budget; other private methods keep the 64 KiB default.
The complete I3 JSON envelope is buffered to at most 32 MiB; the server reserves
one of two global response buffers before any effects. Data travels in chunks
of at most 64 KiB, under a dedicated 512 KiB I1 response budget. Existing native
method limits, codec ceiling and I2 16 MiB frame budget do not increase.

NativeAdminChunk contains http_status, total_bytes, offset, data and done. Status
is 100..599; total is nonzero and at most 32 MiB. Data is nonempty and at most
64 KiB. Checked offset+length cannot exceed total; done is exactly end==total.
The initial offset is zero, each next offset is the previous end, and status
and total cannot change. Complete bytes must decode as UTF-8 JSON. Incomplete,
duplicate, reordered, mismatched and oversized streams refuse as whole results.
An error never authorizes replay of the original call.

I2 feature admin-management/1 exposes AdminCall(operation, raw body, deadline_ms),
AdminRead(target, offset), AdminCancel(target), AdminClose and typed AdminChunk /
AdminClosed outcomes. The outer original AdminCall id is its native request_id.
Cookies/CSRF stay only in the Rust cache, bound to actual service instance and
cache generation. Deadline/cancellation is captured before queueing, rechecked
before open and first mutation dispatch, and propagated natively. No automatic
reopen-and-replay after expiry/rebind. Late opens close rather than populate a
superseded cache. Close removes only its own native session; service end alone
ends all sessions. Closing/rebind suppress stale delivery without rewriting the
original operation or claiming accepted native effects were undone.

Recovery handles live only in the current native Admin cookie/session. Abrupt
CLI death loses their availability; normal close or existing native TTL ends
that authority. There is no fake persistence/cross-process automatic resume.
UI retains unknown outcomes honestly and never automatically replays them.

Rust-emitted chunk semantic cases and I2 schema precede frontend consumers.
Validate exact bounds, stable framing, multibyte UTF-8 split across chunks,
incomplete/invalid streams, typed private-response refusal, first-dispatch
expiry/cancel, close/open races and independent-client isolation. Compilation
or injected handlers alone do not qualify real IPC/editor transport journeys.

The complete AdminNative response family is purpose-private, including chunks
and cancellation/close acknowledgements. Generic metadata and error projections
refuse these typed responses before serialization; only the dedicated bridge
may consume them. A chunk's numeric byte array is not a public metadata packet.

Native calls retain a pure liveness/phase/original-deadline callback through
canonical service dispatch, the registry actor queue and blocking executor
queues. Owning execution checks it after queue admission before first effects;
it never acquires Admin session locks under catalog/checkpoint gates. Pipe
callers preserve their existing admission without this additional callback.
Stream-owned read delivery stays on the pipe so scope tickets remain held
through protected writes; the native Admin facade cannot bypass those tickets.
