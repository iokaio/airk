# macOS cold-status diagnostic after PR 55

This isolated diagnostic branch is not a release candidate. It retains AIRK
1.8.7 and the existing 28-tool MCP / 18-tool CLI contracts. No failed request
becomes retryable and the original failing assertion remains intact.

PR CI 41372380164 passed Windows, Linux, container, extension and four-host
contracts. Apple Silicon passed its first seven-test managed MCP smoke, then
one of four cold pinned clients received the generic connection-error envelope
in the second pass. The error was not an eligible typed retry receipt. Existing
logs did not retain its underlying Rust cause. The manually restarted Apple
Silicon job passed all three smoke rounds and PR 55 merged at main commit
9e9ad76459fb1012cc35752ffab994de58a59a3d (the same Git tree as this branch's
base). Main CI 41380069062 then passed Windows, Linux and Apple Silicon, but
Intel failed the cold source-bound case in smoke pass three with the same
generic envelope. All mixed-read tests passed. The underlying cause remains
unknown; neither failure is treated as an eligible retry receipt.

The diagnostic-only build writes at most 2,048 error characters to stderr and
the fixture forwards bounded stderr for fictional, disposable project inputs.
One Intel job runs twelve cohorts of only the pinned and source cold tests. Every
failure remains a failure. It uploads text traces only, never source/index state
or native binaries. No signing, publication, broad qualification matrix or
Windows/Linux rerun is involved. This branch must not be promoted or merged as
the release fix; diagnose the cause before changing the release branch. The
README, documentation index, release notes and Marketplace description retain
their current 1.8.7 behavior and version; these tracing changes establish no new
product or cross-platform claim.
