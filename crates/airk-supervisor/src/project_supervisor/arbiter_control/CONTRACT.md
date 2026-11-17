# Native optional compute management

The weak management port requests the existing inference owner; it never starts
an independent process/lane or owns a second factory. Its internal envelope
carries typed legacy3 or device4 NativeConfiguration, preserving the exact
original caller deadline for device readiness. The resource actor alone reuses
an actual resident charge, checking exact measured extra host demand, current
resource policy and fresh OS headroom. It does not subtract resident allocations
from actual free memory a second time. Historical callers retain conservative
full-peak reuse. Neither serialized metadata nor this policy view creates an
ExecutionProfile, native ServiceLease, runtime/device witness or capacity claim.
