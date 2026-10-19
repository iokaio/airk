# Request execution and verification costs

MCP accepts at most four evidence requests and two control requests per server instance. Status, history and capabilities use the control capacity. Excess requests return a retryable capacity error immediately; there is no application request queue. `airk_capabilities.request_admission` reports the limits and the process-local rejected-request count. Separate MCP processes have separate limits.

An accepted request runs its usage setup, kernel operation, serialization, baseline lookup and ledger write on one blocking worker. Filesystem and SQLite waits therefore leave the asynchronous protocol executor available. An evidence workload cannot consume the reserved control permits. A contended store can still delay its own accepted requests; this is bounded concurrency, not a universal latency guarantee.

Cancellation stops waiting for a response. Already accepted synchronous work completes and records once, retaining its permit until accounting finishes. Closing stdio or the local tool client stops admission and drains accepted workers. AIRK does not forcibly interrupt a thread holding SQLite state. Abrupt process termination can lose observations. Requests rejected before admission do not start usage spans and appear in the admission counter rather than the usage ledger. Optional usage failures retain the existing dropped-observation reporting and do not turn a successful retrieval into an error.

Warm lexical, vector and hybrid reads authenticate all generation components against publication receipts. Hybrid authenticates once for both candidate branches. Vector filtering and candidate fusion retain private chunk ordinals until the final text budget is applied. Neither cached handles nor filesystem timestamps establish integrity.

Full-source usage comparisons authenticate only the receipt-bound manifest and catalog they consume, with the reader lease retained during lookup. They do not repeat vector, lexical or model verification already required by retrieval. The store's measured lookup API reports authenticated logical bytes, verification time and lookup time separately. This is a narrower authenticated projection, not a cross-request verification cache. Catalog hashing still scales with catalog size; the SQL progress deadline does not include hashing time. Logical bytes are not physical disk traffic.

Run the binary/source-bound retrieval qualification with:

```console
python scripts/check-retrieval-quality.py --binary target/debug/airk --output target/retrieval-qualification.json
```

Use `airk.exe` on Windows. The receipt contains fixed code, prose, minified, long-line, multilingual, alias and sparse-filter judgments, cold/warm latency distributions, usage enabled/disabled trials, four concurrent processes and available process counters. First-query latency means a new MCP process; operating-system caches are uncontrolled. The original static vectors establish adapter behavior only. Held-out judgments live outside the indexed source fixture; this is not an evaluation of learned semantic quality. Hosted native CI retains a receipt for each platform's exact binary. See [the implementation record](airkimprovements.md) and [release notes](release-1.1.4.md) for what actually ran.
