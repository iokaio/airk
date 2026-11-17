# Early CUDA math process default

Owner: the synchronous airk binary entry, startup_math. This bootstrap is the first executable operation in main, before run_cli, bundled verification/ORT paths, the CLI worker thread, Tokio runtime, role admission and every service/login/foreground entry. Source main currently has no application initialization before it. Windows process-local environment updates are thread-safe; the stronger entry-only call order prevents observing/changing the value after any AIRK ORT or worker initialization. The private unsafe function has exactly this caller safety obligation. It must never be moved into an async service handler or called later to repair an initialized process.

Only Windows builds with airk-cli/gpu-cuda invoke it. If ORT_CUDA_GEMM_OPTIONS is absent, set this process's value to exact2. Preserve every explicit OsString unchanged, including2, conflicts, empty and non-Unicode values. Do not set ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO. No parent process, account/OS/registry environment or service-manager definition is changed. CPU/non-CUDA/non-Windows builds do not read or write these settings.

Child overlays alone miss the existing direct Task Scheduler/login, managed-service and explicit foreground entries, which serve in the process the OS/user starts. The common synchronous entry covers them without another process, role, wrapper or public request field. Existing role/image/I4/same-user/start/Stop authority is unchanged; environment initialization grants none.

Retrieval remains read-only: it captures the initial value before ORT and checks immutable GEMM2 for G5. Explicit conflicting values are not silently repaired; existing native F16 refusal remains. Qualified product debug logging stays optional; scientific proof owners explicitly supply debug1. This default is not a GPU/model/precision/fit/qualification claim. Existing FP32/CPU math policies and MiniLM are unchanged.

Pure tests inject read/write functions, exercise absent/explicit/empty/non-Unicode and disabled-target decisions, and prove idempotence without mutating the test process environment. Native artifact/login qualification remains Root-owned and separate.
