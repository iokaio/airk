# Same-host monotonic deadlines

Owner: `airk-process::monotonic`. `now_ns` reports an absolute, same-boot
nanosecond timestamp for private IPC deadlines, never a wall-clock timestamp.
It grants no identity or authority and is not a persisted record or digest input.

The platform clock must share `std::time::Instant`'s basis: Apple uses
`CLOCK_UPTIME_RAW`, other Unix targets use `CLOCK_MONOTONIC`, and Windows uses
QueryPerformanceCounter converted by QueryPerformanceFrequency. Apple uptime
excludes system sleep and uses the mach absolute time basis. Darwin's distinct
`CLOCK_MONOTONIC` has different resolution and sleep behavior; converting an
Instant duration against that clock can extend an original deadline even when
the sample order is conservative.

The advanced-split Darwin correction preserves Windows/Linux behavior and all
existing wire shapes and limits. Private timestamps are exchanged only between
same-host, same-boot peers under their existing matched-version and service /
worker instance fences. The read-budget feature-version gate alone is not a
claim of mixed-version clock compatibility. Never persist, replay after reboot,
or exchange these values between independently selected runtime images.

Validation retains the original strict Host read/settings deadline assertions,
an injected nanosecond sample-order regression, and an Apple native/Instant
sampling sandwich with no tolerance or retry. Actual pinned Rust source and Mac
clock observations are retained in the campaign's attempt4 receipt directory.
