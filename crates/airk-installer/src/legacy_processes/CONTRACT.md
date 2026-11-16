# Same-user executable candidate observation

Owner: `airk-installer::legacy_processes`. This boundary supplies candidate
PIDs only; it never authorizes a signal. Runtime upgrade/replacement owners
retain their exact-process/image/lease authority and require a complete empty
observation before destructive retirement. Unknown or inaccessible candidates
remain a refusal. `Audit` retains its existing serialized fields and shapes.

On macOS, same-effective-user enumeration is followed by bounded native path
queries. Clear/capture errno at the actual query boundary. A failed path query
is not process exit proof: Darwin may report ESRCH for a still-live process's
unavailable executable vnode. A zero-byte path result with exact ESRCH is
excused only by a separate fresh `proc_pidinfo(PROC_PIDTBSDINFO, arg=1)`
observation: either zero bytes with exact ESRCH (kernel process absence), or
an exact native `libc::proc_bsdinfo` byte count for the requested PID and verified effective UID whose
state is SZOMB. The latter includes unreaped zombies and cannot be inherited
by a different/reused active PID or a terminal process of another/unknown UID. Use the pinned libc structure and constants;
never a handwritten ABI. Permissions, unknown errno/state, short/oversized
replies, wrong PID, negative/oversized/unterminated path data stay incomplete.

Every unobserved candidate remains in `scanned`/`unobserved`; the first bounded
PID/path count/errno and BSD query count/errno/PID/UID/state facts occupy the
existing `Audit.failure` string. Diagnostics neither grant signal authority
nor erase the original failed-run evidence. Successful path interpretation
retains the original bounded-count/first-NUL behavior; no count/NUL equality
or terminator convention is added. Other-platform decisions and all
predecessor retirement/refusal predicates remain unchanged.

Cheap qualification selects the existing `legacy_processes::tests` group plus
new Mac classification/refusal/ABI controls, then the native Mac current-self
query. The original CLI same-build replacement identity and all preservation,
cleanup and reactivation assertions must run unchanged on Mac after source
review and scoped Clippy. Its historical unobserved PID/errno was not captured,
so that failure's exact cause remains unknown even if the new controls pass.
