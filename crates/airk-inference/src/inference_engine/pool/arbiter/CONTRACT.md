# Optional native score completion ownership

Owner: `airk-inference::inference_engine::pool::arbiter`. Recorded under the
advanced-split authorization. This is a private Rust error/measurement contract;
it adds no wire, persisted format, task, model, deadline or execution authority.

The one owned lane retains each job's actual resident charge, source pins and
compute/delivery lifetime through its real synchronous score return. A canceled
consumer cannot make that return happen early. The compiled receipt is emitted
immediately after successful compilation, before post-compile token/deadline/
authority admission checks; those checks still precede every native score call.

`ScoreReceipt` contains only the exact compiled input digest/token count and
known queue, compile and score elapsed microseconds. `KnownScoreFailure` carries
that receipt, a `source_delivery_refused` flag, and the original private error
cause. It is created only after the score call has actually returned, whether
successfully or with an error. The successful `Scored` shape is unchanged. The
existing score timing start is preserved, including its surrounding admission
cost; these measurements do not newly assert that an FFI dispatch happened when
a scorer rejected input before entering its native implementation.

A post-score source/configuration/queue delivery refusal retains the same known
receipt on both successful and failed score returns; scores are withheld. Such
a receipt supplies accounting, never renewed source permission or authority to
act on the rejected result. Service callers may downcast `KnownScoreFailure` to
retain its measurements before fallible ledger/policy reconciliation. Actual
source authority and the outer native delivery gate remain fail closed.

A pre-dispatch deadline/cancellation or an undelivered/closed completion channel
cannot fabricate a completed receipt. Missing completion means uncertainty, not
zero work. Retaining known work does not refund an uncertain reservation, permit
replay, invoke another score, or release an actual resident/native charge early.
The lane still drops delivery/job ownership in its existing order after the real
call and final delivery check. Static error display does not include evidence;
the original internal cause remains available to the owning native consumer.

Cheap controls inject only the clock and score-return seam used by production.
They cover successful and failed known completion, post-score revocation of
both outcomes, retained causes, and canceled/expired-before-dispatch absence of
completion. They are not model, FFI, platform, or resource qualification.
