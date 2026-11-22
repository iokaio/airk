# Authenticated HTTP read sessions

Owner: `airk-mcp::remote::http` (`HttpReadService`, `BoundedSessions`). The
HTTPS gateway (`airk-https` `http_gateway.rs`) authenticates every request,
mints the transport read-attempt context and bounds each response stream with
its `OutputBody` (32 MiB output budget, 120 s or, for a call marked an Answer,
Answer's ceiling).

## Session lease

- Sessions are bounded globally (`HttpSessionBudget`) and per credential
  (`HttpReadLimits::sessions_per_client`). A new session must finish its
  handshake within `handshake_timeout`; after that it idles out
  `idle_timeout` (300 s by default) after its last message.
- Idle expiry is this manager's lease (`expire`, `touch`), not rmcp's
  `keep_alive`, which is off: rmcp closed a session after the idle timeout
  even while an Answer ran with no message. Each open response stream holds an
  `InFlight` guard. A session with one never idles out; the lease is renewed
  and restarts when the last stream ends.

## SSE keep-alive

- Production streams carry rmcp's SSE comment keep-alive every 15 s
  (`SSE_KEEP_ALIVE`, rmcp's default, now named). It is produced only as the
  body is polled, so a client that stops reading applies backpressure, and
  the gateway's `OutputBody` bounds the bytes. On the real clock it adds at
  most one comment per 15 s. No production path collects a streamed body
  without a bound.
- `sse_stream` resets that timer from `std::time::Instant::now()`, while
  `tokio::time` under `start_paused` runs ahead of the real clock. After a
  paused-time jump the timer deadline is always in Tokio's past, so it fires
  in a tight loop until real time catches up. Tests that jump paused time past
  the keep-alive build the service with `HttpReadService::with_sse_keep_alive(
  .., None)` and read bodies under a byte bound.

## Fault record: idle-timeout test exhausted memory (22 November 2026)

- **Fault:** `remote::http::tests::an_answer_in_flight_keeps_its_http_session_past_the_idle_timeout`
  (added in `f5451e2a`) aborted after about 280 s with "memory allocation of
  137438953472 bytes failed" (exit 0xc0000409), alone and with
  `airk-https`. Under L2 it had passed, but only after 445 to 463 s.
- **Root cause (test only):** the 400 s paused sleep in the fixture made
  rmcp's real-clock SSE keep-alive emit pings in a tight loop for about 385 s
  of real time, and the test read the body with an unbounded `collect()`,
  whose buffer kept doubling. Production is not affected: real time keeps the
  two clocks together and the gateway bounds and back-pressures the stream.
- **Fix:** the keep-alive is a constructor parameter (`with_sse_keep_alive`,
  private); `HttpReadService::new` passes the unchanged 15 s. Tests of other
  crates get the same service without it through
  `HttpReadService::without_sse_keep_alive`, which exists only with
  `test-support`. The test turns it
  off, which also makes it the stricter case (no message at all for 400 s),
  and reads the body through `http_body_util::Limited` (64 KiB), so a
  returning ping loop fails in milliseconds instead of exhausting memory.
  The test's assertions are unchanged.
- **Evidence:** private receipts under `answer-loop\mcp-idle` (logs listed in
  `docs/arbiter/answer-loop-changes.md` entry 18).

## Closed gap: a live HTTPS Answer past 120 s (L2 deviation 10, 22 November 2026)

- **Fault:** nothing showed that a live TLS connection keeps an Answer open
  past 120 s to its result. Two bounds apply on that connection:
  - the gateway's `OutputBody` stream bound;
  - the connection fence's IO deadline, checked at every socket write.

  Both are widened only after rmcp has returned the event-stream head. The
  one end-to-end test held for 2 s, and a test on `OutputBody` alone did not
  reach the fence. The fence also read the system clock while `OutputBody`
  read Tokio's, so under paused time the IO deadline could not be exercised
  at all.
- **Change:**
  - The fence's IO deadline now reads Tokio's clock (`http_gateway.rs`
    `io_now`), the same clock as the stream bound it is widened and
    tightened from. Outside paused tests this is the system monotonic
    clock, so production behaviour is unchanged.
  - The per-connection serve is extracted into one function
    (`Gateway::serve_accepted`), shared by the listener and the test.
  - Test builds of the gateway can turn rmcp's real-clock keep-alive off.
- **Evidence:**
  `http_gateway::tests::a_live_tls_connection_keeps_an_answer_open_past_120_seconds_to_its_result`
  covers the real per-connection serve, TLS and HTTP/1.1 over an in-memory
  socket under paused time.
  - The response head arrives within 1 s, so the stream bound and the IO
    deadline are both set before the 120 s mark.
  - An Answer held for 130 s then delivers its progress and its result
    after at least 130 s.
  - Every body read is bounded at 256 KiB, with a 300 s timeout.
  - **Mutation check:** with the fence widening
    (`ConnectionFence::widen_output_deadline`) disabled, the test fails with
    "error reading a body from connection". So it depends on the IO deadline
    as well as on the stream bound.
  - A search is no control for this test. Without progress, rmcp sends a
    search's head only with its result, so the search's stream bound starts
    at that point.
  - Receipts: `answer-loop\L2b` (`nextest-g2-live`,
    `nextest-g3-mutation-no-fence-widen`). The change log is entry 18.
