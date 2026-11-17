# Sampled Windows process GPU-memory observer

Owner: [../arbiter-gpu-memory.py](../arbiter-gpu-memory.py).
Portable controls: [../tests/test_arbiter_gpu_memory.py](../tests/test_arbiter_gpu_memory.py).

This helper observes current Windows video-memory accounting. It creates no
process, context, reservation, admission witness, model or CUDA provider. It
changes no driver, registry, global environment or runtime policy. Importing it
does not load a DLL or issue a system query. Only explicit WindowsApi construction
maps the System32 gdi32/kernel32 entry points; only explicit sampling queries them.

## Identity and ownership

The caller supplies the ORIGINAL retained child process HANDLE, authenticated PID
and process creation FILETIME, plus an explicitly verified adapter LUID and physical
adapter index. Process, adapter, borrowed HANDLE and original deadline bindings have read-only
properties for the observer lifetime. The parent must already have established the current human's SID
and the child's same-human identity. This helper checks PID/creation/liveness
before AND after each pair of memory queries. It never opens a process by PID,
accepts a wildcard, guesses a PDH instance, or constructs a CUDA UUID/LUID mapping.
That native metadata mapping is a mandatory caller prerequisite.

Only the observer's adapter metadata handle is opened/closed. The original child
HANDLE is borrowed and never closed, duplicated, replaced or adopted here. Process
exit, PID/creation mismatch, missing query rights, API failure or uncertain
liveness yields an explicitly invalid/unknown sample; it is never a zero reading.

## Windows API and ABI

Use supported D3DKMTQueryVideoMemoryInfo, with hProcess non-null and opened with
PROCESS_QUERY_INFORMATION and SYNCHRONIZE (for retained-handle liveness),
D3DKMTOpenAdapterFromLuid and D3DKMTCloseAdapter.
Adapter selection is exact LUID/index; group0 is LOCAL, group1 NON_LOCAL.
No D3DKMT_QUERYSTATISTICS reserved structure, undocumented working-set maximum,
PDH wildcard, global-free subtraction or allocator-only proxy is substituted.

ABI source: installed Windows SDK10.0.26100.0 shared/d3dkmthk.h, SHA256
e7df81476816d5112cd8c73be18dbaf9924c3af4af352ea2650b48dfc23ba40d;
shared/d3dukmdt.h defines native Windows pointer layout and WOW64 thunking.
LUID is8 bytes, open-adapter12 and close-adapter4. QueryVideoMemoryInfo is56
bytes in the supported x64 layout: hProcess0/hAdapter8/group12/Budget16/
CurrentUsage24/CurrentReservation32/AvailableForReservation40/physical-index48.
The x86 layout has hProcess0/hAdapter4/group8, padding12, and the same64-bit
output offsets and56-byte extent under the SDK's default8-byte packing.
Portable tests assert both layouts. Actual API invocation is restricted to
Windows x64; x86 declarations are layout controls, not a platform qualification.

All signatures use32-bit NTSTATUS/handles/enums,64-bit counters and an actual
pointer-width process HANDLE. Only status STATUS_SUCCESS=0 makes returned
counters readable. Any other status stays explicit unknown, with untouched raw
status; uninitialized output zeros do not become observed usage.

## Sampling, clocks and limits

The caller supplies its ORIGINAL absolute monotonic deadline and invokes sample
at its existing bounded ticks/operation boundaries. There is no scheduling loop,
sleep, retry, fresh allowance or subprocess in this helper. Each query records
its own start/end, and every sample records actual start/end and start-to-start
gap. Calls are synchronous; an external owner still bounds the campaign if a
system call does not return. A late callback cannot extend the original cutoff
or make the sample valid.

LOCAL and NON_LOCAL queries are sequential, not an atomic joint snapshot.
Return each group's current usage, budget and reservation separately. Valid
usage0 is retained as0; unavailable usage is null. Never add independently
observed maxima or treat budget/reservation as measured allocation. Track maxima
only from complete, identity-fenced samples that returned before the cutoff.
Raw invalid observations/statuses remain in the returned sample, marked invalid.

The caller retains EVERY returned sample and its own real process settlement
proof. The helper retains only aggregate counters/maxima/gaps, with no unbounded
internal log. Missing observations and actual gaps are visible; the helper
cannot assert that a gap-free or transient-complete trace occurred.

## Claims and refusals

Outputs describe sampled current OS process usage, including operating-system
accounting semantics. Dedicated/local usage on a discrete adapter and nonlocal
usage do not establish a complete physical/private-only allocation census.
This is neither lifetime peak nor allocator peak, admission authority, a complete
GPU footprint ceiling, nor model/precision qualification. lifetime_peak_bytes
remains null and final_gpu_qualification/admission_authority remain false.
Existing stricter complete-process-peak and final qualification gates are UNCHANGED.

close reports the actual adapter-close status and leaves process ownership with
the parent. Failure to close is retained, not manufactured into successful
cleanup. Repeated close does not secretly retry a failed system call.

Primary API:
https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/d3dkmthk/nf-d3dkmthk-d3dkmtqueryvideomemoryinfo
https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/d3dkmthk/ns-d3dkmthk-_d3dkmt_queryvideomemoryinfo
