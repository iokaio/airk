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
