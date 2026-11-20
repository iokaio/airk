# A6000 capacity sweep

The Windows capacity observer holds touched CUDA device allocations in their own suspended-before-resume Windows Job. It verifies the Python executable and runtime, System32 NVIDIA driver and DriverStore implementation, NVIDIA UUID, CUDA device LUID and physical WDDM adapter. Each allocation is at most 256 MiB. The worker synchronizes every touch, retains its allocations without GPU traffic during measurement, frees them and releases the CUDA context before the Job exits. A fresh output directory retains the exact source hashes, command, bounds, WDDM/host observations and natural release evidence.

In the verified human shell, run the CPU controls first.

```powershell
python -I -B -S scripts/cuda-component-execution/test_capacity_ballast.py
```

The next step uses the assigned GPU slot and a reviewed fresh output path. It runs a 64 MiB model-free touch, hold and release and takes at least one second of baseline and held samples. Pass an absolute, fresh direct campaign child whose name starts with `compact-capacity-`.

```powershell
python -I -B -S scripts/cuda-component-execution/capacity_ballast.py --small-model-free --output $absoluteFreshProofPath
```

The separate `--commit-calibration` selector touches exactly 2 GiB without a model under a 4 GiB host Job. It samples Windows system commit and available physical memory before and after the Job, each chunk's process and Job commit peaks, and WDDM dedicated and shared residency during the hold. It requires at least 8 GiB of spare physical and commit capacity beyond the pending allocation. The selector produces no floor admission.

After reviewing that receipt, use `capacity_sweep.py qkv-fit-probe` with `--tier 8` or `--tier 12`, explicit `--maximum-gib`, `--output`, the small proof path and hash, probe source SHA-256, exact newly sealed inference image and build receipt. The remaining available tiers are 6, 16 and 24 GiB. The worker's allocation ceiling may be lower than the maximum allowed for the tier. It refuses any required allocation beyond that ceiling.

```powershell
python -I -B -S scripts/cuda-component-execution/capacity_sweep.py qkv-fit-probe --tier 8 --maximum-gib 40 --output $absoluteFreshTierPath --small-proof $modelFreeReceipt --small-proof-sha256 $modelFreeDigest --probe-source-sha256 $reviewedProbeDigest --image $image --image-sha256 $imageDigest --build-receipt $buildReceipt --build-receipt-sha256 $buildDigest
```

A fresh simultaneous NVIDIA/WDDM observation gates the native test immediately before its original suspended process resumes. All three recorded free-space values must be at least the selected floor. The tighter contemporaneous NVIDIA/WDDM estimate must be within 128 MiB above the floor. CUDA free bytes are sampled after the ballast touch. The candidate retains its original 120-second wall, 32 GiB process and aggregate host caps, and 24 GiB device cap. The ballast has a separate 2 GiB host Job limit and an absolute 300-second cutoff.

The capacity observer samples the original ballast handle approximately every 100 ms through release and refuses stale or interrupted measurements. The native probe separately retains its own process samples. Numeric completion, verified local memory and admitted floor are distinct fields. Nonzero candidate shared usage remains unresolved without allocation-level attribution. The observers measure this 48 GiB A6000 under pressure and make no physical smaller-GPU equivalence claim.

A later `qkv-resource20` invocation uses the same switches plus `--admitted-fit` and `--admitted-fit-sha256`. It requires a successful single T8192/N8 run at the exact tier on the same native image, build, probe and verified candidate residency. Pressure runs require primary review and the exclusive GPU slot. Each failure retains its original receipt. On a CUDA allocation refusal, the worker records the requested allocation, free bytes immediately before it, and the number and size of successful allocations. Its release record and exit status let the controller retain cleanup evidence even when the candidate never starts.

## Protected Compact attention

Run `protected_capacity_sweep.py pressure-fit --tier N --output <fresh compact-capacity-* directory> --concurrent-host-bytes N --probe-source-sha256 SHA --graph-receipt PATH --graph-sha256 SHA --lineage PATH --lineage-sha256 SHA` with the same isolated Windows interpreter. The graph receipt and Cargo lineage must belong to the currently preserved pressure-capable native image. The controller reopens the original four-width score controls and the model-free ballast proofs. The candidate must reproduce the exact eight T8192 output words.

Reserve all other live Jobs' remaining physical and commit capacity in `--concurrent-host-bytes`. The controller adds the candidate's 16 GiB host Job, the separate ballast envelope of at most the requested allocation plus 2 GiB, and an 8 GiB spare margin. The measured 2 GiB allocation control supplies the original host-commit evidence. Each allocation checks remaining physical and commit headroom; the ballast worker observes its original Job caps. The observed GPU tier and selected provider arena are recorded separately. Existing control modes retain their original limits.

The protected worker requests fresh NVIDIA and WDDM observations on its original adapter after CUDA context initialization and after each synchronized increment of at most 2 GiB. The parent retains each source snapshot, monotonic sampling interval and touched-byte count. The worker checks the nonce, observation age and sequence, rereads CUDA free memory, and allocates in chunks of at most 256 MiB. Every chunk retains its original CUDA and host-reserve checks. The controller preserves the pre-resume measurements even on a refused floor.

The parent verifies all three free-memory observations immediately before resuming the candidate, and binds them to the exact suspended PID, image, typed graph, adapter, nonce and absolute expiry. The native scorer rereads that grant and the actual CUDA free memory after context initialization. It can refuse when the context consumes the remaining room. A successful score, physical residency and unresolved shared usage have separate fields. Keep the original raw process counters. For an admitted tier run `pressure-fixed` with `--prior-fit PATH --prior-fit-sha256 SHA` and the same typed graph, executable, probe and tier. The fixed workload excludes one warmup and measures 20 T8192 forwards. These observations describe only the A6000 under measured pressure.

### Windows shared-memory observations

The corrected 24 GiB experiment reproduced all eight original T8192 score words in a 1.872-second synchronized call. The ballast freed 88 of 88 allocations and both Windows Jobs exited naturally with zero members. Before candidate resume, NVIDIA reported 25,880,952,832 bytes free, WDDM reported 25,866,633,216 bytes and the ballast CUDA context reported 26,280,460,288 bytes. The candidate's own CUDA context subsequently reported 49,886,003,200 bytes free. The different contexts supply distinct measurements. See the private `compact-capacity-protected-24-gib-pressure-fit-r1/execution.json` receipt, SHA-256 `dd82bcac500fb4734dfeaa170dd31a2bcb4f6f710b3acbe53d0cb6ecc98dc0f1`.

The observed candidate shared peak was 79,691,776 bytes, with 19 unknown samples including the suspended baseline. It first reached that value about 3.39 seconds before the forward and stayed there as dedicated usage grew to 6,706,245,632 bytes. The same-image graph-only session and unpressured fixed and mixed timing runs also peaked at 79,691,776 shared bytes. A separate model-free CUDA context holding a 2 GiB allocation peaked at 77,594,624 bytes. The counters expose process totals. Attribution to a model allocation or particular paging operation remains unknown, so the 24 GiB fit status is `observed_shared_usage` and the corresponding fixed20 gate remains closed.

Use `python -I -B -S scripts/cuda-component-execution/windows_cuda_sysmem_policy.py` to repeat the read-only profile query for the pinned Windows executable and NVIDIA library. It uses public NVAPI Driver Settings calls to look up the full executable, inspect the effective profile and enumerate public setting names. The original query found no application-specific profile or public system-memory fallback setting among 125 available settings on driver 597.06. Its result is in `windows-exact-cuda-fallback-r1/execution.json`, SHA-256 `16e2d607480d89f022a40d39418f30fb51bb0aea13cefe57aa2a3da5f4ed817b`. A driver preference and a process's actual paging behavior require separate evidence. [NVIDIA's per-executable setting](https://nvidia.custhelp.com/app/answers/detail/a_id/5490) · [NVIDIA public DRS API](https://github.com/NVIDIA/nvapi/blob/main/nvapi.h)

The SDK declares a process/segment eviction field in `D3DKMT_QUERYSTATISTICS_PROCESS_SEGMENT_INFORMATION`. Microsoft's documentation reserves that structure for system use. The documented DXGI video memory API reports usage, reservation and budget per local/nonlocal segment group and does not identify the backing of individual CUDA tensors. [Process segment statistics](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/d3dkmthk/ns-d3dkmthk-d3dkmt_querystatistics_process_segment_information) · [DXGI memory information](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_4/ns-dxgi1_4-dxgi_query_video_memory_info)
