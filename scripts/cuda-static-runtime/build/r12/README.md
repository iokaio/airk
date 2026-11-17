# Original R12 corrected-source continuation

continue_cuda_runtime.py, producer_environment.py, process_tree.py and
wait_process_tree.ps1 retain the corrected CAPI continuation, scrubbed child
environment and complete process-tree/Job settlement logic. The exact source
patch is included. Full before/after third-party source copies, actual run
requests, failed receipts, diagnostic records and warm state remain data.

R12 was an explicitly bounded continuation of a failed run, not a replacement
cold-build pass. Its original cutoff and failures must stay historical. A new
run must bind fresh source pins, inputs and deadline through the reviewed
manifest; no current fresh-run launcher is provided. Use ../rebuild.py only for
read-only input validation.
