# Peak memory of one process (Windows)

`measure-peak-memory.ps1` runs one executable to completion and prints its
standard output, then a summary line with its exit code, elapsed seconds,
peak working set and peak private (pagefile) bytes. The counters come from the
process's own handle (`GetProcessMemoryInfo`), so they cover the whole run,
not a sampled maximum.

- **Inputs:** `-Exe` (path of the executable, for example a test binary under a
  Cargo target's `deps` folder) and optional `-Arguments`.
- **Outputs:** standard output only; redirect it to keep a receipt. The script
  exits with the measured process's exit code.
- **Writes:** none.

Example, measuring one Rust test:

```powershell
pwsh -File scripts/dev/peak-memory/measure-peak-memory.ps1 `
  -Exe <target>\debug\deps\airk_mcp-<hash>.exe `
  -Arguments '--exact','remote::http::tests::an_answer_in_flight_keeps_its_http_session_past_the_idle_timeout'
```

It was first used to show that this test peaks at about 11 MB after the fix
recorded in `crates/airk-mcp/src/remote/CONTRACT.md`.
